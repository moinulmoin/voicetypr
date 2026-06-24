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
                    if cleaned.trim().is_empty() {
                        return Err(AiProviderError::BadResponse);
                    }
                    return Ok(AiPolishResult {
                        output_text: cleaned,
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

/// Sanitize a model's cleanup response before it is returned for auto-typing.
///
/// Drops control characters except `\n` and `\t` (carriage returns collapse to
/// `\n`) and the bidirectional-formatting controls (Unicode `Cf`) used in
/// Trojan-Source-style injection, then enforces a length ceiling relative to
/// the input so a runaway model cannot dump unbounded text at the cursor.
fn sanitize_ai_output(output: &str, input_byte_len: usize) -> String {
    // 4x covers normal cleanup/translation; the floor keeps short inputs (whose
    // cleaned form can be several times larger) from being clipped.
    const MIN_OUTPUT_CAP: usize = 4096;
    let cap = input_byte_len.saturating_mul(4).max(MIN_OUTPUT_CAP);

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
