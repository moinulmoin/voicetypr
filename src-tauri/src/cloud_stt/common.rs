//! Shared HTTP helpers for cloud STT providers.

use std::path::Path;

/// Authorization header scheme for a provider's REST API.
#[derive(Clone, Copy)]
pub(super) enum AuthScheme {
    /// `Authorization: Bearer <key>`
    Bearer,
    /// `Authorization: Token <key>` (Deepgram)
    Token,
}

#[derive(Debug)]
pub(crate) enum SttError {
    Auth,
    ModelUnavailable,
    RateLimited,
    Timeout,
    Network,
    Server,
    BadResponse,
}

impl SttError {
    pub(crate) fn message(&self, provider_name: &str) -> String {
        match self {
            Self::Auth => format!("Invalid {} API key", provider_name),
            Self::ModelUnavailable => format!("{}: model unavailable for this key", provider_name),
            Self::RateLimited => {
                format!("{} rate limit reached. Try again shortly.", provider_name)
            }
            Self::Timeout => format!("{} request timed out", provider_name),
            Self::Network => format!("Network error reaching {}", provider_name),
            Self::Server => format!("{} service error. Try again shortly.", provider_name),
            Self::BadResponse => format!("{}: unexpected response", provider_name),
        }
    }
}

pub(super) fn classify_status(status: reqwest::StatusCode) -> SttError {
    match status {
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => SttError::Auth,
        reqwest::StatusCode::NOT_FOUND => SttError::ModelUnavailable,
        reqwest::StatusCode::REQUEST_TIMEOUT => SttError::Timeout,
        reqwest::StatusCode::TOO_MANY_REQUESTS => SttError::RateLimited,
        s if s.is_server_error() => SttError::Server,
        _ => SttError::BadResponse,
    }
}

pub(super) fn classify_reqwest_err(e: &reqwest::Error) -> SttError {
    if e.is_timeout() {
        SttError::Timeout
    } else {
        SttError::Network
    }
}

pub(crate) fn is_transient(err: &SttError) -> bool {
    matches!(
        err,
        SttError::RateLimited | SttError::Timeout | SttError::Network | SttError::Server
    )
}

pub(super) fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

pub(super) async fn with_retry<T, F, Fut>(mut op: F) -> Result<T, SttError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, SttError>>,
{
    match op().await {
        Ok(v) => Ok(v),
        Err(e) if is_transient(&e) => {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            op().await
        }
        Err(e) => Err(e),
    }
}

pub(super) async fn log_http_body(resp: reqwest::Response, label: &str) -> SttError {
    let status = resp.status();
    let err = classify_status(status);
    log::warn!("{label}: HTTP {status}; response body suppressed for authenticated request");
    err
}

/// Parse a JSON response whose transcript lives at the top-level `text` field
/// (OpenAI / Groq / Cohere shape).
pub(super) async fn parse_text_response(
    resp: reqwest::Response,
    label: &str,
) -> Result<String, SttError> {
    if !resp.status().is_success() {
        return Err(log_http_body(resp, label).await);
    }
    let json: serde_json::Value = resp.json().await.map_err(|_| SttError::BadResponse)?;
    json.get("text")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or(SttError::BadResponse)
}

/// Validate a key with a GET request to an authenticated listing endpoint.
pub(super) async fn get_validate(
    url: &str,
    scheme: AuthScheme,
    key: &str,
    provider_name: &str,
) -> Result<(), SttError> {
    let client = http_client();
    with_retry(|| {
        let client = client.clone();
        async move {
            let req = match scheme {
                AuthScheme::Bearer => client.get(url).bearer_auth(key),
                AuthScheme::Token => client
                    .get(url)
                    .header("Authorization", format!("Token {}", key)),
            };
            let resp = req.send().await.map_err(|e| classify_reqwest_err(&e))?;
            if resp.status().is_success() {
                Ok(())
            } else {
                Err(log_http_body(resp, provider_name).await)
            }
        }
    })
    .await
}

/// OpenAI/Groq only honor the final ~224 tokens of `prompt`, so an oversized
/// dictionary would be truncated from the front by the provider. Cap it here on
/// a separator (~3 chars/token for jargon-dense terms) so whole early terms win.
fn cap_transcription_prompt(prompt: &str) -> String {
    const MAX_BYTES: usize = 672;
    if prompt.len() <= MAX_BYTES {
        return prompt.to_string();
    }
    let mut end = MAX_BYTES;
    while end > 0 && !prompt.is_char_boundary(end) {
        end -= 1;
    }
    let cut = prompt[..end].rfind([',', ' ']).unwrap_or(end);
    prompt[..cut].trim_end_matches([',', ' ']).to_string()
}

