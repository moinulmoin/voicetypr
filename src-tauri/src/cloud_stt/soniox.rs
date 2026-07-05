//! Soniox cloud STT: async Files + Transcriptions + poll flow.

use super::common::{self, AuthScheme};
use std::path::Path;
use tauri::AppHandle;

pub(super) const MODEL: &str = "stt-async-v5";

const BASE: &str = "https://api.soniox.com/v1";

pub(super) async fn validate_key(key: &str) -> Result<(), String> {
    common::get_validate(
        "https://api.soniox.com/v1/models",
        AuthScheme::Bearer,
        key,
        "Soniox",
    )
    .await
    .map_err(|e| e.message("Soniox"))
}

fn build_create_payload(
    file_id: &str,
    language: Option<&str>,
    context: Option<crate::writing::SonioxContext>,
    diarize: bool,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "model": MODEL,
        "file_id": file_id,
    });

    if let Some(lang) = language.map(str::trim).filter(|lang| !lang.is_empty()) {
        payload["language_hints"] = serde_json::json!([lang]);
    }

    if let Some(context) = context {
        if let Ok(context_value) = serde_json::to_value(context) {
            if context_value
                .as_object()
                .is_some_and(|object| !object.is_empty())
            {
                payload["context"] = context_value;
            }
        }
    }

    if diarize {
        payload["enable_speaker_diarization"] = serde_json::json!(true);
    }

    payload
}

pub(super) async fn transcribe_typed(
    app: &AppHandle,
    key: &str,
    wav_path: &Path,
    language: Option<&str>,
) -> Result<String, common::SttError> {
    use reqwest::multipart::{Form, Part};
    use tokio::fs;

    let wav_bytes = fs::read(wav_path)
        .await
        .map_err(|_| common::SttError::BadResponse)?;

    let client = common::http_client();

    // 1) Upload file -> file_id
    let filename = wav_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("audio.wav")
        .to_string();
    let upload_url = format!("{}/files", BASE);
    let upload_resp = common::with_retry(|| {
        let client = client.clone();
        let filename = filename.clone();
        let upload_url = upload_url.clone();
        let wav_bytes = wav_bytes.clone();
        async move {
            let file_part = Part::bytes(wav_bytes)
                .file_name(filename)
                .mime_str("audio/wav")
                .map_err(|_| common::SttError::BadResponse)?;
            let form = Form::new().part("file", file_part);

            let resp = client
                .post(&upload_url)
                .bearer_auth(key)
                .multipart(form)
                .send()
                .await
                .map_err(|e| common::classify_reqwest_err(&e))?;
            if resp.status().is_success() {
                Ok(resp)
            } else {
                Err(common::log_http_body(resp, "Soniox upload").await)
            }
        }
    })
    .await?;
    let upload_json: serde_json::Value = upload_resp
        .json()
        .await
        .map_err(|_| common::SttError::BadResponse)?;
    let file_id = upload_json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or(common::SttError::BadResponse)?
        .to_string();

    // 2) Create transcription -> transcription_id
    let soniox_context = match crate::writing::load_writing_settings(app) {
        Ok(settings) => crate::writing::compile_soniox_context(&settings, language),
        Err(err) => {
            log::warn!(
                "Failed to load writing settings for Soniox context; continuing without context: {err}"
            );
            None
        }
    };
    let payload = build_create_payload(&file_id, language, soniox_context, false);

    let create_url = format!("{}/transcriptions", BASE);
    let create_resp = common::with_retry(|| {
        let client = client.clone();
        let create_url = create_url.clone();
        let payload = payload.clone();
        async move {
            let resp = client
                .post(&create_url)
                .bearer_auth(key)
                .header("Content-Type", "application/json")
                .json(&payload)
                .send()
                .await
                .map_err(|e| common::classify_reqwest_err(&e))?;
            if resp.status().is_success() {
                Ok(resp)
            } else {
                Err(common::log_http_body(resp, "Soniox create transcription").await)
            }
        }
    })
    .await?;
    let create_json: serde_json::Value = create_resp
        .json()
        .await
        .map_err(|_| common::SttError::BadResponse)?;
    let transcription_id = create_json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or(common::SttError::BadResponse)?
        .to_string();

    // 3) Poll status
    let status_url = format!("{}/transcriptions/{}", BASE, transcription_id);
    let started = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(180);
    loop {
        let resp = common::with_retry(|| {
            let client = client.clone();
            let status_url = status_url.clone();
            async move {
                let resp = client
                    .get(&status_url)
                    .bearer_auth(key)
                    .send()
                    .await
                    .map_err(|e| common::classify_reqwest_err(&e))?;
                if resp.status().is_success() {
                    Ok(resp)
                } else {
                    Err(common::log_http_body(resp, "Soniox status").await)
                }
            }
        })
        .await?;
        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|_| common::SttError::BadResponse)?;
        let status = json.get("status").and_then(|v| v.as_str()).unwrap_or("");
        match status {
            "completed" => break,
            "error" => {
                log::warn!("Soniox transcription job failed");
                return Err(common::SttError::Server);
            }
            _ => {
                if started.elapsed() > timeout {
                    return Err(common::SttError::Timeout);
                }
                tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            }
        }
    }

    // 4) Fetch transcript
    let transcript_url = format!("{}/transcriptions/{}/transcript", BASE, transcription_id);
    let resp = common::with_retry(|| {
        let client = client.clone();
        let transcript_url = transcript_url.clone();
        async move {
            let resp = client
                .get(&transcript_url)
                .bearer_auth(key)
                .send()
                .await
                .map_err(|e| common::classify_reqwest_err(&e))?;
            if resp.status().is_success() {
                Ok(resp)
            } else {
                Err(common::log_http_body(resp, "Soniox transcript").await)
            }
        }
    })
    .await?;
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|_| common::SttError::BadResponse)?;

    // Prefer direct text if present, else join tokens
    if let Some(text) = json.get("text").and_then(|v| v.as_str()) {
        return Ok(text.to_string());
    }
    if let Some(tokens) = json.get("tokens").and_then(|v| v.as_array()) {
        let mut out = String::new();
        let mut first = true;
        for t in tokens {
            if let Some(txt) = t.get("text").and_then(|v| v.as_str()) {
                if !first {
                    out.push(' ');
                } else {
                    first = false;
                }
                out.push_str(txt);
            }
        }
        if !out.is_empty() {
            return Ok(out);
        }
    }
    Err(common::SttError::BadResponse)
}

