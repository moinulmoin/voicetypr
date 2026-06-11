use super::contract::{AiPolishRequest, AiPolishResult};
use super::error::{AiProviderError, MappedAiProviderError};
use super::genai_runtime::{AiKeyResolver, GenaiRuntime};
use super::openai_compatible::OpenAiCompatibleRuntime;
use super::providers::{is_native_provider, PROVIDER_CUSTOM};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct AiExecutor {
    genai_runtime: GenaiRuntime,
    custom_runtime: OpenAiCompatibleRuntime,
}

impl AiExecutor {
    pub fn new(
        http_client: reqwest::Client,
        key_resolver: AiKeyResolver,
        custom_base_url: String,
        custom_no_auth: bool,
    ) -> Self {
        Self::with_native_endpoint_overrides(
            http_client,
            key_resolver,
            custom_base_url,
            custom_no_auth,
            HashMap::new(),
        )
    }

    pub fn with_native_endpoint_overrides(
        http_client: reqwest::Client,
        key_resolver: AiKeyResolver,
        custom_base_url: String,
        custom_no_auth: bool,
        native_endpoint_overrides: HashMap<String, String>,
    ) -> Self {
        Self {
            genai_runtime: GenaiRuntime::with_endpoint_overrides(
                http_client.clone(),
                key_resolver.clone(),
                native_endpoint_overrides,
            ),
            custom_runtime: OpenAiCompatibleRuntime::new(
                http_client,
                key_resolver,
                custom_base_url,
                custom_no_auth,
            ),
        }
    }

    pub async fn polish(
        &self,
        request: AiPolishRequest,
        cancellation_token: CancellationToken,
    ) -> Result<AiPolishResult, AiProviderError> {
        let start = Instant::now();
        let budget = Duration::from_millis(request.timeout_ms);
        let deadline = start + budget;
        let mut attempt = 0_u8;

        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(AiProviderError::Timeout)?;
            let result = self
                .run_with_budget(&request, cancellation_token.clone(), remaining)
                .await;

            match result {
                Ok(output_text) => {
                    if output_text.trim().is_empty() {
                        return Err(AiProviderError::BadResponse);
                    }
                    return Ok(AiPolishResult {
                        output_text,
                        provider_id: request.provider_id,
                        model_id: request.model_id,
                        duration_ms: start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                    });
                }
                Err(mapped) if attempt == 0 && should_retry(&mapped.error) => {
                    attempt += 1;
                    if let Some(retry_after) = mapped.retry_after {
                        let remaining_after_sleep = deadline
                            .checked_duration_since(Instant::now())
                            .ok_or(AiProviderError::Timeout)?;
                        if retry_after >= remaining_after_sleep {
                            return Err(mapped.error);
                        }
                        tokio::select! {
                            _ = cancellation_token.cancelled() => return Err(AiProviderError::Canceled),
                            _ = tokio::time::sleep(retry_after) => {}
                        }
                    }
                }
                Err(mapped) => return Err(mapped.error),
            }
        }
    }

    async fn run_with_budget(
        &self,
        request: &AiPolishRequest,
        cancellation_token: CancellationToken,
        remaining: Duration,
    ) -> Result<String, MappedAiProviderError> {
        tokio::select! {
            _ = cancellation_token.cancelled() => Err(MappedAiProviderError::new(AiProviderError::Canceled)),
            _ = tokio::time::sleep(remaining) => Err(MappedAiProviderError::new(AiProviderError::Timeout)),
            result = self.execute_once(request) => result,
        }
    }

    async fn execute_once(
        &self,
        request: &AiPolishRequest,
    ) -> Result<String, MappedAiProviderError> {
        if is_native_provider(&request.provider_id) {
            self.genai_runtime.polish(request).await
        } else if request.provider_id == PROVIDER_CUSTOM {
            self.custom_runtime.polish(request).await
        } else {
            Err(MappedAiProviderError::new(
                AiProviderError::UnsupportedProvider,
            ))
        }
    }
}

fn should_retry(error: &AiProviderError) -> bool {
    matches!(
        error,
        AiProviderError::RateLimited
            | AiProviderError::ServiceUnavailable
            | AiProviderError::Network
    )
}
