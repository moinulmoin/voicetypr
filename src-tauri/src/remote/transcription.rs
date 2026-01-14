//! Real transcription context for the remote HTTP server
//!
//! Implements ServerContext using actual Whisper transcription.

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Instant;
use tempfile::NamedTempFile;

use super::http::ServerContext;
use super::server::TranscribeResponse;
use crate::whisper::cache::TranscriberCache;

/// Configuration for the transcription server
#[derive(Clone)]
pub struct TranscriptionServerConfig {
    /// Server's display name (e.g., "Desktop-PC")
    pub server_name: String,
    /// Password for authentication (None = no auth required)
    pub password: Option<String>,
    /// Path to the currently selected model
    pub model_path: PathBuf,
    /// Name of the current model (e.g., "large-v3-turbo")
    pub model_name: String,
}

/// Real transcription context that uses Whisper
///
/// Uses std::sync::Mutex for the cache because transcription is inherently
/// blocking/CPU-bound and we want to serialize requests (as per design).
pub struct RealTranscriptionContext {
    config: TranscriptionServerConfig,
    /// Cache for loaded transcriber models - uses std Mutex for blocking access
    cache: Arc<StdMutex<TranscriberCache>>,
}

impl RealTranscriptionContext {
    /// Create a new transcription context
    pub fn new(config: TranscriptionServerConfig) -> Self {
        Self {
            config,
            cache: Arc::new(StdMutex::new(TranscriberCache::new())),
        }
    }

    /// Update the model being served
    pub fn update_model(&mut self, model_path: PathBuf, model_name: String) {
        self.config.model_path = model_path;
        self.config.model_name = model_name;
    }

    /// Update the password
    pub fn update_password(&mut self, password: Option<String>) {
        self.config.password = password;
    }

    /// Get the current model path
    pub fn get_model_path(&self) -> &PathBuf {
        &self.config.model_path
    }
}

impl ServerContext for RealTranscriptionContext {
    fn get_model_name(&self) -> String {
        self.config.model_name.clone()
    }

    fn get_server_name(&self) -> String {
        self.config.server_name.clone()
    }

    fn get_password(&self) -> Option<String> {
        self.config.password.clone()
    }

    fn transcribe(&self, audio_data: &[u8]) -> Result<TranscribeResponse, String> {
        let start = Instant::now();

        log::info!(
            "Starting remote transcription: {} bytes of audio",
            audio_data.len()
        );

        // Validate audio data is not empty
        if audio_data.is_empty() {
            return Err("Empty audio data".to_string());
        }

        // Write audio data to a temporary WAV file
        let mut temp_file = NamedTempFile::new()
            .map_err(|e| format!("Failed to create temp file: {}", e))?;

        temp_file
            .write_all(audio_data)
            .map_err(|e| format!("Failed to write audio data: {}", e))?;

        temp_file
            .flush()
            .map_err(|e| format!("Failed to flush temp file: {}", e))?;

        let temp_path = temp_file.path().to_path_buf();

        log::info!("Audio written to temp file: {:?}", temp_path);

        // Get transcriber from cache (blocking lock - serializes all transcriptions)
        let transcriber = {
            let mut cache = self
                .cache
                .lock()
                .map_err(|e| format!("Failed to acquire cache lock: {}", e))?;

            cache
                .get_or_create(&self.config.model_path)
                .map_err(|e| format!("Failed to load model: {}", e))?
        };

        log::info!("Model loaded, starting transcription...");

        // Perform transcription (this can take a while)
        // Note: transcriber is Arc<Transcriber>, safe to use after releasing cache lock
        let text = transcriber
            .transcribe_with_translation(&temp_path, None, false)
            .map_err(|e| format!("Transcription failed: {}", e))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        log::info!(
            "Remote transcription completed: {} chars in {}ms using {}",
            text.len(),
            duration_ms,
            self.config.model_name
        );

        Ok(TranscribeResponse {
            text,
            duration_ms,
            model: self.config.model_name.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transcription_server_config() {
        let config = TranscriptionServerConfig {
            server_name: "Test Server".to_string(),
            password: Some("secret".to_string()),
            model_path: PathBuf::from("/models/test.bin"),
            model_name: "test-model".to_string(),
        };

        assert_eq!(config.server_name, "Test Server");
        assert_eq!(config.password, Some("secret".to_string()));
        assert_eq!(config.model_name, "test-model");
    }

    #[test]
    fn test_real_context_creation() {
        let config = TranscriptionServerConfig {
            server_name: "Desktop-PC".to_string(),
            password: None,
            model_path: PathBuf::from("/models/large-v3-turbo.bin"),
            model_name: "large-v3-turbo".to_string(),
        };

        let ctx = RealTranscriptionContext::new(config);

        assert_eq!(ctx.get_model_name(), "large-v3-turbo");
        assert_eq!(ctx.get_server_name(), "Desktop-PC");
        assert!(ctx.get_password().is_none());
    }

    #[test]
    fn test_context_with_password() {
        let config = TranscriptionServerConfig {
            server_name: "Secure Server".to_string(),
            password: Some("mypassword".to_string()),
            model_path: PathBuf::from("/models/base.en.bin"),
            model_name: "base.en".to_string(),
        };

        let ctx = RealTranscriptionContext::new(config);

        assert_eq!(ctx.get_password(), Some("mypassword".to_string()));
    }

    #[test]
    fn test_context_update_model() {
        let config = TranscriptionServerConfig {
            server_name: "Test".to_string(),
            password: None,
            model_path: PathBuf::from("/models/old.bin"),
            model_name: "old-model".to_string(),
        };

        let mut ctx = RealTranscriptionContext::new(config);

        ctx.update_model(
            PathBuf::from("/models/new.bin"),
            "new-model".to_string(),
        );

        assert_eq!(ctx.get_model_name(), "new-model");
        assert_eq!(ctx.get_model_path(), &PathBuf::from("/models/new.bin"));
    }

    #[test]
    fn test_context_update_password() {
        let config = TranscriptionServerConfig {
            server_name: "Test".to_string(),
            password: None,
            model_path: PathBuf::from("/models/test.bin"),
            model_name: "test".to_string(),
        };

        let mut ctx = RealTranscriptionContext::new(config);

        assert!(ctx.get_password().is_none());

        ctx.update_password(Some("newsecret".to_string()));
        assert_eq!(ctx.get_password(), Some("newsecret".to_string()));

        ctx.update_password(None);
        assert!(ctx.get_password().is_none());
    }

    #[test]
    fn test_transcribe_empty_audio_returns_error() {
        let config = TranscriptionServerConfig {
            server_name: "Test".to_string(),
            password: None,
            model_path: PathBuf::from("/models/test.bin"),
            model_name: "test".to_string(),
        };

        let ctx = RealTranscriptionContext::new(config);

        // Empty audio should return error
        let result = ctx.transcribe(&[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Empty audio data"));
    }

    #[test]
    fn test_transcribe_invalid_model_path_returns_error() {
        let config = TranscriptionServerConfig {
            server_name: "Test".to_string(),
            password: None,
            model_path: PathBuf::from("/nonexistent/model.bin"),
            model_name: "test".to_string(),
        };

        let ctx = RealTranscriptionContext::new(config);

        // Some fake audio data (not valid WAV, but tests path validation)
        let result = ctx.transcribe(&[1, 2, 3, 4, 5]);
        assert!(result.is_err());
        // Should fail because model doesn't exist
        assert!(result.unwrap_err().contains("Failed to load model"));
    }
}
