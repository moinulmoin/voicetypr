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
    transcribe_at("https://api.deepgram.com", key, audio_path, language)
        .await
        .map_err(|e| e.message("Deepgram"))
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

#[cfg(test)]
mod tests {
    use super::{common, transcribe_at};
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
}
