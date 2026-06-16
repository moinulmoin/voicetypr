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

pub(super) async fn transcribe_typed(
    _app: &AppHandle,
    key: &str,
    audio_path: &Path,
    language: Option<&str>,
) -> Result<String, common::SttError> {
    transcribe_at("https://api.deepgram.com", key, audio_path, language).await
}

pub(super) async fn transcribe_at(
    base_url: &str,
    key: &str,
    audio_path: &Path,
    language: Option<&str>,
) -> Result<String, common::SttError> {
    use tokio::fs;

    let bytes = fs::read(audio_path)
        .await
        .map_err(|_| common::SttError::BadResponse)?;

    let mut url = format!("{}/v1/listen?model={}&smart_format=true", base_url, MODEL);
    if let Some(lang) = language.map(str::trim).filter(|lang| !lang.is_empty()) {
        url.push_str("&language=");
        url.push_str(lang);
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
}

pub(super) async fn transcribe_typed_diarized(
    _app: &AppHandle,
    key: &str,
    audio_path: &Path,
    language: Option<&str>,
) -> Result<super::CloudTranscript, common::SttError> {
    transcribe_at_diarized("https://api.deepgram.com", key, audio_path, language).await
}

pub(super) async fn transcribe_at_diarized(
    base_url: &str,
    key: &str,
    audio_path: &Path,
    language: Option<&str>,
) -> Result<super::CloudTranscript, common::SttError> {
    use tokio::fs;

    let bytes = fs::read(audio_path)
        .await
        .map_err(|_| common::SttError::BadResponse)?;

    let mut url = format!(
        "{}/v1/listen?model={}&smart_format=true&diarize=true",
        base_url, MODEL
    );
    if let Some(lang) = language.map(str::trim).filter(|lang| !lang.is_empty()) {
        url.push_str("&language=");
        url.push_str(lang);
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
                return Err(common::log_http_body(resp, "Deepgram diarized transcription").await);
            }

            let json: serde_json::Value = resp
                .json()
                .await
                .map_err(|_| common::SttError::BadResponse)?;
            parse_diarized_response(json).ok_or(common::SttError::BadResponse)
        }
    })
    .await
}

fn parse_diarized_response(json: serde_json::Value) -> Option<super::CloudTranscript> {
    let alt = json.pointer("/results/channels/0/alternatives/0")?;
    let text = alt.get("transcript")?.as_str()?.to_string();
    let words = alt
        .get("words")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(parse_deepgram_word).collect())
        .unwrap_or_default();
    Some(super::CloudTranscript { text, words })
}

fn parse_deepgram_word(w: &serde_json::Value) -> Option<crate::transcription::TranscriptionWord> {
    let text = w
        .get("punctuated_word")
        .and_then(|v| v.as_str())
        .or_else(|| w.get("word").and_then(|v| v.as_str()))?
        .to_string();
    let start_ms = w
        .get("start")
        .and_then(|v| v.as_f64())
        .map(|s| (s * 1000.0).round() as u64);
    let end_ms = w
        .get("end")
        .and_then(|v| v.as_f64())
        .map(|s| (s * 1000.0).round() as u64);
    let speaker_id = w
        .get("speaker")
        .and_then(|v| v.as_i64())
        .map(|n| format!("Speaker {n}"));
    let confidence = w
        .get("confidence")
        .and_then(|v| v.as_f64())
        .map(|c| c as f32);
    Some(crate::transcription::TranscriptionWord {
        text,
        start_ms,
        end_ms,
        speaker_id,
        confidence,
    })
}

