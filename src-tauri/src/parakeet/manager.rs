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

    pub fn get_model_definition(&self, model_name: &str) -> Option<&'static ParakeetModelDefinition> {
        AVAILABLE_MODELS.iter().find(|m| m.id == model_name)
    }

    pub fn model_dir(&self, model_name: &str) -> PathBuf {
        self.root_dir.join(model_name)
    }

    pub fn is_model_downloaded(&self, definition: &ParakeetModelDefinition) -> bool {
        let model_dir = self.model_dir(definition.id);
        definition
            .files
            .iter()
            .all(|file| model_dir.join(&file.filename).exists())
    }

    pub async fn download_model(
        &self,
        model_name: &str,
        cancel_flag: Option<Arc<AtomicBool>>,
        progress_callback: impl Fn(u64, u64) + Send + 'static,
    ) -> Result<(), String> {
        let Some(definition) = self.get_model_definition(model_name) else {
            return Err(format!("Unknown Parakeet model: {model_name}"));
        };

        let model_dir = self.model_dir(definition.id);
        std::fs::create_dir_all(&model_dir)
            .map_err(|e| format!("Failed to create model directory: {e}"))?;

        let mut total_bytes: u64 = 0;
        let mut sizes = Vec::new();

        for file in definition.files {
            let url = self.file_url(definition, file.filename);
            match self.http.head(&url).send().await {
                Ok(resp) => {
                    let size = resp.content_length().unwrap_or(0);
                    sizes.push(size);
                    total_bytes += size;
                }
                Err(err) => {
                    warn!("HEAD request failed for {}: {}", file.filename, err);
                    sizes.push(0);
                }
            }
        }

        if total_bytes == 0 {
            // Fallback to estimated size if HEAD failed
            total_bytes = definition.estimated_size;
        }

        progress_callback(0, total_bytes);

        let mut downloaded = 0u64;
        for (idx, file) in definition.files.iter().enumerate() {
            let file_size_hint = sizes.get(idx).copied().unwrap_or(0);
            let url = self.file_url(definition, file.filename);
            let destination = model_dir.join(file.filename);
            let temp_path = destination.with_extension(".download");

            let mut response = self
                .http
                .get(&url)
                .send()
                .await
                .map_err(|e| format!("Failed to download {}: {}", file.filename, e))?;

            let mut file = tokio::fs::File::create(&temp_path)
                .await
                .map_err(|e| format!("Failed to create {}: {}", temp_path.display(), e))?;

            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|e| format!("Failed to read response: {}", e))?
            {
                if let Some(flag) = cancel_flag.as_ref() {
                    if flag.load(Ordering::Relaxed) {
                        let _ = tokio::fs::remove_file(&temp_path).await;
                        return Err("Download cancelled by user".to_string());
                    }
                }

                file.write_all(&chunk)
                    .await
                    .map_err(|e| format!("Failed to write chunk: {}", e))?;
                downloaded += chunk.len() as u64;
                progress_callback(downloaded, total_bytes.max(downloaded + file_size_hint));
            }

            file.flush()
                .await
                .map_err(|e| format!("Failed to flush {}: {}", temp_path.display(), e))?;
            drop(file);

            tokio::fs::rename(&temp_path, &destination)
                .await
                .map_err(|e| format!("Failed to finalise file {}: {}", destination.display(), e))?;
        }

        let final_total = if downloaded == 0 { total_bytes } else { downloaded };
        progress_callback(final_total, final_total);

        Ok(())
    }

    pub fn delete_model(&self, model_name: &str) -> Result<(), String> {
        let Some(definition) = self.get_model_definition(model_name) else {
            return Err(format!("Unknown Parakeet model: {model_name}"));
        };

        let model_dir = self.model_dir(definition.id);
        if model_dir.exists() {
            std::fs::remove_dir_all(&model_dir)
                .map_err(|e| format!("Failed to delete Parakeet model directory {}: {}", model_dir.display(), e))?;
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

    pub async fn shutdown(&self) {
        self.client.shutdown().await;
    }
}