pub(super) async fn transcribe_typed_diarized(
    app: &AppHandle,
    key: &str,
    wav_path: &Path,
    language: Option<&str>,
) -> Result<super::CloudTranscript, common::SttError> {
    use reqwest::multipart::{Form, Part};
    use tokio::fs;

    let wav_bytes = fs::read(wav_path)
        .await
        .map_err(|_| common::SttError::BadResponse)?;

    let client = common::http_client();

    // 1) Upload file -> file_id
    let filename = wav_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("audio.wav")
        .to_string();
    let upload_url = format!("{}/files", BASE);
    let upload_resp = common::with_retry(|| {
        let client = client.clone();
        let filename = filename.clone();
        let upload_url = upload_url.clone();
        let wav_bytes = wav_bytes.clone();
        async move {
            let file_part = Part::bytes(wav_bytes)
                .file_name(filename)
                .mime_str("audio/wav")
                .map_err(|_| common::SttError::BadResponse)?;
            let form = Form::new().part("file", file_part);
            let resp = client
                .post(&upload_url)
                .bearer_auth(key)
                .multipart(form)
                .send()
                .await
                .map_err(|e| common::classify_reqwest_err(&e))?;
            if resp.status().is_success() {
                Ok(resp)
            } else {
                Err(common::log_http_body(resp, "Soniox upload (diarized)").await)
            }
        }
    })
    .await?;
    let upload_json: serde_json::Value = upload_resp
        .json()
        .await
        .map_err(|_| common::SttError::BadResponse)?;
    let file_id = upload_json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or(common::SttError::BadResponse)?
        .to_string();

    // 2) Create transcription with diarization -> transcription_id
    let soniox_context = match crate::writing::load_writing_settings(app) {
        Ok(settings) => crate::writing::compile_soniox_context(&settings, language),
        Err(err) => {
            log::warn!(
                "Failed to load writing settings for Soniox diarized context; continuing: {err}"
            );
            None
        }
    };
    let payload = build_create_payload(&file_id, language, soniox_context, true);

    let create_url = format!("{}/transcriptions", BASE);
    let create_resp = common::with_retry(|| {
        let client = client.clone();
        let create_url = create_url.clone();
        let payload = payload.clone();
        async move {
            let resp = client
                .post(&create_url)
                .bearer_auth(key)
                .header("Content-Type", "application/json")
                .json(&payload)
                .send()
                .await
                .map_err(|e| common::classify_reqwest_err(&e))?;
            if resp.status().is_success() {
                Ok(resp)
            } else {
                Err(common::log_http_body(resp, "Soniox create transcription (diarized)").await)
            }
        }
    })
    .await?;
    let create_json: serde_json::Value = create_resp
        .json()
        .await
        .map_err(|_| common::SttError::BadResponse)?;
    let transcription_id = create_json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or(common::SttError::BadResponse)?
        .to_string();

    // 3) Poll status
    let status_url = format!("{}/transcriptions/{}", BASE, transcription_id);
    let started = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(180);
    loop {
        let resp = common::with_retry(|| {
            let client = client.clone();
            let status_url = status_url.clone();
            async move {
                let resp = client
                    .get(&status_url)
                    .bearer_auth(key)
                    .send()
                    .await
                    .map_err(|e| common::classify_reqwest_err(&e))?;
                if resp.status().is_success() {
                    Ok(resp)
                } else {
                    Err(common::log_http_body(resp, "Soniox status (diarized)").await)
                }
            }
        })
        .await?;
        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|_| common::SttError::BadResponse)?;
        let status = json.get("status").and_then(|v| v.as_str()).unwrap_or("");
        match status {
            "completed" => break,
            "error" => {
                log::warn!("Soniox diarized transcription job failed");
                return Err(common::SttError::Server);
            }
            _ => {
                if started.elapsed() > timeout {
                    return Err(common::SttError::Timeout);
                }
                tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            }
        }
    }

    // 4) Fetch transcript
    let transcript_url = format!("{}/transcriptions/{}/transcript", BASE, transcription_id);
    let resp = common::with_retry(|| {
        let client = client.clone();
        let transcript_url = transcript_url.clone();
        async move {
            let resp = client
                .get(&transcript_url)
                .bearer_auth(key)
                .send()
                .await
                .map_err(|e| common::classify_reqwest_err(&e))?;
            if resp.status().is_success() {
                Ok(resp)
            } else {
                Err(common::log_http_body(resp, "Soniox transcript (diarized)").await)
            }
        }
    })
    .await?;
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|_| common::SttError::BadResponse)?;

    // Extract text (prefer top-level `text`, else join tokens)
    let text = json
        .get("text")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
            json.get("tokens").and_then(|v| v.as_array()).map(|tokens| {
                tokens
                    .iter()
                    .filter_map(|t| t.get("text").and_then(|v| v.as_str()))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        })
        .filter(|s| !s.is_empty())
        .ok_or(common::SttError::BadResponse)?;

    // Parse per-word speaker data from tokens
    let words = json
        .get("tokens")
        .and_then(|v| v.as_array())
        .map(|tokens| tokens.iter().filter_map(parse_soniox_token).collect())
        .unwrap_or_default();

    Ok(super::CloudTranscript { text, words })
}

