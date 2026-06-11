#[cfg(test)]
mod tests {
    use super::super::contract::{AiPolishRequest, AiReasoningEffort};
    use super::super::error::AiProviderError;
    use super::super::executor::AiExecutor;
    use super::super::genai_runtime::AiKeyResolver;
    use super::super::providers::{
        PROVIDER_ANTHROPIC, PROVIDER_CUSTOM, PROVIDER_GEMINI, PROVIDER_OPENAI,
    };
    use reqwest::header::AUTHORIZATION;
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tokio_util::sync::CancellationToken;
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    #[derive(Clone, Copy, Debug)]
    struct ProviderCase {
        id: &'static str,
        model: &'static str,
    }

    const PROVIDERS: &[ProviderCase] = &[
        ProviderCase {
            id: PROVIDER_OPENAI,
            model: "gpt-5-nano",
        },
        ProviderCase {
            id: PROVIDER_ANTHROPIC,
            model: "claude-sonnet-4-6",
        },
        ProviderCase {
            id: PROVIDER_GEMINI,
            model: "gemini-3-flash-preview",
        },
        ProviderCase {
            id: PROVIDER_CUSTOM,
            model: "custom-model",
        },
    ];

    #[tokio::test]
    async fn ai_runtime_success_round_trip_for_all_providers() {
        for case in PROVIDERS {
            let server = MockServer::start().await;
            mount_sequence(&server, case.id, vec![ok_response(case.id, "polished")]).await;
            let executor = executor_for(*case, &server, true, false);

            let result = executor
                .polish(request(*case, 1_000, None), CancellationToken::new())
                .await
                .unwrap();

            assert_eq!(result.output_text, "polished");
            assert_eq!(result.provider_id, case.id);
            assert_eq!(result.model_id, case.model);
            assert_eq!(server.received_requests().await.unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn ai_runtime_maps_401_to_invalid_api_key_for_all_providers() {
        for case in PROVIDERS {
            let server = MockServer::start().await;
            mount_sequence(&server, case.id, vec![error_response(401, "bad key")]).await;
            let executor = executor_for(*case, &server, true, false);

            let error = executor
                .polish(request(*case, 1_000, None), CancellationToken::new())
                .await
                .unwrap_err();

            assert_eq!(error, AiProviderError::InvalidApiKey);
            assert_eq!(server.received_requests().await.unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn ai_runtime_retries_429_once_then_success_for_all_providers() {
        for case in PROVIDERS {
            let server = MockServer::start().await;
            mount_sequence(
                &server,
                case.id,
                vec![
                    error_response(429, "rate limited"),
                    ok_response(case.id, "polished"),
                ],
            )
            .await;
            let executor = executor_for(*case, &server, true, false);

            let result = executor
                .polish(request(*case, 1_000, None), CancellationToken::new())
                .await
                .unwrap();

            assert_eq!(result.output_text, "polished");
            assert_eq!(server.received_requests().await.unwrap().len(), 2);
        }
    }

    #[tokio::test]
    async fn ai_runtime_retries_500_once_then_success_for_all_providers() {
        for case in PROVIDERS {
            let server = MockServer::start().await;
            mount_sequence(
                &server,
                case.id,
                vec![
                    error_response(500, "server error"),
                    ok_response(case.id, "polished"),
                ],
            )
            .await;
            let executor = executor_for(*case, &server, true, false);

            let result = executor
                .polish(request(*case, 1_000, None), CancellationToken::new())
                .await
                .unwrap();

            assert_eq!(result.output_text, "polished");
            assert_eq!(server.received_requests().await.unwrap().len(), 2);
        }
    }

    #[tokio::test]
    async fn ai_runtime_retries_500_once_then_returns_service_unavailable_for_all_providers() {
        for case in PROVIDERS {
            let server = MockServer::start().await;
            mount_sequence(
                &server,
                case.id,
                vec![
                    error_response(500, "server error"),
                    error_response(500, "server error"),
                ],
            )
            .await;
            let executor = executor_for(*case, &server, true, false);

            let error = executor
                .polish(request(*case, 1_000, None), CancellationToken::new())
                .await
                .unwrap_err();

            assert_eq!(error, AiProviderError::ServiceUnavailable);
            assert_eq!(server.received_requests().await.unwrap().len(), 2);
        }
    }

    #[tokio::test]
    async fn ai_runtime_timeout_uses_total_budget_for_all_providers() {
        for case in PROVIDERS {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path_regex(".*"))
                .respond_with(ok_response(case.id, "late").set_delay(Duration::from_secs(2)))
                .mount(&server)
                .await;
            let executor = executor_for(*case, &server, true, false);
            let started = Instant::now();

            let error = executor
                .polish(request(*case, 150, None), CancellationToken::new())
                .await
                .unwrap_err();

            let elapsed = started.elapsed();
            assert_eq!(error, AiProviderError::Timeout);
            assert!(elapsed >= Duration::from_millis(100), "elapsed={elapsed:?}");
            assert!(elapsed < Duration::from_millis(900), "elapsed={elapsed:?}");
        }
    }

    #[tokio::test]
    async fn ai_runtime_cancellation_mid_flight_for_all_providers() {
        for case in PROVIDERS {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path_regex(".*"))
                .respond_with(ok_response(case.id, "late").set_delay(Duration::from_secs(2)))
                .mount(&server)
                .await;
            let executor = executor_for(*case, &server, true, false);
            let token = CancellationToken::new();
            let child = token.clone();
            let request = request(*case, 2_000, None);
            let handle = tokio::spawn(async move { executor.polish(request, child).await });
            tokio::time::sleep(Duration::from_millis(50)).await;
            token.cancel();

            let error = handle.await.unwrap().unwrap_err();

            assert_eq!(error, AiProviderError::Canceled);
        }
    }

    #[tokio::test]
    async fn ai_runtime_empty_content_is_bad_response_for_all_providers() {
        for case in PROVIDERS {
            let server = MockServer::start().await;
            mount_sequence(&server, case.id, vec![ok_response(case.id, "   ")]).await;
            let executor = executor_for(*case, &server, true, false);

            let error = executor
                .polish(request(*case, 1_000, None), CancellationToken::new())
                .await
                .unwrap_err();

            assert_eq!(error, AiProviderError::BadResponse);
        }
    }

    #[tokio::test]
    async fn ai_runtime_reasoning_effort_is_native_only() {
        for case in PROVIDERS {
            let server = MockServer::start().await;
            mount_sequence(&server, case.id, vec![ok_response(case.id, "polished")]).await;
            let executor = executor_for(*case, &server, true, false);

            executor
                .polish(
                    request(*case, 1_000, Some(AiReasoningEffort::Low)),
                    CancellationToken::new(),
                )
                .await
                .unwrap();

            let requests = server.received_requests().await.unwrap();
            let payload: Value = serde_json::from_slice(&requests[0].body).unwrap();
            match case.id {
                PROVIDER_OPENAI => assert_eq!(payload["reasoning_effort"], "low"),
                PROVIDER_ANTHROPIC => assert!(payload.get("thinking").is_some()),
                PROVIDER_GEMINI => assert!(payload
                    .pointer("/generationConfig/thinkingConfig")
                    .is_some()),
                PROVIDER_CUSTOM => assert!(payload.get("reasoning_effort").is_none()),
                _ => unreachable!(),
            }
        }
    }

    #[tokio::test]
    async fn ai_runtime_auth_header_from_key_source_and_custom_no_auth_supported() {
        for case in PROVIDERS.iter().filter(|case| case.id != PROVIDER_CUSTOM) {
            let server = MockServer::start().await;
            mount_sequence(&server, case.id, vec![ok_response(case.id, "polished")]).await;
            let executor = executor_for(*case, &server, true, false);

            executor
                .polish(request(*case, 1_000, None), CancellationToken::new())
                .await
                .unwrap();

            let request = server.received_requests().await.unwrap().remove(0);
            assert!(native_auth_header_present(case.id, &request));
        }

        let custom = ProviderCase {
            id: PROVIDER_CUSTOM,
            model: "custom-model",
        };
        let server = MockServer::start().await;
        mount_sequence(&server, custom.id, vec![ok_response(custom.id, "polished")]).await;
        let executor = executor_for(custom, &server, false, true);

        executor
            .polish(request(custom, 1_000, None), CancellationToken::new())
            .await
            .unwrap();

        let request = server.received_requests().await.unwrap().remove(0);
        assert!(request.headers.get(AUTHORIZATION).is_none());
    }

    async fn mount_sequence(
        server: &MockServer,
        provider_id: &str,
        responses: Vec<ResponseTemplate>,
    ) {
        let counter = Arc::new(AtomicUsize::new(0));
        let responses = Arc::new(responses);
        Mock::given(method("POST"))
            .and(path_regex(path_for_provider(provider_id)))
            .respond_with(move |_request: &Request| {
                let index = counter.fetch_add(1, Ordering::SeqCst);
                responses
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| responses.last().unwrap().clone())
            })
            .mount(server)
            .await;
    }

    fn executor_for(
        case: ProviderCase,
        server: &MockServer,
        include_key: bool,
        custom_no_auth: bool,
    ) -> AiExecutor {
        let key_resolver: AiKeyResolver = Arc::new(move |_provider_id| {
            if include_key {
                Some("test-key".to_string())
            } else {
                None
            }
        });
        let mut overrides = HashMap::new();
        if case.id != PROVIDER_CUSTOM {
            overrides.insert(case.id.to_string(), server.uri());
        }
        AiExecutor::with_native_endpoint_overrides(
            reqwest::Client::new(),
            key_resolver,
            server.uri(),
            custom_no_auth,
            overrides,
        )
    }

    fn request(
        case: ProviderCase,
        timeout_ms: u64,
        reasoning_effort: Option<AiReasoningEffort>,
    ) -> AiPolishRequest {
        AiPolishRequest {
            provider_id: case.id.to_string(),
            model_id: case.model.to_string(),
            input_text: "raw transcript".to_string(),
            prompt: "polish the transcript".to_string(),
            timeout_ms,
            reasoning_effort,
        }
    }

    fn ok_response(provider_id: &str, content: &str) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(match provider_id {
            PROVIDER_ANTHROPIC => json!({
                "model": "claude-sonnet-4-6",
                "content": [{ "type": "text", "text": content }],
                "stop_reason": "end_turn"
            }),
            PROVIDER_GEMINI => json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [{ "text": content }]
                    },
                    "finishReason": "STOP"
                }]
            }),
            _ => json!({
                "model": "gpt-5-nano",
                "choices": [{
                    "message": { "role": "assistant", "content": content },
                    "finish_reason": "stop"
                }]
            }),
        })
    }

    fn error_response(status: u16, body: &str) -> ResponseTemplate {
        ResponseTemplate::new(status).set_body_string(body.to_string())
    }

    fn path_for_provider(provider_id: &str) -> &'static str {
        match provider_id {
            PROVIDER_ANTHROPIC => r"^/messages$",
            PROVIDER_GEMINI => r"^/models/.+:generateContent$",
            _ => r"^/chat/completions$",
        }
    }

    fn native_auth_header_present(provider_id: &str, request: &Request) -> bool {
        match provider_id {
            PROVIDER_OPENAI => {
                request
                    .headers
                    .get(AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    == Some("Bearer test-key")
            }
            PROVIDER_ANTHROPIC => {
                request
                    .headers
                    .get("x-api-key")
                    .and_then(|value| value.to_str().ok())
                    == Some("test-key")
            }
            PROVIDER_GEMINI => {
                request
                    .headers
                    .get("x-goog-api-key")
                    .and_then(|value| value.to_str().ok())
                    == Some("test-key")
            }
            _ => false,
        }
    }
}
