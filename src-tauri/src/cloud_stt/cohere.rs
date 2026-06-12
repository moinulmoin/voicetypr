//! Cohere Transcribe cloud STT via `/v2/audio/transcriptions`.
//!
//! Cohere requires an explicit `language` form field; we default to `en` when
//! the caller does not supply one.

use super::common::{self, AuthScheme};
use std::path::Path;
use tauri::AppHandle;

pub(super) const MODEL: &str = "cohere-transcribe-03-2026";

pub(super) async fn validate_key(key: &str) -> Result<(), String> {
    common::get_validate(
        "https://api.cohere.com/v1/models",
        AuthScheme::Bearer,
        key,
        "Cohere",
    )
    .await
    .map_err(|e| e.message("Cohere"))
}

pub(super) async fn transcribe(
    _app: &AppHandle,
    key: &str,
    audio_path: &Path,
    language: Option<&str>,
) -> Result<String, String> {
    use reqwest::multipart::{Form, Part};
    use tokio::fs;

    let bytes = fs::read(audio_path)
        .await
        .map_err(|_| common::SttError::BadResponse.message("Cohere"))?;
    let filename = audio_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("audio.wav")
        .to_string();

    // Cohere requires a language; default to English when unspecified.
    let lang = language
        .map(str::trim)
        .filter(|lang| !lang.is_empty())
        .unwrap_or("en")
        .to_string();

    let client = common::http_client();
    common::with_retry(|| {
        let bytes = bytes.clone();
        let client = client.clone();
        let filename = filename.clone();
        let lang = lang.clone();
        async move {
            let file_part = Part::bytes(bytes)
                .file_name(filename)
                .mime_str("audio/wav")
                .map_err(|_| common::SttError::BadResponse)?;
            let form = Form::new()
                .part("file", file_part)
                .text("model", MODEL)
                .text("language", lang);

            let resp = client
                .post("https://api.cohere.com/v2/audio/transcriptions")
                .bearer_auth(key)
                .multipart(form)
                .send()
                .await
                .map_err(|e| common::classify_reqwest_err(&e))?;

            common::parse_text_response(resp, "Cohere transcription").await
        }
    })
    .await
    .map_err(|e| e.message("Cohere"))
}
