use super::contract::{AiPolishRequest, AiPolishResult};
use super::error::{AiProviderError, MappedAiProviderError};
use super::genai_runtime::{AiKeyResolver, GenaiRuntime};
use super::openai_compatible::OpenAiCompatibleRuntime;
use super::providers::PROVIDER_CUSTOM;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct AiExecutor {
    genai_runtime: GenaiRuntime,
    custom_runtime: OpenAiCompatibleRuntime,
}

impl AiExecutor {
    pub fn new(
        http_client: reqwest::Client,
        key_resolver: AiKeyResolver,
        custom_base_url: String,
        custom_no_auth: bool,
    ) -> Self {
        Self::with_native_endpoint_overrides(
            http_client,
            key_resolver,
            custom_base_url,
            custom_no_auth,
            HashMap::new(),
        )
    }

    pub fn with_native_endpoint_overrides(
        http_client: reqwest::Client,
        key_resolver: AiKeyResolver,
        custom_base_url: String,
        custom_no_auth: bool,
        native_endpoint_overrides: HashMap<String, String>,
    ) -> Self {
        Self {
            genai_runtime: GenaiRuntime::with_endpoint_overrides(
                http_client.clone(),
                key_resolver.clone(),
                native_endpoint_overrides,
            ),
            custom_runtime: OpenAiCompatibleRuntime::new(
                http_client,
                key_resolver,
                custom_base_url,
                custom_no_auth,
            ),
        }
    }

    pub async fn polish(
        &self,
        request: AiPolishRequest,
        cancellation_token: CancellationToken,
    ) -> Result<AiPolishResult, AiProviderError> {
        let start = Instant::now();
        let budget = Duration::from_millis(request.timeout_ms);
        let deadline = start + budget;
        let mut attempt = 0_u8;

        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(AiProviderError::Timeout)?;
            let result = self
                .run_with_budget(&request, cancellation_token.clone(), remaining)
                .await;

            match result {
                Ok(output_text) => {
                    let cleaned = sanitize_ai_output(&output_text, request.input_text.len());
                    let validated = match validate_ai_output(&cleaned, &request.input_text) {
                        Ok(output) => output,
                        Err(error) if attempt == 0 => {
                            attempt += 1;
                            log::warn!(
                                "AI cleanup response failed validation; retrying once category={:?}",
                                error
                            );
                            continue;
                        }
                        Err(error) => return Err(error),
                    };
                    if validated.trim().is_empty() {
                        return Err(AiProviderError::BadResponse);
                    }
                    return Ok(AiPolishResult {
                        output_text: validated,
                        provider_id: request.provider_id,
                        model_id: request.model_id,
                        duration_ms: start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                    });
                }
                Err(mapped) if attempt == 0 && should_retry(&mapped.error) => {
                    attempt += 1;
                    if let Some(retry_after) = mapped.retry_after {
                        let remaining_after_sleep = deadline
                            .checked_duration_since(Instant::now())
                            .ok_or(AiProviderError::Timeout)?;
                        if retry_after >= remaining_after_sleep {
                            return Err(mapped.error);
                        }
                        tokio::select! {
                            _ = cancellation_token.cancelled() => return Err(AiProviderError::Canceled),
                            _ = tokio::time::sleep(retry_after) => {}
                        }
                    }
                }
                Err(mapped) => return Err(mapped.error),
            }
        }
    }

    async fn run_with_budget(
        &self,
        request: &AiPolishRequest,
        cancellation_token: CancellationToken,
        remaining: Duration,
    ) -> Result<String, MappedAiProviderError> {
        tokio::select! {
            biased;
            _ = tokio::time::sleep(remaining) => Err(MappedAiProviderError::new(AiProviderError::Timeout)),
            _ = cancellation_token.cancelled() => Err(MappedAiProviderError::new(AiProviderError::Canceled)),
            result = self.execute_once(request) => result,
        }
    }

    async fn execute_once(
        &self,
        request: &AiPolishRequest,
    ) -> Result<String, MappedAiProviderError> {
        if crate::ai::catalog::is_native_provider(&request.provider_id) {
            self.genai_runtime.polish(request).await
        } else if request.provider_id == PROVIDER_CUSTOM {
            self.custom_runtime.polish(request).await
        } else {
            Err(MappedAiProviderError::new(
                AiProviderError::UnsupportedProvider,
            ))
        }
    }
}

