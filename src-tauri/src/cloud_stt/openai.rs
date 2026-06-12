//! OpenAI cloud STT via the OpenAI-compatible `/v1/audio/transcriptions`.

use super::common::{self, AuthScheme};
use std::path::Path;
use tauri::AppHandle;

pub(super) const MODEL: &str = "gpt-4o-transcribe";

const BASE: &str = "https://api.openai.com/v1";

pub(super) async fn validate_key(key: &str) -> Result<(), String> {
    common::get_validate(
        "https://api.openai.com/v1/models",
        AuthScheme::Bearer,
        key,
        "OpenAI",
    )
    .await
    .map_err(|e| e.message("OpenAI"))
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
        "OpenAI transcription",
    )
    .await
    .map_err(|e| e.message("OpenAI"))
}
