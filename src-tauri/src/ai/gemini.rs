use super::config::*;
use super::{prompts, AIEnhancementRequest, AIEnhancementResponse, AIError, AIProvider};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

// Supported Gemini models
const SUPPORTED_MODELS: &[&str] = &["gemini-2.5-flash-lite"];

pub struct GeminiProvider {
    #[allow(dead_code)]
    api_key: String,
    model: String,
    client: Client,
    base_url: String,
    options: HashMap<String, serde_json::Value>,
}

impl GeminiProvider {
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
            base_url: "https://generativelanguage.googleapis.com/v1beta/models".to_string(),
            options,
        })
    }

    async fn make_request_with_retry(
        &self,
        request: &GeminiRequest,
    ) -> Result<GeminiResponse, AIError> {
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
        request: &GeminiRequest,
    ) -> Result<GeminiResponse, AIError> {
        let url = format!("{}/{}:generateContent", self.base_url, self.model);

        let response = self
            .client
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await
            .map_err(|e| AIError::NetworkError(e.to_string()))?;

        let status = response.status();

        // Handle rate limiting
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
struct GeminiRequest {
    contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
}

#[derive(Serialize, Deserialize)]
struct Content {
    parts: Vec<Part>,
}

#[derive(Serialize, Deserialize)]
struct Part {
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Vec<Candidate>,
}

#[derive(Deserialize)]
struct Candidate {
    content: Content,
}

#[async_trait]
impl AIProvider for GeminiProvider {
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

        let generation_config = GenerationConfig {
            temperature: Some(temperature.clamp(0.0, 2.0)),
            max_output_tokens: max_tokens,
        };

        let gemini_request = GeminiRequest {
            contents: vec![Content {
                parts: vec![Part { text: prompt }],
            }],
            generation_config: Some(generation_config),
        };

        let gemini_response = self.make_request_with_retry(&gemini_request).await?;

        let enhanced_text = gemini_response
            .candidates
            .first()
            .ok_or_else(|| AIError::InvalidResponse("No candidates in response".to_string()))?
            .content
            .parts
            .first()
            .ok_or_else(|| AIError::InvalidResponse("No parts in response".to_string()))?
            .text
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
        "gemini"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let result = GeminiProvider::new(
            "".to_string(),
            "gemini-1.5-flash".to_string(),
            HashMap::new(),
        );
        assert!(result.is_err());

        let result = GeminiProvider::new(
            "test_key_12345".to_string(),
            "invalid-model".to_string(),
            HashMap::new(),
        );
        assert!(result.is_err());

        let result = GeminiProvider::new(
            "test_key_12345".to_string(),
            "gemini-2.5-flash-lite".to_string(),
            HashMap::new(),
        );
        assert!(result.is_ok());
    }
}
