use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod config;
pub mod gemini;
pub mod groq;
pub mod prompts;

pub use config::{MAX_CUSTOM_VOCABULARY, MAX_TEXT_LENGTH, MAX_VOCABULARY_TERM_LENGTH};
pub use prompts::EnhancementOptions;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIProviderConfig {
    pub provider: String,
    pub model: String,
    #[serde(skip_serializing)] // Don't serialize API key
    pub api_key: String,
    pub enabled: bool,
    #[serde(default)]
    pub options: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIEnhancementRequest {
    pub text: String,
    pub context: Option<String>,
    #[serde(default)]
    pub options: Option<EnhancementOptions>,
}

impl AIEnhancementRequest {
    pub fn validate(&self) -> Result<(), AIError> {
        if self.text.trim().is_empty() {
            return Err(AIError::ValidationError("Text cannot be empty".to_string()));
        }

        if self.text.len() > MAX_TEXT_LENGTH {
            return Err(AIError::ValidationError(format!(
                "Text exceeds maximum length of {} characters",
                MAX_TEXT_LENGTH
            )));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIEnhancementResponse {
    pub enhanced_text: String,
    pub original_text: String,
    pub provider: String,
    pub model: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AIError {
    #[error("API error: {0}")]
    ApiError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Provider not found: {0}")]
    ProviderNotFound(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,
}

#[async_trait]
pub trait AIProvider: Send + Sync {
    async fn enhance_text(
        &self,
        request: AIEnhancementRequest,
    ) -> Result<AIEnhancementResponse, AIError>;

    fn name(&self) -> &str;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIModel {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

pub struct AIProviderFactory;

impl AIProviderFactory {
    pub fn create(config: &AIProviderConfig) -> Result<Box<dyn AIProvider>, AIError> {
        // Validate provider name
        if !Self::is_valid_provider(&config.provider) {
            return Err(AIError::ProviderNotFound(config.provider.clone()));
        }

        match config.provider.as_str() {
            "groq" => Ok(Box::new(groq::GroqProvider::new(
                config.api_key.clone(),
                config.model.clone(),
                config.options.clone(),
            )?)),
            "gemini" => Ok(Box::new(gemini::GeminiProvider::new(
                config.api_key.clone(),
                config.model.clone(),
                config.options.clone(),
            )?)),
            provider => Err(AIError::ProviderNotFound(provider.to_string())),
        }
    }

    fn is_valid_provider(provider: &str) -> bool {
        matches!(provider, "groq" | "gemini")
    }
}
