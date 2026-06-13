use super::contract::{AiPolishRequest, AiReasoningEffort};
use super::error::{map_genai_error, AiProviderError, MappedAiProviderError};
use crate::ai::catalog;
use genai::adapter::AdapterKind;
use genai::chat::{ChatOptions, ChatRequest, ReasoningEffort};
use genai::resolver::{AuthData, Endpoint};
use genai::{Client, ClientBuilder, ModelIden, ServiceTarget};
use std::collections::HashMap;
use std::sync::Arc;

pub type AiKeyResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

#[derive(Clone)]
pub struct GenaiRuntime {
    client: Client,
}

impl GenaiRuntime {
    pub fn with_endpoint_overrides(
        reqwest_client: reqwest::Client,
        key_resolver: AiKeyResolver,
        endpoint_overrides: HashMap<String, String>,
    ) -> Self {
        let endpoint_overrides = Arc::new(endpoint_overrides);
        let auth_resolver = key_resolver.clone();
        let endpoint_resolver = endpoint_overrides.clone();
        let client = ClientBuilder::default()
            .with_reqwest(reqwest_client)
            .with_auth_resolver_fn(move |model: ModelIden| {
                let provider_id = provider_id_for_adapter(model.adapter_kind).unwrap_or_default();
                Ok(auth_resolver(provider_id).map(AuthData::from_single))
            })
            .with_service_target_resolver_fn(move |mut target: ServiceTarget| {
                if let Some(provider_id) = provider_id_for_adapter(target.model.adapter_kind) {
                    if let Some(base_url) = endpoint_resolver.get(provider_id) {
                        target.endpoint = Endpoint::from_owned(ensure_trailing_slash(base_url));
                    }
                }
                Ok(target)
            })
            .build();
        Self { client }
    }

    pub async fn polish(&self, request: &AiPolishRequest) -> Result<String, MappedAiProviderError> {
        let adapter_kind = adapter_kind_for_provider(&request.provider_id)
            .ok_or_else(|| MappedAiProviderError::new(AiProviderError::UnsupportedProvider))?;
        let model_str = namespaced_model(&request.provider_id, &request.model_id);
        let model = ModelIden::new(adapter_kind, model_str);
        let chat_request =
            ChatRequest::from_user(request.input_text.clone()).with_system(request.prompt.clone());
        let options = request.reasoning_effort.map(|effort| {
            ChatOptions::default().with_reasoning_effort(map_reasoning_effort(effort))
        });

        let response = self
            .client
            .exec_chat(model, chat_request, options.as_ref())
            .await
            .map_err(|error| map_genai_error(&error))?;

        response
            .into_first_text()
            .ok_or_else(|| MappedAiProviderError::new(AiProviderError::BadResponse))
    }
}

fn adapter_kind_for_provider(provider_id: &str) -> Option<AdapterKind> {
    match catalog::adapter_name(provider_id)? {
        "OpenAI" => Some(AdapterKind::OpenAI),
        "Anthropic" => Some(AdapterKind::Anthropic),
        "Gemini" => Some(AdapterKind::Gemini),
        "Groq" => Some(AdapterKind::Groq),
        "Xai" => Some(AdapterKind::Xai),
        "OpenRouter" => Some(AdapterKind::OpenRouter),
        "DeepSeek" => Some(AdapterKind::DeepSeek),
        "Cohere" => Some(AdapterKind::Cohere),
        _ => None,
    }
}

fn provider_id_for_adapter(adapter_kind: AdapterKind) -> Option<&'static str> {
    let adapter_name = match adapter_kind {
        AdapterKind::OpenAI => "OpenAI",
        AdapterKind::Anthropic => "Anthropic",
        AdapterKind::Gemini => "Gemini",
        AdapterKind::Groq => "Groq",
        AdapterKind::Xai => "Xai",
        AdapterKind::OpenRouter => "OpenRouter",
        AdapterKind::DeepSeek => "DeepSeek",
        AdapterKind::Cohere => "Cohere",
        _ => return None,
    };
    catalog::provider_for_adapter(adapter_name)
}

fn namespaced_model(provider_id: &str, model_id: &str) -> String {
    match catalog::namespace(provider_id) {
        Some(namespace) => format!("{namespace}{model_id}"),
        None => model_id.to_string(),
    }
}

fn map_reasoning_effort(effort: AiReasoningEffort) -> ReasoningEffort {
    match effort {
        AiReasoningEffort::Low => ReasoningEffort::Low,
        AiReasoningEffort::Medium => ReasoningEffort::Medium,
        AiReasoningEffort::High => ReasoningEffort::High,
    }
}

fn ensure_trailing_slash(base_url: &str) -> String {
    if base_url.ends_with('/') {
        base_url.to_string()
    } else {
        format!("{base_url}/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespaced_model_prefixes_compat_adapters() {
        assert_eq!(
            namespaced_model("groq", "llama-3.3-70b-versatile"),
            "groq::llama-3.3-70b-versatile"
        );
        assert_eq!(namespaced_model("xai", "grok-4.3"), "xai::grok-4.3");
        assert_eq!(
            namespaced_model("openrouter", "openai/gpt-4.1-mini"),
            "open_router::openai/gpt-4.1-mini"
        );
        assert_eq!(
            namespaced_model("deepseek", "deepseek-chat"),
            "deepseek::deepseek-chat"
        );
    }

    #[test]
    fn namespaced_model_leaves_native_adapters_clean() {
        assert_eq!(namespaced_model("openai", "gpt-5-mini"), "gpt-5-mini");
        assert_eq!(
            namespaced_model("anthropic", "claude-haiku-4-5"),
            "claude-haiku-4-5"
        );
        assert_eq!(
            namespaced_model("gemini", "gemini-2.5-flash"),
            "gemini-2.5-flash"
        );
    }

    #[test]
    fn adapter_kind_round_trips_to_provider_id() {
        for provider_id in [
            "openai",
            "anthropic",
            "gemini",
            "groq",
            "xai",
            "openrouter",
            "deepseek",
            "cohere",
        ] {
            let kind = adapter_kind_for_provider(provider_id)
                .unwrap_or_else(|| panic!("{provider_id} should map to a genai adapter"));
            assert_eq!(provider_id_for_adapter(kind), Some(provider_id));
        }
    }

    #[test]
    fn compat_providers_hide_reasoning_control() {
        let providers = catalog::launch_providers();
        let supports = |id: &str| {
            providers
                .iter()
                .find(|provider| provider.id == id)
                .map(|provider| provider.supports_reasoning)
        };
        // genai drops reasoning_effort for the OpenAI-compatible adapters -> hide the control.
        assert_eq!(supports("groq"), Some(false));
        assert_eq!(supports("xai"), Some(false));
        assert_eq!(supports("openrouter"), Some(false));
        // native adapters keep reasoning.
        assert_eq!(supports("openai"), Some(true));
        assert_eq!(supports("anthropic"), Some(true));
        assert_eq!(supports("gemini"), Some(true));
    }
}
