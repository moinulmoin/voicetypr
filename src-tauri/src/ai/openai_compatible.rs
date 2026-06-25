use super::contract::AiPolishRequest;
use super::error::{map_http_status, map_reqwest_error, AiProviderError, MappedAiProviderError};
use super::genai_runtime::AiKeyResolver;
use super::providers::PROVIDER_CUSTOM;
use reqwest::StatusCode;
use serde_json::{json, Value};

#[derive(Clone)]
pub struct OpenAiCompatibleRuntime {
    client: reqwest::Client,
    key_resolver: AiKeyResolver,
    base_url: String,
    no_auth: bool,
}

impl OpenAiCompatibleRuntime {
    pub fn new(
        client: reqwest::Client,
        key_resolver: AiKeyResolver,
        base_url: String,
        no_auth: bool,
    ) -> Self {
        Self {
            client,
            key_resolver,
            base_url,
            no_auth,
        }
    }

    pub async fn polish(&self, request: &AiPolishRequest) -> Result<String, MappedAiProviderError> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let payload = json!({
            "model": request.model_id,
            "messages": [
                { "role": "system", "content": request.prompt },
                { "role": "user", "content": request.input_text }
            ],
            "stream": false
        });
        let mut builder = self.client.post(url).json(&payload);
        if !self.no_auth {
            let key = (self.key_resolver)(PROVIDER_CUSTOM)
                .ok_or_else(|| MappedAiProviderError::new(AiProviderError::MissingApiKey))?;
            builder = builder.bearer_auth(key);
        }

        let response = builder
            .send()
            .await
            .map_err(|error| MappedAiProviderError::new(map_reqwest_error(&error)))?;
        let status = response.status();
        let headers = response.headers().clone();
        let body = response
            .text()
            .await
            .map_err(|error| MappedAiProviderError::new(map_reqwest_error(&error)))?;
        if status != StatusCode::OK {
            return Err(map_http_status(status, Some(&body), Some(&headers)));
        }

        let value: Value = serde_json::from_str(&body)
            .map_err(|_| MappedAiProviderError::new(AiProviderError::BadResponse))?;
        value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .ok_or_else(|| MappedAiProviderError::new(AiProviderError::BadResponse))
    }
}
