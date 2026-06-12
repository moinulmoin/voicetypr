//! Deepgram cloud STT via the pre-recorded `/v1/listen` endpoint.
//!
//! Deepgram differs from the OpenAI-compatible providers: `Token` auth (not
//! Bearer), a raw audio body (not multipart), the model in the query string,
//! and a nested transcript path in the response.

use super::common::{self, AuthScheme};
use std::path::Path;
use tauri::AppHandle;

pub(super) const MODEL: &str = "nova-3";

pub(super) async fn validate_key(key: &str) -> Result<(), String> {
    common::get_validate(
        "https://api.deepgram.com/v1/projects",
        AuthScheme::Token,
        key,
        "Deepgram",
    )
    .await
    .map_err(|e| e.message("Deepgram"))
}

pub(super) async fn transcribe(
    _app: &AppHandle,
    key: &str,
    audio_path: &Path,
    language: Option<&str>,
) -> Result<String, String> {
    use tokio::fs;

    let bytes = fs::read(audio_path)
        .await
        .map_err(|_| common::SttError::BadResponse.message("Deepgram"))?;

    let mut url = format!(
        "https://api.deepgram.com/v1/listen?model={}&smart_format=true",
        MODEL
    );
    if let Some(lang) = language.map(str::trim).filter(|lang| !lang.is_empty()) {
        url.push_str(&format!("&language={}", lang));
    }

    let client = common::http_client();
    common::with_retry(|| {
        let body = bytes.clone();
        let client = client.clone();
        let url = url.clone();
        async move {
            let resp = client
                .post(&url)
                .header("Authorization", format!("Token {}", key))
                .header("Content-Type", "audio/wav")
                .body(body)
                .send()
                .await
                .map_err(|e| common::classify_reqwest_err(&e))?;

            if !resp.status().is_success() {
                return Err(common::log_http_body(resp, "Deepgram transcription").await);
            }

            let json: serde_json::Value = resp
                .json()
                .await
                .map_err(|_| common::SttError::BadResponse)?;
            json.pointer("/results/channels/0/alternatives/0/transcript")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .ok_or(common::SttError::BadResponse)
        }
    })
    .await
    .map_err(|e| e.message("Deepgram"))
}
