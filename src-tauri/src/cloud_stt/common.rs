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
pub(super) enum SttError {
    Auth,
    ModelUnavailable,
    RateLimited,
    Timeout,
    Network,
    Server,
    BadResponse,
}

impl SttError {
    pub(super) fn message(&self, provider_name: &str) -> String {
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

pub(super) fn is_transient(err: &SttError) -> bool {
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
    let body = resp.text().await.unwrap_or_default();
    let snippet: String = body.chars().take(300).collect();
    if !snippet.is_empty() {
        log::warn!("{label}: HTTP {status} body: {snippet}");
    } else {
        log::warn!("{label}: HTTP {status}");
    }
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

/// Transcribe via an OpenAI-compatible multipart `/audio/transcriptions`
/// endpoint (OpenAI, Groq). `base_url` excludes the trailing path.
pub(super) async fn openai_compatible_transcribe(
    base_url: &str,
    key: &str,
    model: &str,
    audio_path: &Path,
    language: Option<&str>,
    label: &str,
) -> Result<String, SttError> {
    use reqwest::multipart::{Form, Part};
    use tokio::fs;

    let bytes = fs::read(audio_path)
        .await
        .map_err(|_| SttError::BadResponse)?;
    let filename = audio_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("audio.wav")
        .to_string();
    let language = language
        .map(str::trim)
        .filter(|lang| !lang.is_empty())
        .map(str::to_string);
    let client = http_client();
    let url = format!("{}/audio/transcriptions", base_url);

    with_retry(|| {
        let bytes = bytes.clone();
        let client = client.clone();
        let filename = filename.clone();
        let language = language.clone();
        let url = url.clone();
        async move {
            let file_part = Part::bytes(bytes)
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
