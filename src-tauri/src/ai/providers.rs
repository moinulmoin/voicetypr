use super::contract::{AiModel, AiProvider};

pub const PROVIDER_OPENAI: &str = "openai";
pub const PROVIDER_ANTHROPIC: &str = "anthropic";
pub const PROVIDER_GEMINI: &str = "gemini";
pub const PROVIDER_CUSTOM: &str = "custom";

const PROVIDERS: &[(&str, &str, bool, bool, bool)] = &[
    (PROVIDER_OPENAI, "OpenAI", true, false, true),
    (PROVIDER_ANTHROPIC, "Anthropic", true, false, true),
    (PROVIDER_GEMINI, "Google Gemini", true, false, true),
    (
        PROVIDER_CUSTOM,
        "Custom (OpenAI-compatible)",
        false,
        true,
        false,
    ),
];

const OPENAI_MODELS: &[(&str, &str, bool)] = &[
    ("gpt-5-nano", "GPT-5 Nano", true),
    ("gpt-5-mini", "GPT-5 Mini", true),
];

const GEMINI_MODELS: &[(&str, &str, bool)] = &[
    ("gemini-3-flash-preview", "Gemini 3 Flash", true),
    ("gemini-2.5-flash", "Gemini 2.5 Flash", true),
    ("gemini-2.5-flash-lite", "Gemini 2.5 Flash Lite", true),
];

const ANTHROPIC_MODELS: &[(&str, &str, bool)] = &[
    ("claude-haiku-4-5", "Claude Haiku 4.5", true),
    ("claude-sonnet-4-6", "Claude Sonnet 4.6", false),
];

pub fn launch_providers() -> Vec<AiProvider> {
    PROVIDERS
        .iter()
        .map(
            |(id, label, requires_api_key, supports_base_url, supports_reasoning)| AiProvider {
                id: (*id).to_string(),
                label: (*label).to_string(),
                requires_api_key: *requires_api_key,
                supports_base_url: *supports_base_url,
                supports_reasoning: *supports_reasoning,
            },
        )
        .collect()
}

pub fn recommended_models(provider_id: &str) -> Vec<AiModel> {
    let models = match provider_id {
        PROVIDER_OPENAI => OPENAI_MODELS,
        PROVIDER_GEMINI => GEMINI_MODELS,
        PROVIDER_ANTHROPIC => ANTHROPIC_MODELS,
        _ => return Vec::new(),
    };

    models
        .iter()
        .map(|(model_id, label, recommended)| AiModel {
            provider_id: provider_id.to_string(),
            model_id: (*model_id).to_string(),
            label: (*label).to_string(),
            recommended: *recommended,
        })
        .collect()
}

pub fn is_native_provider(provider_id: &str) -> bool {
    matches!(
        provider_id,
        PROVIDER_OPENAI | PROVIDER_ANTHROPIC | PROVIDER_GEMINI
    )
}
