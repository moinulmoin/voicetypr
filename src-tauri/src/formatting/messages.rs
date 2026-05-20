#![allow(dead_code)]

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FormattingCommand {
    Health {
        id: String,
        #[serde(rename = "protocolVersion")]
        protocol_version: u32,
    },
    ListProviders {
        id: String,
        #[serde(rename = "protocolVersion")]
        protocol_version: u32,
    },
    ListModels {
        id: String,
        #[serde(rename = "protocolVersion")]
        protocol_version: u32,
        provider: String,
    },
    Format {
        id: String,
        #[serde(rename = "protocolVersion")]
        protocol_version: u32,
        provider: String,
        model: String,
        prompt: String,
        #[serde(rename = "systemPrompt", skip_serializing_if = "Option::is_none")]
        system_prompt: Option<String>,
        #[serde(rename = "apiKey", skip_serializing_if = "Option::is_none")]
        api_key: Option<String>,
        #[serde(rename = "noAuth")]
        no_auth: bool,
        #[serde(rename = "customBaseUrl", skip_serializing_if = "Option::is_none")]
        custom_base_url: Option<String>,
        #[serde(rename = "timeoutMs")]
        timeout_ms: u64,
    },
    Shutdown {
        id: String,
        #[serde(rename = "protocolVersion")]
        protocol_version: u32,
    },
}

impl FormattingCommand {
    pub fn id(&self) -> &str {
        match self {
            Self::Health { id, .. }
            | Self::ListProviders { id, .. }
            | Self::ListModels { id, .. }
            | Self::Format { id, .. }
            | Self::Shutdown { id, .. } => id,
        }
    }

    pub fn is_format(&self) -> bool {
        matches!(self, Self::Format { .. })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct FormattingUsage {
    #[serde(rename = "inputTokens")]
    pub input_tokens: Option<u64>,
    #[serde(rename = "outputTokens")]
    pub output_tokens: Option<u64>,
    pub cost: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FormattingModel {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub recommended: bool,
    #[serde(rename = "contextWindow")]
    pub context_window: Option<u64>,
    #[serde(rename = "maxTokens")]
    pub max_tokens: Option<u64>,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub input: Vec<String>,
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FormattingProviderInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FormattingResponse {
    Ready {
        id: Option<String>,
        #[serde(rename = "protocolVersion")]
        protocol_version: u32,
        ok: bool,
    },
    Providers {
        id: Option<String>,
        #[serde(rename = "protocolVersion")]
        protocol_version: u32,
        ok: bool,
        providers: Vec<FormattingProviderInfo>,
    },
    Models {
        id: Option<String>,
        #[serde(rename = "protocolVersion")]
        protocol_version: u32,
        ok: bool,
        provider: String,
        models: Vec<FormattingModel>,
    },
    Formatted {
        id: Option<String>,
        #[serde(rename = "protocolVersion")]
        protocol_version: u32,
        ok: bool,
        text: String,
        provider: String,
        model: String,
        #[serde(rename = "latencyMs")]
        latency_ms: u64,
        usage: Option<FormattingUsage>,
    },
    Error {
        id: Option<String>,
        #[serde(rename = "protocolVersion")]
        protocol_version: Option<u32>,
        ok: bool,
        code: String,
        message: String,
        retryable: bool,
    },
    Shutdown {
        id: Option<String>,
        #[serde(rename = "protocolVersion")]
        protocol_version: u32,
        ok: bool,
    },
}

impl FormattingResponse {
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Ready { id, .. }
            | Self::Providers { id, .. }
            | Self::Models { id, .. }
            | Self::Formatted { id, .. }
            | Self::Error { id, .. }
            | Self::Shutdown { id, .. } => id.as_deref(),
        }
    }

    pub fn protocol_version(&self) -> Option<u32> {
        match self {
            Self::Ready {
                protocol_version, ..
            }
            | Self::Providers {
                protocol_version, ..
            }
            | Self::Models {
                protocol_version, ..
            }
            | Self::Formatted {
                protocol_version, ..
            }
            | Self::Shutdown {
                protocol_version, ..
            } => Some(*protocol_version),
            Self::Error {
                protocol_version, ..
            } => *protocol_version,
        }
    }
}
