use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use log::{info, warn};
use reqwest::Client;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::error::ParakeetError;
use super::messages::{ParakeetCommand, ParakeetResponse, ParakeetVocabularyTerm};
use super::models::{get_available_models, ParakeetModelDefinition, AVAILABLE_MODELS};
use super::sidecar::ParakeetClient;

#[derive(Debug, Clone, Serialize)]
pub struct ParakeetModelStatus {
    pub name: String,
    pub display_name: String,
    pub size: u64,
    pub url: String,
    pub sha256: String,
    pub downloaded: bool,
    pub speed_score: u8,
    pub accuracy_score: u8,
    pub recommended: bool,
    pub engine: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ParakeetVocabularyStatus {
    pub supported: bool,
    pub ready: bool,
}

pub struct ParakeetManager {
    client: ParakeetClient,
    root_dir: PathBuf,
    #[allow(dead_code)]
    http: Client,
}

const PARAKEET_UNAVAILABLE_EVENT: &str = "parakeet-unavailable";

fn fluid_audio_model_dir(home: &Path, definition: &ParakeetModelDefinition) -> PathBuf {
    home.join("Library/Application Support/FluidAudio/Models")
        .join(definition.id)
}

fn model_files_complete(model_dir: &Path, definition: &ParakeetModelDefinition) -> bool {
    definition.files.iter().all(|file| {
        let path = model_dir.join(file.filename);
        path.exists()
    })
}

impl ParakeetManager {
    pub fn new(root_dir: PathBuf) -> Self {
        Self {
            client: ParakeetClient::new("parakeet-sidecar"),
            root_dir,
            http: Client::new(),
        }
    }

    fn model_version_for(definition: &ParakeetModelDefinition) -> &'static str {
        if definition.id.ends_with("-v2") {
            "v2"
        } else {
            "v3"
        }
    }

    /// Returns available Parakeet models for the current architecture.
    ///
    /// **Platform Support**: This returns an empty list on non-macOS platforms.
    /// Parakeet uses Apple's Neural Engine via FluidAudio, which is macOS-only.
    ///
    /// # Platform Behavior
    /// - **macOS (Apple Silicon)**: Returns all Parakeet models
    /// - **macOS (Intel)**: Returns an empty list (Parakeet requires Apple Silicon)
    /// - **Windows/Linux**: Returns empty vector (compile-time exclusion)
    pub fn list_models(&self) -> Vec<ParakeetModelStatus> {
        // Parakeet Swift/FluidAudio integration is macOS-only
        // On non-macOS platforms, this returns empty at compile time
        #[cfg(not(target_os = "macos"))]
        {
            return vec![];
        }

        #[cfg(target_os = "macos")]
        {
            // Use get_available_models() which filters out Apple Silicon-only models on Intel
            get_available_models()
                .into_iter()
                .map(|definition| ParakeetModelStatus {
                    name: definition.id.to_string(),
                    display_name: definition.display_name.to_string(),
                    size: definition.estimated_size,
                    url: format!("https://huggingface.co/{}", definition.repo_id),
                    sha256: String::new(),
                    downloaded: self.is_model_downloaded(definition),
                    speed_score: definition.speed_score,
                    accuracy_score: definition.accuracy_score,
                    recommended: definition.recommended,
                    engine: "parakeet".to_string(),
                })
                .collect()
        }
    }