fn should_retry(error: &AiProviderError) -> bool {
    matches!(
        error,
        AiProviderError::RateLimited
            | AiProviderError::ServiceUnavailable
            | AiProviderError::Network
    )
}

fn validate_ai_output(output: &str, input: &str) -> Result<String, AiProviderError> {
    let cleaned = strip_wrapping_quotes(
        strip_known_preamble(strip_wrapping_quotes(
            strip_markdown_fence(output).trim(),
            input,
        )),
        input,
    )
    .trim()
    .to_string();

    if cleaned.is_empty()
        || starts_with_refusal_or_commentary(&cleaned)
        || has_anomalous_cleanup_length(&cleaned, input)
    {
        Err(AiProviderError::BadResponse)
    } else {
        Ok(cleaned)
    }
}

fn strip_markdown_fence(output: &str) -> &str {
    let trimmed = output.trim();
    if !trimmed.starts_with("```") {
        return trimmed;
    }

    let Some(first_newline) = trimmed.find('\n') else {
        return trimmed;
    };
    let body_and_close = &trimmed[first_newline + 1..];
    let Some(close_start) = body_and_close.rfind("```") else {
        return trimmed;
    };
    if body_and_close[close_start + 3..].trim().is_empty() {
        body_and_close[..close_start].trim()
    } else {
        trimmed
    }
}

fn strip_wrapping_quotes<'a>(output: &'a str, input: &str) -> &'a str {
    let trimmed = output.trim();
    let input_trimmed = input.trim();
    if is_wrapped_in_quotes(input_trimmed).is_some() {
        return trimmed;
    }

    match is_wrapped_in_quotes(trimmed) {
        Some((open_len, close_len)) => trimmed[open_len..trimmed.len() - close_len].trim(),
        None => trimmed,
    }
}

fn is_wrapped_in_quotes(text: &str) -> Option<(usize, usize)> {
    let pairs = [
        ('"', '"'),
        ('\'', '\''),
        ('\u{201c}', '\u{201d}'),
        ('\u{2018}', '\u{2019}'),
    ];
    pairs.iter().find_map(|(open, close)| {
        if text.starts_with(*open) && text.ends_with(*close) && text.len() > open.len_utf8() {
            Some((open.len_utf8(), close.len_utf8()))
        } else {
            None
        }
    })
}

fn strip_known_preamble(output: &str) -> &str {
    let Some((first_line, rest)) = output.split_once('\n') else {
        return output;
    };
    let normalized = first_line
        .trim()
        .trim_matches(|ch: char| ch == '"' || ch == '\'' || ch.is_whitespace())
        .to_ascii_lowercase();
    let normalized = normalized.trim_end_matches(':').trim();
    let known = [
        "here is the fixed text",
        "here's the fixed text",
        "here is the cleaned text",
        "here's the cleaned text",
        "here is the corrected text",
        "here's the corrected text",
        "here is the polished text",
        "here's the polished text",
        "sure, here is the fixed text",
        "sure, here's the fixed text",
        "sure, here is the cleaned text",
        "sure, here's the cleaned text",
        "sure, here is the corrected text",
        "sure, here's the corrected text",
        "sure, here is the polished text",
        "sure, here's the polished text",
        "sure",
    ];

    if known.contains(&normalized) {
        rest.trim()
    } else {
        output
    }
}

fn starts_with_refusal_or_commentary(output: &str) -> bool {
    let lower = output.trim_start().to_ascii_lowercase();
    lower.starts_with("i can't")
        || lower.starts_with("i cannot")
        || lower.starts_with("i'm sorry")
        || lower.starts_with("i am sorry")
}

