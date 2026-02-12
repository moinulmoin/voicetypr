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

#[derive(Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    /// For GPT-5 models: set reasoning effort to minimal for fast text formatting
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
}

#[derive(Serialize, Deserialize)]
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

        // For GPT-5 models, use minimal reasoning effort for fast text formatting
        let reasoning_effort = if self.model.starts_with("gpt-5") {
            Some("minimal".to_string())
        } else {
            None
        };

        let request_body = OpenAIRequest {
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
            temperature: Some(temperature.clamp(0.0, 2.0)),
            max_tokens,
            reasoning_effort,
        };

        let api_response = self.make_request_with_retry(&request_body).await?;

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
}
