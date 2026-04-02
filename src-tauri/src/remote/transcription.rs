//! Real transcription context for the remote HTTP server
//!
//! Implements ServerContext using actual Whisper or Parakeet transcription.

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, RwLock};
use std::time::Instant;
use tauri::AppHandle;
use tempfile::NamedTempFile;

use super::http::ServerContext;
use super::server::TranscribeResponse;
use crate::parakeet::messages::ParakeetResponse;
use crate::parakeet::ParakeetManager;
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

/// Coherent snapshot of remote model state
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SharedServerStateSnapshot {
    /// Name of the model currently configured for remote transcription.
    pub model_name: String,
    /// Path to the model artifact currently configured for this context.
    pub model_path: PathBuf,
    /// Engine used for remote transcription.
    pub engine: String,
}

impl Default for SharedServerStateSnapshot {
    fn default() -> Self {
        Self {
            model_name: String::new(),
            model_path: PathBuf::new(),
            engine: "whisper".to_string(),
        }
    }
}

/// Shared state that can be updated while the server is running.
#[derive(Clone)]
pub struct SharedServerState {
    /// Coherent snapshot of model configuration shared across threads.
    snapshot: Arc<RwLock<SharedServerStateSnapshot>>,
}

impl SharedServerState {
    /// Create new shared state from initial values
    pub fn new(model_name: String, model_path: PathBuf, engine: String) -> Self {
        Self {
            snapshot: Arc::new(RwLock::new(SharedServerStateSnapshot {
                model_name,
                model_path,
                engine,
            })),
        }
    }

    /// Update the model atomically
    pub fn update_model(&self, model_name: String, model_path: PathBuf, engine: String) {
        if let Ok(mut snapshot) = self.snapshot.write() {
            *snapshot = SharedServerStateSnapshot {
                model_name,
                model_path,
                engine,
            };
        }
    }

    /// Read the current model configuration snapshot
    pub(crate) fn get_snapshot(&self) -> SharedServerStateSnapshot {
        self.snapshot
            .read()
            .map(|snapshot| snapshot.clone())
            .unwrap_or_default()
    }

    /// Get the current model name
    pub fn get_model_name(&self) -> String {
        self.get_snapshot().model_name
    }

    /// Get the current model path
    pub fn get_model_path(&self) -> PathBuf {
        self.get_snapshot().model_path
    }

    /// Get the current engine
    pub fn get_engine(&self) -> String {
        self.get_snapshot().engine
    }
}

/// Real transcription context that uses Whisper or Parakeet
///
/// Uses std::sync::Mutex for the cache because transcription is inherently
/// blocking/CPU-bound and we want to serialize requests (as per design).
pub struct RealTranscriptionContext {
    /// Server name (static)
    server_name: String,
    /// Password for authentication
    password: Option<String>,
    /// Shared state for dynamic model updates
    shared_state: SharedServerState,
    /// Cache for loaded transcriber models - uses std Mutex for blocking access
    cache: Arc<StdMutex<TranscriberCache>>,
    /// AppHandle for accessing Parakeet manager (optional, needed for parakeet engine)
    app_handle: Option<AppHandle>,
}

impl RealTranscriptionContext {
    /// Create a new transcription context with shared state and AppHandle
    pub fn new_with_shared_state(
        server_name: String,
        password: Option<String>,
        shared_state: SharedServerState,
        app_handle: Option<AppHandle>,
    ) -> Self {
        Self {
            server_name,
            password,
            shared_state,
            cache: Arc::new(StdMutex::new(TranscriberCache::new())),
            app_handle,
        }
    }

    /// Create a new transcription context (legacy, creates its own shared state)
    pub fn new(config: TranscriptionServerConfig) -> Self {
        // Default to whisper engine for legacy compatibility
        let shared_state =
            SharedServerState::new(config.model_name, config.model_path, "whisper".to_string());
        Self {
            server_name: config.server_name,
            password: config.password,
            shared_state,
            cache: Arc::new(StdMutex::new(TranscriberCache::new())),
            app_handle: None,
        }
    }

    /// Update the model being served
    pub fn update_model(&mut self, model_path: PathBuf, model_name: String, engine: String) {
        self.shared_state
            .update_model(model_name, model_path, engine);
    }

    /// Update the password
    pub fn update_password(&mut self, password: Option<String>) {
        self.password = password;
    }

    /// Get the current model path
    pub fn get_model_path(&self) -> PathBuf {
        self.shared_state.get_model_path()
    }

    /// Get the current model snapshot (coherent read)
    pub(crate) fn get_model_snapshot(&self) -> SharedServerStateSnapshot {
        self.shared_state.get_snapshot()
    }

    /// Get the shared state (for external updates)
    pub fn get_shared_state(&self) -> SharedServerState {
        self.shared_state.clone()
    }
}

impl ServerContext for RealTranscriptionContext {
    fn get_model_name(&self) -> String {
        // Read current model name from shared state (dynamic)
        self.shared_state.get_model_name()
    }

