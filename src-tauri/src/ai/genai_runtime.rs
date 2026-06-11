use super::contract::{AiPolishRequest, AiReasoningEffort};
use super::error::{map_genai_error, AiProviderError, MappedAiProviderError};
use super::providers::{PROVIDER_ANTHROPIC, PROVIDER_GEMINI, PROVIDER_OPENAI};
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
        let model = ModelIden::new(adapter_kind, request.model_id.as_str());
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
    match provider_id {
        PROVIDER_OPENAI => Some(AdapterKind::OpenAI),
        PROVIDER_ANTHROPIC => Some(AdapterKind::Anthropic),
        PROVIDER_GEMINI => Some(AdapterKind::Gemini),
        _ => None,
    }
}

fn provider_id_for_adapter(adapter_kind: AdapterKind) -> Option<&'static str> {
    match adapter_kind {
        AdapterKind::OpenAI => Some(PROVIDER_OPENAI),
        AdapterKind::Anthropic => Some(PROVIDER_ANTHROPIC),
        AdapterKind::Gemini => Some(PROVIDER_GEMINI),
        _ => None,
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
