use super::config::*;
use super::{prompts, AIEnhancementRequest, AIEnhancementResponse, AIError, AIProvider};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

pub struct AnthropicProvider {
    api_key: String,
    model: String,
    client: Client,
    options: HashMap<String, serde_json::Value>,
}

impl AnthropicProvider {
    pub fn new(
        api_key: String,
        model: String,
        options: HashMap<String, serde_json::Value>,
    ) -> Result<Self, AIError> {
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
            options,
        })
    }

    async fn make_request_with_retry(
        &self,
        request: &AnthropicRequest,
    ) -> Result<AnthropicResponse, AIError> {
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
        request: &AnthropicRequest,
    ) -> Result<AnthropicResponse, AIError> {
        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(request)
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

/// Config to disable extended thinking for fast text formatting
#[derive(Serialize)]
struct ThinkingConfig {
    /// Set to 0 to disable extended thinking
    budget_tokens: u32,
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    /// Disable extended thinking for simple text formatting tasks
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<ThinkingConfig>,
}

#[derive(Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

#[async_trait]
impl AIProvider for AnthropicProvider {
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
            .map(|v| v as u32)
            .unwrap_or(4096);

        let request_body = AnthropicRequest {
            model: self.model.clone(),
            max_tokens,
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt,
            }],
            system: Some("You are a careful text formatter that only returns the cleaned text per the provided rules.".to_string()),
            temperature: Some(temperature.clamp(0.0, 1.0)),
            // Omit thinking parameter to disable extended thinking for fast text formatting
            thinking: None,
        };

        let api_response = self.make_request_with_retry(&request_body).await?;

        let enhanced_text = api_response
            .content
            .iter()
            .filter(|c| c.content_type == "text")
            .filter_map(|c| c.text.as_deref())
            .collect::<Vec<_>>()
            .join("")
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
        "anthropic"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let result = AnthropicProvider::new("".to_string(), "claude-haiku-4-5".to_string(), HashMap::new());
        assert!(result.is_err());

        let result = AnthropicProvider::new(
            "test_key_12345".to_string(),
            "claude-haiku-4-5".to_string(),
            HashMap::new(),
        );
        assert!(result.is_ok());
    }
}
