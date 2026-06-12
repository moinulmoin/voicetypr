//! Groq cloud STT via the OpenAI-compatible `/openai/v1/audio/transcriptions`.

use super::common::{self, AuthScheme};
use std::path::Path;
use tauri::AppHandle;

pub(super) const MODEL: &str = "whisper-large-v3-turbo";

const BASE: &str = "https://api.groq.com/openai/v1";

pub(super) async fn validate_key(key: &str) -> Result<(), String> {
    common::get_validate(
        "https://api.groq.com/openai/v1/models",
        AuthScheme::Bearer,
        key,
        "Groq",
    )
    .await
    .map_err(|e| e.message("Groq"))
}

pub(super) async fn transcribe(
    _app: &AppHandle,
    key: &str,
    audio_path: &Path,
    language: Option<&str>,
) -> Result<String, String> {
    common::openai_compatible_transcribe(
        BASE,
        key,
        MODEL,
        audio_path,
        language,
        "Groq transcription",
    )
    .await
    .map_err(|e| e.message("Groq"))
}