    fn get_server_name(&self) -> String {
        self.server_name.clone()
    }

    fn get_password(&self) -> Option<String> {
        self.password.clone()
    }

    fn transcribe(&self, audio_data: &[u8]) -> Result<TranscribeResponse, String> {
        let start = Instant::now();

        // Capture a single coherent snapshot for this request
        let snapshot = self.shared_state.get_snapshot();

        log::info!(
            "Starting remote transcription: {} bytes of audio, engine='{}', model='{}'",
            audio_data.len(),
            snapshot.engine,
            snapshot.model_name
        );

        // Validate audio data is not empty
        if audio_data.is_empty() {
            return Err("Empty audio data".to_string());
        }

        // Write audio data to a temporary WAV file
        let mut temp_file =
            NamedTempFile::new().map_err(|e| format!("Failed to create temp file: {}", e))?;

        temp_file
            .write_all(audio_data)
            .map_err(|e| format!("Failed to write audio data: {}", e))?;

        temp_file
            .flush()
            .map_err(|e| format!("Failed to flush temp file: {}", e))?;

        let temp_path = temp_file.path().to_path_buf();

        log::info!("Audio written to temp file: {:?}", temp_path);

        // Route to appropriate transcription engine
        let text = match snapshot.engine.as_str() {
            "parakeet" => {
                // Use Parakeet sidecar for transcription
                self.transcribe_with_parakeet(&temp_path, &snapshot.model_name)?
            }
            _ => {
                // Default to Whisper transcription
                self.transcribe_with_whisper(&temp_path, &snapshot.model_path)?
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        log::info!(
            "Remote transcription completed: {} chars in {}ms using {} ({})",
            text.len(),
            duration_ms,
            snapshot.model_name,
            snapshot.engine
        );

        Ok(TranscribeResponse {
            text,
            duration_ms,
            model: snapshot.model_name,
        })
    }
}

impl RealTranscriptionContext {
    /// Transcribe using Whisper model
    fn transcribe_with_whisper(
        &self,
        audio_path: &PathBuf,
        model_path: &PathBuf,
    ) -> Result<String, String> {
        // Get transcriber from cache (blocking lock - serializes all transcriptions)
        let transcriber = {
            let mut cache = self
                .cache
                .lock()
                .map_err(|e| format!("Failed to acquire cache lock: {}", e))?;

            cache
                .get_or_create(&model_path)
                .map_err(|e| format!("Failed to load Whisper model: {}", e))?
        };

        log::info!("Whisper model loaded, starting transcription...");

        // Perform transcription (this can take a while)
        transcriber
            .transcribe_with_translation(audio_path, None, false)
            .map_err(|e| format!("Whisper transcription failed: {}", e))
    }

    /// Transcribe using Parakeet sidecar
    fn transcribe_with_parakeet(
        &self,
        audio_path: &PathBuf,
        model_name: &str,
    ) -> Result<String, String> {
        let app_handle = self.app_handle.as_ref().ok_or_else(|| {
            "Parakeet transcription requires AppHandle but none was provided".to_string()
        })?;

        log::info!(
            "Using Parakeet engine for transcription with model '{}'",
            model_name
        );

        // Get the ParakeetManager from app state
        use tauri::Manager;
        let parakeet_manager = app_handle.state::<ParakeetManager>();

        // Clone what we need for the async block
        let app_handle_clone = app_handle.clone();
        let model_name_owned = model_name.to_string();
        let audio_path_clone = audio_path.clone();

        // Run the async Parakeet transcription in a blocking context
        // Since we're already in a sync context (ServerContext::transcribe), use block_on
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                // First, ensure the model is loaded
                parakeet_manager
                    .load_model(&app_handle_clone, &model_name_owned)
                    .await
                    .map_err(|e| format!("Failed to load Parakeet model: {}", e))?;

                // Perform transcription
                let response = parakeet_manager
                    .transcribe(
                        &app_handle_clone,
                        &model_name_owned,
                        audio_path_clone,
                        None,  // language
                        false, // translate
                    )
                    .await
                    .map_err(|e| format!("Parakeet transcription failed: {}", e))?;

                // Extract text from the ParakeetResponse enum
                match response {
                    ParakeetResponse::Transcription { text, .. } => Ok::<String, String>(text),
                    ParakeetResponse::Error { code, message, .. } => {
                        Err(format!("Parakeet error {}: {}", code, message))
                    }
                    other => Err(format!("Unexpected Parakeet response: {:?}", other)),
                }
            })
        });

        result
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
            "whisper".to_string(),
        );

        assert_eq!(ctx.get_model_name(), "new-model");
        assert_eq!(ctx.get_model_path(), PathBuf::from("/models/new.bin"));
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
        let error = result.unwrap_err();
        assert!(
            error.contains("Failed to load") || error.contains("No such file"),
            "unexpected error: {error}"
        );
    }
}
