use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use log::{info, warn};
use reqwest::Client;
use serde::Serialize;
use tauri::AppHandle;
use tokio::io::AsyncWriteExt;

use super::error::ParakeetError;
use super::messages::{ParakeetCommand, ParakeetResponse};
use super::models::{ParakeetModelDefinition, AVAILABLE_MODELS};
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

pub struct ParakeetManager {
    client: ParakeetClient,
    root_dir: PathBuf,
    http: Client,
}

impl ParakeetManager {
    pub fn new(root_dir: PathBuf) -> Self {
        Self {
            client: ParakeetClient::new("parakeet-sidecar"),
            root_dir,
            http: Client::new(),
        }
    }

    pub fn list_models(&self) -> Vec<ParakeetModelStatus> {
        // Parakeet Swift/FluidAudio integration is macOS-only
        #[cfg(not(target_os = "macos"))]
        {
            return vec![];
        }

        #[cfg(target_os = "macos")]
        {
            AVAILABLE_MODELS
                .iter()
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

    pub fn get_model_definition(&self, model_name: &str) -> Option<&'static ParakeetModelDefinition> {
        AVAILABLE_MODELS.iter().find(|m| m.id == model_name)
    }

    pub fn model_dir(&self, model_name: &str) -> PathBuf {
        self.root_dir.join(model_name)
    }

    /// Check if a Parakeet model is available.
    /// FluidAudio stores models in ~/Library/Application Support/FluidAudio/Models/
    pub fn is_model_downloaded(&self, definition: &ParakeetModelDefinition) -> bool {
        if let Some(home) = dirs::home_dir() {
            // FluidAudio's actual model storage path
            // IMPORTANT: "Application Support" has a SPACE (macOS standard path)
            let fluid_audio_models_path = home
                .join("Library/Application Support/FluidAudio/Models")
                .join(format!("{}-coreml", definition.id));
            
            if fluid_audio_models_path.exists() {
                // Check if directory contains model files
                if let Ok(entries) = std::fs::read_dir(&fluid_audio_models_path) {
                    if entries.count() > 0 {
                        info!("Found FluidAudio model at: {:?}", fluid_audio_models_path);
                        return true;
                    }
                }
            }
        }

        // Fallback: check if old MLX model directory exists (for backward compatibility)
        let model_dir = self.model_dir(definition.id);
        if model_dir.exists() && model_dir.read_dir().map(|mut d| d.next().is_some()).unwrap_or(false) {
            return true;
        }

        false
    }

    pub async fn download_model(
        &self,
        app: &AppHandle,
        model_name: &str,
        cancel_flag: Option<Arc<AtomicBool>>,
        progress_callback: impl Fn(u64, u64) + Send + 'static,
    ) -> Result<(), String> {
        let Some(definition) = self.get_model_definition(model_name) else {
            return Err(format!("Unknown Parakeet model: {model_name}"));
        };

        // For Swift sidecar, delegate download to FluidAudio
        // Send load_model command which triggers download in Swift
        let command = ParakeetCommand::LoadModel {
            model_id: definition.id.to_string(),
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
        match self.client.send(app, &command).await {
            Ok(ParakeetResponse::Ok { .. }) | Ok(ParakeetResponse::Status { loaded_model: Some(_), .. }) => {
                // Swift sidecar returns StatusResponse with loaded_model when download completes
                // Mark as downloaded since Swift handles it
                progress_callback(definition.estimated_size, definition.estimated_size);
                Ok(())
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

        // Send delete_model command to Swift sidecar to remove FluidAudio cached files
        match self.client.send(app, &ParakeetCommand::DeleteModel {}).await {
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
            std::fs::remove_dir_all(&model_dir)
                .map_err(|e| format!("Failed to delete old model directory {}: {}", model_dir.display(), e))?;
        }

        Ok(())
    }

    pub async fn load_model(
        &self,
        app: &AppHandle,
        model_name: &str,
    ) -> Result<(), ParakeetError> {
        let Some(definition) = self.get_model_definition(model_name) else {
            return Err(ParakeetError::SpawnError(format!(
                "Unknown Parakeet model: {model_name}"
            )));
        };

        let model_dir = self.model_dir(definition.id);
        let command = ParakeetCommand::LoadModel {
            model_id: definition.repo_id.to_string(),
            local_path: Some(model_dir.to_string_lossy().to_string()),
            cache_dir: Some(self.root_dir.to_string_lossy().to_string()),
            precision: "bf16".into(),
            attention: "local".into(),
            local_attention_context: 256,
            chunk_duration: Some(120.0),
            overlap_duration: Some(15.0),
            eager_unload: Some(false),
        };

        match self.client.send(app, &command).await? {
            ParakeetResponse::Ok { .. } => Ok(()),
            ParakeetResponse::Error { code, message, .. } => {
                Err(ParakeetError::SidecarError { code, message })
            }
            other => {
                info!("Unexpected response while loading model: {:?}", other);
                Ok(())
            }
        }
    }

    pub async fn transcribe(
        &self,
        app: &AppHandle,
        _model_name: &str,
        audio_path: PathBuf,
        language: Option<String>,
        translate: bool,
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
        };

        self.client.send(app, &command).await
    }

    fn file_url(&self, definition: &ParakeetModelDefinition, filename: &str) -> String {
        format!(
            "https://huggingface.co/{}/resolve/main/{}",
            definition.repo_id, filename
        )
    }

    /// Check if the Parakeet sidecar is healthy and can respond to commands
    pub async fn health_check(&self, app: &AppHandle) -> Result<bool, ParakeetError> {
        match self.client.send(app, &ParakeetCommand::Status {}).await {
            Ok(ParakeetResponse::Status { .. }) => Ok(true),
            Ok(_) => Ok(true), // Any successful response is good
            Err(e) => {
                warn!("Parakeet sidecar health check failed: {:?}", e);
                Err(e)
            }
        }
    }

    pub async fn shutdown(&self) {
        self.client.shutdown().await;
    }
}
