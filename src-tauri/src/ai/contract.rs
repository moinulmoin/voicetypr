use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProvider {
    pub id: String,
    pub label: String,
    pub requires_api_key: bool,
    pub supports_base_url: bool,
    pub supports_reasoning: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiModel {
    pub provider_id: String,
    pub model_id: String,
    pub label: String,
    pub recommended: bool,
}

#[derive(Debug, Clone)]
pub struct AiPolishRequest {
    pub provider_id: String,
    pub model_id: String,
    pub input_text: String,
    pub prompt: String,
    pub timeout_ms: u64,
    pub reasoning_effort: Option<AiReasoningEffort>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiPolishResult {
    pub output_text: String,
    pub provider_id: String,
    pub model_id: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiReasoningEffort {
    Low,
    Medium,
    High,
}
