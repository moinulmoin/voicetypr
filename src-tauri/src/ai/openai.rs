use super::config::*;
use super::{prompts, AIEnhancementRequest, AIEnhancementResponse, AIError, AIProvider};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

pub struct OpenAIProvider {
    api_key: String,
    model: String,
    base_url: String,
    options: HashMap<String, serde_json::Value>,
}

pub(crate) fn model_uses_max_completion_tokens(model: &str) -> bool {
    let normalized = model.trim().to_ascii_lowercase();
    normalized.starts_with("gpt-5")
        || normalized.starts_with("o1")
        || normalized.starts_with("o3")
        || normalized.starts_with("o4")
}

pub(crate) fn is_unsupported_token_parameter_error(
    error_text: &str,
    parameter_name: &str,
) -> bool {
    let haystack = error_text.to_ascii_lowercase();
    let parameter = parameter_name.to_ascii_lowercase();
    let unsupported_single = format!("unsupported parameter: '{}'", parameter);
    let unsupported_double = format!("unsupported parameter: \"{}\"", parameter);
    let param_field = format!("\"param\":\"{}\"", parameter);

    haystack.contains(&unsupported_single)
        || haystack.contains(&unsupported_double)
        || haystack.contains(&param_field)
}

fn is_unsupported_temperature_value_error(error_text: &str) -> bool {
    let haystack = error_text.to_ascii_lowercase();
    haystack.contains("unsupported value")
        && haystack.contains("temperature")
        && (haystack.contains("default") || haystack.contains("only"))
}

impl OpenAIProvider {
    pub fn new(
        api_key: String,
        model: String,
        mut options: HashMap<String, serde_json::Value>,
    ) -> Result<Self, AIError> {
        // Do not restrict model IDs; accept any OpenAI-compatible model string

        // Determine if auth is required
        let no_auth = options
            .get("no_auth")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Validate API key format (basic check) only if auth is required
        if !no_auth && (api_key.trim().is_empty() || api_key.len() < MIN_API_KEY_LENGTH) {
            return Err(AIError::ValidationError(
                "Invalid API key format".to_string(),
            ));
        }

        // Resolve base URL: expect versioned base (e.g., https://api.openai.com/v1) and append only /chat/completions
        let base_root = options
            .get("base_url")
            .and_then(|v| v.as_str())
            .unwrap_or("https://api.openai.com/v1");
        let base_trim = base_root.trim_end_matches('/');
        let base_url = format!("{}/chat/completions", base_trim);

        // Ensure the normalized values are kept in options for downstream if needed
        options.insert(
            "base_url".into(),
            serde_json::Value::String(base_root.to_string()),
        );

        Ok(Self {
            api_key,
            model,
            base_url,
            options,
        })
    }

    fn create_http_client() -> Result<Client, AIError> {
        Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .map_err(|e| AIError::NetworkError(format!("Failed to create HTTP client: {}", e)))
    }

    async fn make_request_with_retry(
        &self,
        request: &OpenAIRequest,
    ) -> Result<OpenAIResponse, AIError> {
        let mut last_error = None;

        for attempt in 1..=MAX_RETRIES {
            match self.make_single_request(request).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    log::warn!("API request attempt {} failed: {}", attempt, e);
                    last_error = Some(e);

                    if attempt < MAX_RETRIES {
                        tokio::time::sleep(Duration::from_millis(
                            RETRY_BASE_DELAY_MS * attempt as u64,
                        ))
                        .await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| AIError::NetworkError("Unknown error".to_string())))
    }

    async fn make_single_request(
        &self,
        request: &OpenAIRequest,
    ) -> Result<OpenAIResponse, AIError> {
        // Determine if auth header should be sent
        let no_auth = self
            .options
            .get("no_auth")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let client = Self::create_http_client()?;

        let mut req = client
            .post(&self.base_url)
            .header("Content-Type", "application/json")
            .json(request);

        if !no_auth {
            req = req.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let response = req
            .send()
            .await
            .map_err(|e| AIError::NetworkError(e.to_string()))?;

        let status = response.status();

        if status.as_u16() == 429 {
            return Err(AIError::RateLimitExceeded);
        }

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AIError::ApiError(format!(
                "API returned {}: {}",
                status, error_text
            )));
        }

        response
            .json()
            .await
            .map_err(|e| AIError::InvalidResponse(e.to_string()))
    }
}

#[derive(Serialize, Clone)]
struct OpenAIRequest {
    model: String,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
    /// For GPT-5 models: set reasoning effort to minimal for fast text formatting
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OpenAIResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[async_trait]
impl AIProvider for OpenAIProvider {
    async fn enhance_text(
        &self,
        request: AIEnhancementRequest,
    ) -> Result<AIEnhancementResponse, AIError> {
        request.validate()?;

        let prompt = prompts::build_enhancement_prompt(
            &request.text,
            request.context.as_deref(),
            &request.options.unwrap_or_default(),
            request.language.as_deref(),
        );

        let temperature = self
            .options
            .get("temperature")
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .unwrap_or(DEFAULT_TEMPERATURE);

        let max_tokens = self
            .options
            .get("max_tokens")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);