fn has_anomalous_cleanup_length(output: &str, input: &str) -> bool {
    let input_len = input.trim().len();
    let output_len = output.trim().len();
    if input_len < 80 {
        output_len > 4096
    } else if input_len >= 256 {
        output_len >= input_len.saturating_mul(4).saturating_sub(32)
    } else {
        output_len > input_len.saturating_mul(12).max(4096)
    }
}

/// Sanitize a model's cleanup response before it is returned for auto-typing.
///
/// Drops control characters except `\n` and `\t` (carriage returns collapse to
/// `\n`) and the bidirectional-formatting controls (Unicode `Cf`) used in
/// Trojan-Source-style injection, then enforces a length ceiling relative to
/// the input so a runaway model cannot dump unbounded text at the cursor.
fn sanitize_ai_output(output: &str, input_byte_len: usize) -> String {
    // 4x covers normal cleanup/translation; the floor keeps short inputs (whose
    // cleaned form can be several times larger) from being clipped.
    let cap = input_byte_len
        .saturating_mul(4)
        .max(usize::try_from(super::contract::AI_OUTPUT_MIN_TOKEN_CAP).unwrap_or(4096) * 4);

    let mut sanitized = String::with_capacity(output.len().min(cap));
    let mut chars = output.chars().peekable();
    let mut truncated = false;
    while let Some(ch) = chars.next() {
        if sanitized.len() + ch.len_utf8() > cap {
            truncated = true;
            break;
        }
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            sanitized.push('\n');
        } else if ch == '\n' || ch == '\t' {
            sanitized.push(ch);
        } else if ch.is_control() || is_bidi_override(ch) {
            // Drop Cc control and Cf bidi-format characters.
        } else {
            sanitized.push(ch);
        }
    }
    if truncated {
        log::warn!(
            "AI cleanup output exceeded the {cap}-byte length ceiling; truncated before insertion"
        );
    }
    sanitized
}

/// Bidirectional-formatting controls (Unicode category `Cf`) with no legitimate
/// place in auto-typed prose: the Trojan-Source override set. Zero-width
/// joiners/non-joiners are intentionally excluded — they are load-bearing in
/// some scripts and emoji sequences, so stripping them would corrupt text.
fn is_bidi_override(ch: char) -> bool {
    matches!(
        ch,
        '\u{061C}'                          // ARABIC LETTER MARK
            | '\u{200E}' | '\u{200F}'       // LTR / RTL MARK
            | '\u{202A}'..='\u{202E}'       // LRE / RLE / PDF / LRO / RLO
            | '\u{2066}'..='\u{2069}'       // LRI / RLI / FSI / PDI
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_strips_known_preamble_followed_by_newline() {
        let output = "Here is the fixed text:\nMeet me at 4.";
        assert_eq!(
            validate_ai_output(output, "meet me at four").unwrap(),
            "Meet me at 4."
        );
    }

    #[test]
    fn validate_strips_markdown_fence() {
        let output = "```text\nMeet me at 4.\n```";
        assert_eq!(
            validate_ai_output(output, "meet me at four").unwrap(),
            "Meet me at 4."
        );
    }

    #[test]
    fn validate_strips_wrapping_quotes_when_input_was_not_quoted() {
        let output = "\"Meet me at 4.\"";
        assert_eq!(
            validate_ai_output(output, "meet me at four").unwrap(),
            "Meet me at 4."
        );
    }

    #[test]
    fn validate_preserves_wrapping_quotes_when_input_was_quoted() {
        let output = "\"Meet me at 4.\"";
        assert_eq!(
            validate_ai_output(output, "\"meet me at four\"").unwrap(),
            "\"Meet me at 4.\""
        );
    }

    #[test]
    fn validate_rejects_refusal_commentary() {
        let error = validate_ai_output("I'm sorry, I can't do that.", "hello").unwrap_err();
        assert!(matches!(error, AiProviderError::BadResponse));
    }

    #[test]
    fn validate_keeps_identity_output_unchanged() {
        let output = "Already clean.";
        assert_eq!(validate_ai_output(output, output).unwrap(), output);
    }
}
