use super::config::*;
use super::{prompts, AIEnhancementRequest, AIEnhancementResponse, AIError, AIProvider};
use crate::utils::network_diagnostics::{
    log_api_request, log_api_response, log_network_error, log_network_error_with_duration,
    log_retry_attempt, NetworkError,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

// Supported models with validation
const SUPPORTED_MODELS: &[&str] = &["llama-3.1-8b-instant"];

pub struct GroqProvider {
    #[allow(dead_code)]
    api_key: String, // Keep but don't expose
    model: String,
    client: Client,
    base_url: String,
    options: HashMap<String, serde_json::Value>,
}

impl GroqProvider {
    pub fn new(
        api_key: String,
        model: String,
        options: HashMap<String, serde_json::Value>,
    ) -> Result<Self, AIError> {
        // Validate model
        if !SUPPORTED_MODELS.contains(&model.as_str()) {
            return Err(AIError::ValidationError(format!(
                "Unsupported model: {}",
                model
            )));
        }

        // Validate API key format (basic check)
        if api_key.trim().is_empty() || api_key.len() < MIN_API_KEY_LENGTH {
            return Err(AIError::ValidationError(
                "Invalid API key format".to_string(),
            ));
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .map_err(|e| AIError::NetworkError(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            api_key,
            model,
            client,
            base_url: "https://api.groq.com/openai/v1/chat/completions".to_string(),
            options,
        })
    }

    async fn make_request_with_retry(
        &self,
        request: &GroqRequest,
    ) -> Result<GroqResponse, AIError> {
        let mut last_error = None;

        for attempt in 1..=MAX_RETRIES {
            if attempt > 1 && log::log_enabled!(log::Level::Info) {
                log_retry_attempt("groq_api_request", attempt as u32, MAX_RETRIES as u32);
            }

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

    async fn make_single_request(&self, request: &GroqRequest) -> Result<GroqResponse, AIError> {
        let request_start = Instant::now();
        let response = self
            .client
            .post(&self.base_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await
            .map_err(|e| {
                let elapsed = request_start.elapsed().as_millis() as u64;
                let error = NetworkError::Unknown {
                    message: e.to_string(),
                };
                log_network_error_with_duration(error, Some(elapsed));
                AIError::NetworkError(e.to_string())
            })?;

        let status = response.status();
        let duration_ms = request_start.elapsed().as_millis() as u64;

        // Log response metrics (only if logging is enabled)
        if log::log_enabled!(log::Level::Info) {
            log_api_response(
                "groq",
                "POST",
                &self.base_url,
                status.as_u16(),
                duration_ms,
                None,
            );
        }

        // Handle rate limiting
        if status.as_u16() == 429 {
            if log::log_enabled!(log::Level::Error) {
                let error = NetworkError::RateLimited { retry_after: None };
                log_network_error(error);
            }
            return Err(AIError::RateLimitExceeded);
        }

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            // Log specific error types (only if logging is enabled)
            if log::log_enabled!(log::Level::Error) {
                let error = if status.as_u16() == 401 {
                    NetworkError::AuthenticationFailed {
                        provider: "groq".to_string(),
                    }
                } else {
                    NetworkError::Unknown {
                        message: format!("Status {}: {}", status, error_text),
                    }
                };
                log_network_error(error);
            }

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

#[derive(Serialize)]
struct GroqRequest {
    model: String,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct GroqResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[async_trait]
impl AIProvider for GroqProvider {
    async fn enhance_text(
        &self,
        request: AIEnhancementRequest,
    ) -> Result<AIEnhancementResponse, AIError> {
        // Validate request
        request.validate()?;

        let prompt = prompts::build_enhancement_prompt(
            &request.text,
            request.context.as_deref(),
            &request.options.unwrap_or_default(),
            request.language.as_deref(),
        );

        // Log API request details (only if logging is enabled)
        if log::log_enabled!(log::Level::Info) {
            log_api_request("groq", &self.model, prompt.len());
        }

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

        let groq_request = GroqRequest {
            model: self.model.clone(),
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt,
            }],
            temperature: Some(temperature.clamp(0.0, 2.0)), // Clamp to valid range
            max_tokens,
        };

        let groq_response = self.make_request_with_retry(&groq_request).await?;

        let enhanced_text = groq_response
            .choices
            .first()
            .ok_or_else(|| AIError::InvalidResponse("No choices in response".to_string()))?
            .message
            .content
            .trim()
            .to_string();

        // Validate that we got a reasonable response
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
        "groq"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let result = GroqProvider::new(
            "".to_string(),
            "llama-3.1-8b-instant".to_string(),
            HashMap::new(),
        );
        assert!(result.is_err());

        let result = GroqProvider::new(
            "test_key_12345".to_string(),
            "invalid-model".to_string(),
            HashMap::new(),
        );
        assert!(result.is_err());

        let result = GroqProvider::new(
            "test_key_12345".to_string(),
            "llama-3.1-8b-instant".to_string(),
            HashMap::new(),
        );
        assert!(result.is_ok());
    }
}