    pub fn get_model_definition(
        &self,
        model_name: &str,
    ) -> Option<&'static ParakeetModelDefinition> {
        AVAILABLE_MODELS.iter().find(|m| m.id == model_name)
    }

    pub fn model_dir(&self, model_name: &str) -> PathBuf {
        self.root_dir.join(model_name)
    }

    /// Check if a Parakeet model is available.
    /// FluidAudio stores models in ~/Library/Application Support/FluidAudio/Models/<repo-folder>/.
    pub fn is_model_downloaded(&self, definition: &ParakeetModelDefinition) -> bool {
        let Some(home) = dirs::home_dir() else {
            return false;
        };

        let fluid_audio_model_path = fluid_audio_model_dir(&home, definition);
        let complete = model_files_complete(&fluid_audio_model_path, definition);

        if complete {
            info!(
                "Found complete FluidAudio model at: {:?}",
                fluid_audio_model_path
            );
        }

        complete
    }

    pub async fn download_model(
        &self,
        app: &AppHandle,
        model_name: &str,
        cancel_flag: Option<Arc<AtomicBool>>,
        mut progress_callback: impl FnMut(u64, u64, Option<String>) + Send + 'static,
    ) -> Result<(), String> {
        let Some(definition) = self.get_model_definition(model_name) else {
            return Err(format!("Unknown Parakeet model: {model_name}"));
        };

        // For Swift sidecar, delegate download to FluidAudio
        // Send load_model command which triggers download in Swift
        let version = Self::model_version_for(definition);

        let command = ParakeetCommand::LoadModel {
            model_id: definition.id.to_string(),
            model_version: Some(version.to_string()),
            force_download: Some(true),
            local_path: None,
            cache_dir: None,
            precision: "bf16".to_string(),
            attention: "full".to_string(),
            local_attention_context: 256,
            chunk_duration: Some(120.0),
            overlap_duration: Some(15.0),
            eager_unload: Some(false),
        };

        // Send to sidecar and let it handle the download
        let estimated_size = definition.estimated_size;
        let mut last_downloaded = 0;
        match self
            .send_command_with_progress_and_cancel(
                app,
                &command,
                cancel_flag.clone(),
                |progress, phase| {
                    let progress = progress.clamp(0.0, 1.0) as f64;
                    let downloaded = (estimated_size as f64 * progress).round() as u64;
                    last_downloaded = downloaded;
                    progress_callback(downloaded, estimated_size, phase.map(str::to_string));
                },
            )
            .await
        {
            Ok(ParakeetResponse::Status {
                loaded_model: Some(id),
                ..
            }) if id == definition.id => {
                if cancel_flag
                    .as_ref()
                    .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
                {
                    let _ = self.delete_model(app, model_name).await;
                    return Err("Download cancelled by user".to_string());
                }

                // Download/load completed for the requested version
                if last_downloaded < estimated_size {
                    progress_callback(estimated_size, estimated_size, Some("complete".to_string()));
                }
                Ok(())
            }
            Ok(ParakeetResponse::Status {
                loaded_model: Some(other_id),
                ..
            }) => Err(format!(
                "Sidecar loaded '{}' but '{}' was requested",
                other_id, definition.id
            )),
            Ok(ParakeetResponse::Ok { .. }) => {
                Err("Unexpected OK without status payload".to_string())
            }
            Ok(ParakeetResponse::Error { code, message, .. }) => {
                Err(format!("Failed to download model: {}: {}", code, message))
            }
            Err(e) => Err(format!("Failed to communicate with sidecar: {}", e)),
            _ => Err("Unexpected response from sidecar".to_string()),
        }
    }

    pub async fn delete_model(&self, app: &AppHandle, model_name: &str) -> Result<(), String> {
        let Some(definition) = self.get_model_definition(model_name) else {
            return Err(format!("Unknown Parakeet model: {model_name}"));
        };

        let version = Self::model_version_for(definition);

        // Send delete_model command to Swift sidecar to remove FluidAudio cached files
        let command = ParakeetCommand::DeleteModel {
            model_id: Some(definition.id.to_string()),
            model_version: Some(version.to_string()),
        };

        match self.send_command(app, &command).await {
            Ok(ParakeetResponse::Ok { .. }) | Ok(ParakeetResponse::Status { .. }) => {
                // Successfully deleted
            }
            Ok(ParakeetResponse::Error { code, message, .. }) => {
                return Err(format!("Failed to delete model: {}: {}", code, message));
            }
            Err(e) => {
                return Err(format!("Failed to communicate with sidecar: {}", e));
            }
            _ => {
                // Unexpected response but continue
            }
        }

        // Remove our tracking directory if it exists (from old Python implementation)
        let model_dir = self.model_dir(definition.id);
        if model_dir.exists() {
            std::fs::remove_dir_all(&model_dir).map_err(|e| {
                format!(
                    "Failed to delete old model directory {}: {}",
                    model_dir.display(),
                    e
                )
            })?;
        }

        Ok(())
    }

    pub async fn load_model(&self, app: &AppHandle, model_name: &str) -> Result<(), ParakeetError> {
        let Some(definition) = self.get_model_definition(model_name) else {
            return Err(ParakeetError::SpawnError(format!(
                "Unknown Parakeet model: {model_name}"
            )));
        };

        let version = Self::model_version_for(definition);
        let command = ParakeetCommand::LoadModel {
            model_id: definition.id.to_string(),
            model_version: Some(version.to_string()),
            force_download: Some(false),
            local_path: None,
            cache_dir: None,
            precision: "bf16".into(),
            attention: "local".into(),
            local_attention_context: 256,
            chunk_duration: Some(120.0),
            overlap_duration: Some(15.0),
            eager_unload: Some(false),
        };

        match self.send_command(app, &command).await? {
            ParakeetResponse::Ok { .. } => Ok(()),
            ParakeetResponse::Status {
                loaded_model,
                model_version,
                ..
            } => {
                let expected_v = version.to_string();
                let ok = match (loaded_model.as_deref(), model_version.as_deref()) {
                    (Some(id), mv) if mv == Some(expected_v.as_str()) => {
                        // Accept exact match ("parakeet-...-v2") or prefix match for id variants
                        id == definition.id
                            || id == format!("{}-{}", definition.id, expected_v)
                            || id.starts_with(definition.id)
                    }
                    _ => false,
                };
                if ok {
                    Ok(())
                } else {
                    Err(ParakeetError::SidecarError {
                        code: "load_mismatch".to_string(),
                        message: format!(
                            "Sidecar loaded '{:?}' (version {:?}) but '{}' (version {}) was requested",
                            loaded_model, model_version, definition.id, expected_v
                        ),
                    })
                }
            }
            ParakeetResponse::Error { code, message, .. } => {
                Err(ParakeetError::SidecarError { code, message })
            }
            other => Err(ParakeetError::SidecarError {
                code: "unexpected_response".to_string(),
                message: format!("Unexpected response: {:?}", other),
            }),
        }
    }

    pub fn vocabulary_status_from_response(
        response: &ParakeetResponse,
    ) -> Option<ParakeetVocabularyStatus> {
        match response {
            ParakeetResponse::Status {
                custom_vocabulary_supported,
                custom_vocabulary_ready,
                ..
            } => Some(ParakeetVocabularyStatus {
                supported: *custom_vocabulary_supported,
                ready: *custom_vocabulary_ready,
            }),
            _ => None,
        }
    }

    pub async fn status(&self, app: &AppHandle) -> Result<ParakeetResponse, ParakeetError> {
        self.send_command(app, &ParakeetCommand::Status {}).await
    }

    pub async fn download_ctc_models(
        &self,
        app: &AppHandle,
    ) -> Result<ParakeetResponse, ParakeetError> {
        self.send_command(app, &ParakeetCommand::DownloadCtcModels {})
            .await
    }

    pub async fn transcribe(
        &self,
        app: &AppHandle,
        model_name: &str,
        audio_path: PathBuf,
        language: Option<String>,
        translate: bool,
    ) -> Result<ParakeetResponse, ParakeetError> {
        self.transcribe_with_custom_vocabulary(
            app,
            model_name,
            audio_path,
            language,
            translate,
            Vec::new(),
        )
        .await
    }

    pub async fn transcribe_with_custom_vocabulary(
        &self,
        app: &AppHandle,
        _model_name: &str,
        audio_path: PathBuf,
        language: Option<String>,
        translate: bool,
        custom_vocabulary: Vec<ParakeetVocabularyTerm>,
    ) -> Result<ParakeetResponse, ParakeetError> {
        let command = ParakeetCommand::Transcribe {
            audio_path: audio_path.to_string_lossy().to_string(),
            language,
            translate_to_english: translate,
            prompt: None,
            use_word_timestamps: Some(true),
            chunk_duration: None,
            overlap_duration: None,
            attention: None,
            local_attention_context: None,
            custom_vocabulary: (!custom_vocabulary.is_empty()).then_some(custom_vocabulary),
        };

        self.send_command(app, &command).await
    }

    pub async fn diarize(
        &self,
        app: &AppHandle,
        audio_path: PathBuf,
    ) -> Result<ParakeetResponse, ParakeetError> {
        let command = ParakeetCommand::Diarize {
            audio_path: audio_path.to_string_lossy().to_string(),
        };

        self.send_command(app, &command).await
    }

    /// Check if the Parakeet sidecar is healthy and can respond to commands
    #[allow(dead_code)]
    pub async fn health_check(&self, app: &AppHandle) -> Result<bool, ParakeetError> {
        match self.send_command(app, &ParakeetCommand::Status {}).await {
            Ok(ParakeetResponse::Status { .. }) => Ok(true),
            Ok(_) => Ok(true), // Any successful response is good
            Err(e) => {
                warn!("Parakeet sidecar health check failed: {:?}", e);
                Err(e)
            }
        }
    }

    #[allow(dead_code)]
    pub async fn shutdown(&self) {
        self.client.shutdown().await;
    }

    fn friendly_spawn_message(details: &str) -> String {
        format!(
            "Parakeet is unavailable. Please reinstall VoiceTypr or remove the quarantine flag by running `xattr -dr com.apple.quarantine /Applications/VoiceTypr.app`. Details: {}",
            details
        )
    }

    fn emit_unavailable(app: &AppHandle, message: &str) {
        if let Err(err) = app.emit(PARAKEET_UNAVAILABLE_EVENT, message.to_string()) {
            warn!("Failed to emit Parakeet unavailable event: {err:?}");
        }
    }

    async fn send_command(
        &self,
        app: &AppHandle,
        command: &ParakeetCommand,
    ) -> Result<ParakeetResponse, ParakeetError> {
        match self.client.send(app, command).await {
            Ok(response) => Ok(response),
            Err(ParakeetError::SpawnError(details)) => {
                let message = Self::friendly_spawn_message(&details);
                Self::emit_unavailable(app, &message);
                Err(ParakeetError::Unavailable(message))
            }
            Err(err) => Err(err),
        }
    }

    async fn send_command_with_progress_and_cancel<F>(
        &self,
        app: &AppHandle,
        command: &ParakeetCommand,
        cancel_flag: Option<Arc<AtomicBool>>,
        progress_callback: F,
    ) -> Result<ParakeetResponse, ParakeetError>
    where
        F: FnMut(f32, Option<&str>),
    {
        match self
            .client
            .send_with_progress_and_cancel(app, command, cancel_flag, progress_callback)
            .await
        {
            Ok(response) => Ok(response),
            Err(ParakeetError::SpawnError(details)) => {
                let message = Self::friendly_spawn_message(&details);
                Self::emit_unavailable(app, &message);
                Err(ParakeetError::Unavailable(message))
            }
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{fluid_audio_model_dir, model_files_complete};
    use crate::parakeet::models::AVAILABLE_MODELS;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn fluid_audio_model_dir_matches_fluidaudio_cache_shape() {
        let temp = TempDir::new().expect("temp dir");
        let definition = &AVAILABLE_MODELS[0];

        assert_eq!(
            fluid_audio_model_dir(temp.path(), definition),
            temp.path()
                .join("Library/Application Support/FluidAudio/Models")
                .join(definition.id)
        );
    }

    #[test]
    fn model_files_complete_rejects_partial_cache() {
        let temp = TempDir::new().expect("temp dir");
        let definition = &AVAILABLE_MODELS[0];
        let model_dir = temp.path().join(definition.id);
        fs::create_dir_all(&model_dir).expect("model dir");
        fs::create_dir_all(model_dir.join(definition.files[0].filename)).expect("one model file");

        assert!(!model_files_complete(&model_dir, definition));
    }

    #[test]
    fn model_files_complete_accepts_required_files() {
        let temp = TempDir::new().expect("temp dir");
        let definition = &AVAILABLE_MODELS[0];
        let model_dir = temp.path().join(definition.id);
        fs::create_dir_all(&model_dir).expect("model dir");

        for file in definition.files {
            let path = model_dir.join(file.filename);
            if file.filename.ends_with(".mlmodelc") {
                fs::create_dir_all(path).expect("model package");
            } else {
                fs::write(path, b"{}").expect("model asset");
            }
        }

        assert!(model_files_complete(&model_dir, definition));
    }
}