        let use_max_completion_tokens = model_uses_max_completion_tokens(&self.model);

        // For GPT-5 models, use minimal reasoning effort for fast text formatting
        let reasoning_effort = if self.model.starts_with("gpt-5") {
            Some("minimal".to_string())
        } else {
            None
        };

        let mut request_body = OpenAIRequest {
            model: self.model.clone(),
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: "You are a careful text formatter that only returns the cleaned text per the provided rules.".to_string(),
                },
                Message {
                    role: "user".to_string(),
                    content: prompt,
                },
            ],
            temperature: if use_max_completion_tokens {
                None
            } else {
                Some(temperature.clamp(0.0, 2.0))
            },
            max_tokens: if use_max_completion_tokens {
                None
            } else {
                max_tokens
            },
            max_completion_tokens: if use_max_completion_tokens {
                max_tokens
            } else {
                None
            },
            reasoning_effort,
        };

        let mut api_response = None;
        let mut attempts = 0;
        while attempts < 3 {
            attempts += 1;

            match self.make_request_with_retry(&request_body).await {
                Ok(response) => {
                    api_response = Some(response);
                    break;
                }
                Err(AIError::ApiError(error_text)) => {
                    if request_body.temperature.is_some()
                        && is_unsupported_temperature_value_error(&error_text)
                    {
                        log::warn!(
                            "OpenAI temperature override unsupported for model '{}'; retrying with default temperature",
                            self.model
                        );
                        request_body.temperature = None;
                        continue;
                    }

                    if max_tokens.is_some() {
                        let attempted_param = if request_body.max_completion_tokens.is_some() {
                            "max_completion_tokens"
                        } else {
                            "max_tokens"
                        };

                        if is_unsupported_token_parameter_error(&error_text, attempted_param) {
                            log::warn!(
                                "OpenAI token parameter '{}' unsupported for model '{}'; retrying with alternate parameter",
                                attempted_param,
                                self.model
                            );
                            std::mem::swap(
                                &mut request_body.max_tokens,
                                &mut request_body.max_completion_tokens,
                            );
                            continue;
                        }
                    }

                    return Err(AIError::ApiError(error_text));
                }
                Err(error) => return Err(error),
            }
        }

        let api_response = api_response.ok_or_else(|| {
            AIError::ApiError("Failed OpenAI request after compatibility retries".to_string())
        })?;

        let enhanced_text = api_response
            .choices
            .first()
            .ok_or_else(|| AIError::InvalidResponse("No choices in response".to_string()))?
            .message
            .content
            .trim()
            .to_string();

        if enhanced_text.is_empty() {
            return Err(AIError::InvalidResponse(
                "Empty response from API".to_string(),
            ));
        }

        Ok(AIEnhancementResponse {
            enhanced_text,
            original_text: request.text,
            provider: self.name().to_string(),
            model: self.model.clone(),
        })
    }

    fn name(&self) -> &str {
        "openai"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let result = OpenAIProvider::new("".to_string(), "gpt-5-nano".to_string(), HashMap::new());
        assert!(result.is_err());

        let result = OpenAIProvider::new(
            "test_key_12345".to_string(),
            "gpt-unknown".to_string(),
            HashMap::new(),
        );
        // Unknown model should be allowed with a warning (not hard error)
        assert!(result.is_ok());

        let result = OpenAIProvider::new(
            "test_key_12345".to_string(),
            "gpt-5-nano".to_string(),
            HashMap::new(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_model_uses_max_completion_tokens() {
        assert!(model_uses_max_completion_tokens("gpt-5"));
        assert!(model_uses_max_completion_tokens("gpt-5-mini"));
        assert!(model_uses_max_completion_tokens("o1"));
        assert!(model_uses_max_completion_tokens("o1-mini"));
        assert!(model_uses_max_completion_tokens("o3-mini"));
        assert!(model_uses_max_completion_tokens("o4-mini"));

        assert!(!model_uses_max_completion_tokens("gpt-4o"));
        assert!(!model_uses_max_completion_tokens("gpt-4.1"));
    }

    #[test]
    fn test_unsupported_token_parameter_error_detection() {
        let error = "Unsupported parameter: 'max_tokens' is not supported with this model. Use 'max_completion_tokens' instead.";
        assert!(is_unsupported_token_parameter_error(error, "max_tokens"));
        assert!(!is_unsupported_token_parameter_error(error, "max_completion_tokens"));
    }

    #[test]
    fn test_unsupported_temperature_value_error_detection() {
        let error = "Unsupported value: 'temperature' does not support 0 with this model. Only the default (1) value is supported.";
        assert!(is_unsupported_temperature_value_error(error));
        assert!(!is_unsupported_temperature_value_error(
            "Unsupported parameter: 'max_tokens'"
        ));
    }
}