fn parse_soniox_token(t: &serde_json::Value) -> Option<crate::transcription::TranscriptionWord> {
    let text = t.get("text").and_then(|v| v.as_str())?.to_string();
    let start_ms = t
        .get("start_ms")
        .and_then(|v| v.as_i64())
        .map(|ms| ms as u64);
    let end_ms = t.get("end_ms").and_then(|v| v.as_i64()).map(|ms| ms as u64);
    let speaker_id = t.get("speaker").and_then(|v| {
        v.as_i64()
            .map(|n| format!("Speaker {n}"))
            .or_else(|| v.as_str().map(|s| format!("Speaker {s}")))
    });
    Some(crate::transcription::TranscriptionWord {
        text,
        start_ms,
        end_ms,
        speaker_id,
        confidence: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writing::{SonioxContext, SonioxContextField};

    #[test]
    fn create_payload_includes_language_and_structured_context() {
        let payload = build_create_payload(
            "file_123",
            Some(" en "),
            Some(SonioxContext {
                general: vec![SonioxContextField {
                    key: "domain".to_string(),
                    value: "Software".to_string(),
                }],
                terms: vec!["Voicetypr".to_string(), "Tauri".to_string()],
                text: Some(
                    "Spoken forms map to canonical spellings: voice typer -> Voicetypr."
                        .to_string(),
                ),
            }),
            false,
        );

        assert_eq!(payload["model"].as_str(), Some("stt-async-v5"));
        assert_eq!(payload["file_id"].as_str(), Some("file_123"));
        assert_eq!(
            payload["language_hints"].as_array().unwrap()[0].as_str(),
            Some("en")
        );
        assert_eq!(
            payload["context"]["terms"].as_array().unwrap()[0].as_str(),
            Some("Voicetypr")
        );
        assert_eq!(
            payload["context"]["text"].as_str(),
            Some("Spoken forms map to canonical spellings: voice typer -> Voicetypr.")
        );
    }

    #[test]
    fn create_payload_omits_empty_optional_fields() {
        let payload = build_create_payload(
            "file_123",
            Some(" "),
            Some(SonioxContext {
                general: Vec::new(),
                terms: Vec::new(),
                text: None,
            }),
            false,
        );

        assert_eq!(payload["model"].as_str(), Some("stt-async-v5"));
        assert!(payload.get("language_hints").is_none());
        assert!(payload.get("context").is_none());
    }

    #[test]
    fn build_create_payload_diarize_flag_sets_field() {
        let payload = build_create_payload("fid", None, None, true);
        assert_eq!(payload["enable_speaker_diarization"].as_bool(), Some(true));
        let payload_no_diarize = build_create_payload("fid", None, None, false);
        assert!(payload_no_diarize
            .get("enable_speaker_diarization")
            .is_none());
    }

    #[test]
    fn parse_soniox_token_with_speaker_produces_speaker_id() {
        let t = serde_json::json!({
            "text": "Hello",
            "start_ms": 0,
            "end_ms": 500,
            "speaker": 0
        });
        let word = parse_soniox_token(&t).unwrap();
        assert_eq!(word.text, "Hello");
        assert_eq!(word.start_ms, Some(0));
        assert_eq!(word.end_ms, Some(500));
        assert_eq!(word.speaker_id, Some("Speaker 0".to_string()));
        assert!(word.confidence.is_none());
    }

    #[test]
    fn parse_soniox_token_without_speaker_gives_none_speaker_id() {
        let t = serde_json::json!({
            "text": "world",
            "start_ms": 600,
            "end_ms": 900
        });
        let word = parse_soniox_token(&t).unwrap();
        assert_eq!(word.text, "world");
        assert_eq!(word.speaker_id, None);
    }

    #[test]
    fn parse_soniox_token_string_speaker_is_prefixed() {
        let t = serde_json::json!({
            "text": "yes",
            "start_ms": 100,
            "end_ms": 200,
            "speaker": "A"
        });
        let word = parse_soniox_token(&t).unwrap();
        assert_eq!(word.speaker_id, Some("Speaker A".to_string()));
    }

    #[test]
    fn parse_soniox_token_missing_text_returns_none() {
        let t = serde_json::json!({ "start_ms": 0, "end_ms": 100 });
        assert!(parse_soniox_token(&t).is_none());
    }
}