#[cfg(test)]
mod tests {
    use super::{common, parse_deepgram_word, parse_diarized_response, transcribe_at};
    use std::io::Write;
    use tempfile::NamedTempFile;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn audio_file() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"wav").unwrap();
        file
    }

    #[tokio::test]
    async fn transcribe_at_posts_token_auth_raw_body_and_parses_nested_transcript() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/listen"))
            .and(header("authorization", "Token k"))
            .and(header("content-type", "audio/wav"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": {
                    "channels": [{
                        "alternatives": [{
                            "transcript": "hi"
                        }]
                    }]
                }
            })))
            .expect(1)
            .mount(&server)
            .await;
        let audio = audio_file();

        let text = transcribe_at(&server.uri(), "k", audio.path(), Some("en"))
            .await
            .unwrap();

        assert_eq!(text, "hi");
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert!(!requests[0].body.is_empty());
        assert_eq!(
            requests[0].url.query(),
            Some("model=nova-3&smart_format=true&language=en")
        );
    }

    #[tokio::test]
    async fn transcribe_at_maps_auth_error_without_retry() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/listen"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;
        let audio = audio_file();

        let error = transcribe_at(&server.uri(), "k", audio.path(), None)
            .await
            .unwrap_err();

        assert!(matches!(error, common::SttError::Auth));
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[test]
    fn parse_deepgram_word_uses_punctuated_word_and_maps_fields() {
        let w = serde_json::json!({
            "word": "hello",
            "punctuated_word": "Hello,",
            "start": 0.5,
            "end": 1.25,
            "confidence": 0.98,
            "speaker": 1
        });
        let word = parse_deepgram_word(&w).unwrap();
        assert_eq!(word.text, "Hello,");
        assert_eq!(word.start_ms, Some(500));
        assert_eq!(word.end_ms, Some(1250));
        assert_eq!(word.speaker_id, Some("Speaker 1".to_string()));
        assert!((word.confidence.unwrap() - 0.98_f32).abs() < 0.001);
    }

    #[test]
    fn parse_deepgram_word_falls_back_to_word_when_no_punctuated() {
        let w = serde_json::json!({
            "word": "world",
            "start": 1.0,
            "end": 1.5,
            "confidence": 0.9,
            "speaker": 0
        });
        let word = parse_deepgram_word(&w).unwrap();
        assert_eq!(word.text, "world");
        assert_eq!(word.speaker_id, Some("Speaker 0".to_string()));
    }

    #[test]
    fn parse_deepgram_word_returns_none_for_missing_text() {
        let w = serde_json::json!({ "start": 0.0, "end": 0.5 });
        assert!(parse_deepgram_word(&w).is_none());
    }

    #[test]
    fn parse_diarized_response_two_speakers_multiple_words() {
        let json = serde_json::json!({
            "results": {
                "channels": [{
                    "alternatives": [{
                        "transcript": "Hello world thanks",
                        "words": [
                            {
                                "word": "hello", "punctuated_word": "Hello",
                                "start": 0.0, "end": 0.5,
                                "confidence": 0.99, "speaker": 0
                            },
                            {
                                "word": "world", "punctuated_word": "world",
                                "start": 0.6, "end": 1.1,
                                "confidence": 0.95, "speaker": 0
                            },
                            {
                                "word": "thanks", "punctuated_word": "thanks.",
                                "start": 1.5, "end": 2.0,
                                "confidence": 0.88, "speaker": 1
                            }
                        ]
                    }]
                }]
            }
        });
        let ct = parse_diarized_response(json).unwrap();
        assert_eq!(ct.text, "Hello world thanks");
        assert_eq!(ct.words.len(), 3);
        assert_eq!(ct.words[0].text, "Hello");
        assert_eq!(ct.words[0].speaker_id, Some("Speaker 0".to_string()));
        assert_eq!(ct.words[0].start_ms, Some(0));
        assert_eq!(ct.words[0].end_ms, Some(500));
        assert_eq!(ct.words[2].text, "thanks.");
        assert_eq!(ct.words[2].speaker_id, Some("Speaker 1".to_string()));
        assert_eq!(ct.words[2].start_ms, Some(1500));
        assert_eq!(ct.words[2].end_ms, Some(2000));
    }

    #[test]
    fn parse_diarized_response_no_words_field_gives_empty_vec() {
        let json = serde_json::json!({
            "results": {
                "channels": [{
                    "alternatives": [{
                        "transcript": "hi there"
                    }]
                }]
            }
        });
        let ct = parse_diarized_response(json).unwrap();
        assert_eq!(ct.text, "hi there");
        assert!(ct.words.is_empty());
    }
}