/// Transcribe via an OpenAI-compatible multipart `/audio/transcriptions`
/// endpoint (OpenAI, Groq). `base_url` excludes the trailing path.
pub(super) async fn openai_compatible_transcribe(
    base_url: &str,
    key: &str,
    model: &str,
    audio_path: &Path,
    language: Option<&str>,
    prompt: Option<&str>,
    label: &str,
) -> Result<String, SttError> {
    use futures_util::stream;
    use reqwest::{
        multipart::{Form, Part},
        Body,
    };
    use tokio::fs::{metadata, File};
    use tokio::io::AsyncReadExt;

    let audio_len = metadata(audio_path)
        .await
        .map_err(|_| SttError::BadResponse)?
        .len();
    let filename = audio_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("audio.wav")
        .to_string();
    let language = language
        .map(str::trim)
        .filter(|lang| !lang.is_empty())
        .map(str::to_string);
    let prompt = prompt
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .map(cap_transcription_prompt);
    let client = http_client();
    let url = format!("{}/audio/transcriptions", base_url);

    with_retry(|| {
        let client = client.clone();
        let filename = filename.clone();
        let language = language.clone();
        let prompt = prompt.clone();
        let url = url.clone();
        async move {
            let file = File::open(audio_path)
                .await
                .map_err(|_| SttError::BadResponse)?;
            let stream = stream::try_unfold(file, |mut file| async move {
                let mut chunk = vec![0; 64 * 1024];
                let bytes_read = file.read(&mut chunk).await?;
                if bytes_read == 0 {
                    Ok::<Option<(Vec<u8>, File)>, std::io::Error>(None)
                } else {
                    chunk.truncate(bytes_read);
                    Ok(Some((chunk, file)))
                }
            });
            let file_part = Part::stream_with_length(Body::wrap_stream(stream), audio_len)
                .file_name(filename)
                .mime_str("audio/wav")
                .map_err(|_| SttError::BadResponse)?;
            let mut form = Form::new()
                .part("file", file_part)
                .text("model", model.to_string())
                .text("response_format", "json".to_string());
            if let Some(lang) = language {
                form = form.text("language", lang);
            }
            if let Some(prompt) = prompt {
                form = form.text("prompt", prompt);
            }

            let resp = client
                .post(&url)
                .bearer_auth(key)
                .multipart(form)
                .send()
                .await
                .map_err(|e| classify_reqwest_err(&e))?;

            parse_text_response(resp, label).await
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::{get_validate, openai_compatible_transcribe, AuthScheme, SttError};
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tempfile::NamedTempFile;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    #[test]
    fn transcription_prompt_capped_to_token_budget_on_term_boundary() {
        // Short prompts pass through untouched.
        let short = "Preferred spellings: shadcn/ui, Tauri.";
        assert_eq!(super::cap_transcription_prompt(short), short);

        // A large dictionary is capped (OpenAI/Groq honor only the final ~224
        // tokens) on a separator, so whole leading terms survive with no
        // mid-token cut.
        let many: Vec<String> = (0..300).map(|i| format!("term{i}")).collect();
        let long = format!("Preferred spellings: {}.", many.join(", "));
        let capped = super::cap_transcription_prompt(&long);
        assert!(capped.len() <= 672, "capped len was {}", capped.len());
        assert!(!capped.ends_with(',') && !capped.ends_with(' '));
        assert!(long.starts_with(&capped));
    }

    fn audio_file() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"wav").unwrap();
        file
    }

    async fn mount_sequence(server: &MockServer, responses: Vec<ResponseTemplate>) {
        let counter = Arc::new(AtomicUsize::new(0));
        let responses = Arc::new(responses);
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .respond_with(move |_request: &Request| {
                let index = counter.fetch_add(1, Ordering::SeqCst);
                responses
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| responses.last().unwrap().clone())
            })
            .expect(2)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn openai_compatible_transcribe_posts_multipart_and_parses_text() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .and(header("authorization", "Bearer k"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "text": "hello"
            })))
            .expect(1)
            .mount(&server)
            .await;
        let audio = audio_file();

        let text = openai_compatible_transcribe(
            &server.uri(),
            "k",
            "gpt-4o-transcribe",
            audio.path(),
            None,
            None,
            "OpenAI transcription",
        )
        .await
        .unwrap();

        assert_eq!(text, "hello");
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        let content_length = request
            .headers
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert!(!content_length.is_empty());
        let content_type = request
            .headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert!(content_type.starts_with("multipart/form-data; boundary="));
        let body = String::from_utf8_lossy(&request.body);
        assert!(body.contains("name=\"model\""));
        assert!(body.contains("gpt-4o-transcribe"));
    }
    #[tokio::test]
    async fn openai_compatible_transcribe_includes_prompt_field_when_provided() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .and(header("authorization", "Bearer k"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "text": "ok"
            })))
            .expect(1)
            .mount(&server)
            .await;
        let audio = audio_file();
        let prompt = "Preferred spellings: VoiceTypr, Tauri.";

        let text = openai_compatible_transcribe(
            &server.uri(),
            "k",
            "gpt-4o-transcribe",
            audio.path(),
            None,
            Some(prompt),
            "OpenAI transcription",
        )
        .await
        .unwrap();

        assert_eq!(text, "ok");
        let request = &server.received_requests().await.unwrap()[0];
        let body = String::from_utf8_lossy(&request.body);
        assert!(body.contains("name=\"prompt\""));
        assert!(body.contains("Preferred spellings: VoiceTypr, Tauri."));
    }

    #[tokio::test]
    async fn openai_compatible_transcribe_omits_prompt_field_when_empty_or_none() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "text": "ok"
            })))
            .expect(2)
            .mount(&server)
            .await;
        let audio = audio_file();

        // Empty/whitespace prompt must not produce a `prompt` part.
        openai_compatible_transcribe(
            &server.uri(),
            "k",
            "gpt-4o-transcribe",
            audio.path(),
            None,
            Some("   "),
            "OpenAI transcription",
        )
        .await
        .unwrap();
        // `None` prompt must not produce a `prompt` part either.
        openai_compatible_transcribe(
            &server.uri(),
            "k",
            "gpt-4o-transcribe",
            audio.path(),
            None,
            None,
            "OpenAI transcription",
        )
        .await
        .unwrap();

        for request in server.received_requests().await.unwrap() {
            let body = String::from_utf8_lossy(&request.body);
            assert!(!body.contains("name=\"prompt\""));
        }
    }

    #[tokio::test]
    async fn openai_compatible_transcribe_retries_500_once_then_succeeds() {
        let server = MockServer::start().await;
        mount_sequence(
            &server,
            vec![
                ResponseTemplate::new(500).set_body_string("temporary"),
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "text": "hello"
                })),
            ],
        )
        .await;
        let audio = audio_file();

        let text = openai_compatible_transcribe(
            &server.uri(),
            "k",
            "gpt-4o-transcribe",
            audio.path(),
            None,
            None,
            "OpenAI transcription",
        )
        .await
        .unwrap();

        let requests = server.received_requests().await.unwrap();
        assert_eq!(text, "hello");
        assert_eq!(requests.len(), 2);
        for request in requests {
            assert!(request.headers.get("content-length").is_some());
        }
    }

    #[tokio::test]
    async fn openai_compatible_transcribe_reopens_audio_file_for_retry() {
        let server = MockServer::start().await;
        let audio = audio_file();
        let audio_path = audio.path().to_path_buf();
        let responses_seen = Arc::new(AtomicUsize::new(0));
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .respond_with(move |_request: &Request| {
                if responses_seen.fetch_add(1, Ordering::SeqCst) == 0 {
                    std::fs::write(&audio_path, b"two").unwrap();
                    ResponseTemplate::new(500).set_body_string("temporary")
                } else {
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "text": "hello"
                    }))
                }
            })
            .expect(2)
            .mount(&server)
            .await;

        let text = openai_compatible_transcribe(
            &server.uri(),
            "k",
            "gpt-4o-transcribe",
            audio.path(),
            None,
            None,
            "OpenAI transcription",
        )
        .await
        .unwrap();

        let requests = server.received_requests().await.unwrap();
        let second_body = String::from_utf8_lossy(&requests[1].body);
        assert_eq!(text, "hello");
        assert!(second_body.contains("two"));
    }

    #[tokio::test]
    async fn openai_compatible_transcribe_retries_500_once_then_returns_server_without_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(500).set_body_string("unique-provider-body"))
            .expect(2)
            .mount(&server)
            .await;
        let audio = audio_file();

        let error = openai_compatible_transcribe(
            &server.uri(),
            "k",
            "gpt-4o-transcribe",
            audio.path(),
            None,
            None,
            "OpenAI transcription",
        )
        .await
        .unwrap_err();

        assert!(matches!(error, SttError::Server));
        let message = error.message("OpenAI");
        assert!(!message.contains("unique-provider-body"));
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn openai_compatible_transcribe_does_not_retry_auth_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(401).set_body_string("bad key"))
            .expect(1)
            .mount(&server)
            .await;
        let audio = audio_file();

        let error = openai_compatible_transcribe(
            &server.uri(),
            "k",
            "gpt-4o-transcribe",
            audio.path(),
            None,
            None,
            "OpenAI transcription",
        )
        .await
        .unwrap_err();

        assert!(matches!(error, SttError::Auth));
        assert_eq!(error.message("OpenAI"), "Invalid OpenAI API key");
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn get_validate_succeeds_on_200() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .and(header("authorization", "Bearer k"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        get_validate(
            &format!("{}/models", server.uri()),
            AuthScheme::Bearer,
            "k",
            "Groq",
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn get_validate_maps_auth_error_without_retry() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;

        let error = get_validate(
            &format!("{}/models", server.uri()),
            AuthScheme::Bearer,
            "k",
            "Groq",
        )
        .await
        .unwrap_err();

        assert!(matches!(error, SttError::Auth));
        assert_eq!(error.message("Groq"), "Invalid Groq API key");
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn get_validate_retries_500_once_then_returns_server() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(500))
            .expect(2)
            .mount(&server)
            .await;

        let error = get_validate(
            &format!("{}/models", server.uri()),
            AuthScheme::Bearer,
            "k",
            "Groq",
        )
        .await
        .unwrap_err();

        assert!(matches!(error, SttError::Server));
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }
}
