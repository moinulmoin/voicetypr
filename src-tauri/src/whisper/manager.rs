use std::path::PathBuf;
use std::collections::HashMap;
use reqwest;
use futures_util::StreamExt;
use tokio::fs;
use tokio::io::AsyncWriteExt;
// use sha2::{Sha256, Digest};  // TODO: Uncomment when implementing checksum verification
// use tokio::io::AsyncReadExt;  // TODO: Uncomment when implementing checksum verification

#[derive(Clone, serde::Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub size: u64,
    pub url: String,
    pub sha256: String,
    pub downloaded: bool,
}

pub struct WhisperManager {
    models_dir: PathBuf,
    models: HashMap<String, ModelInfo>,
}

impl WhisperManager {
    pub fn new(models_dir: PathBuf) -> Self {
        let mut models = HashMap::new();

        // Define available models based on official whisper.cpp download script
        // URLs match https://github.com/ggml-org/whisper.cpp/blob/master/models/download-ggml-model.sh
        models.insert("large-v3-turbo-q5_0".to_string(), ModelInfo {
            name: "large-v3-turbo-q5_0".to_string(),
            size: 547_000_000, // ~547MB
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin".to_string(),
            sha256: "".to_string(), // SHA verification disabled per user request
            downloaded: false,
        });

        let mut manager = Self { models_dir, models };
        manager.check_downloaded_models();
        manager
    }

    fn check_downloaded_models(&mut self) {
        println!("Checking models directory: {:?}", self.models_dir);
        if let Ok(entries) = std::fs::read_dir(&self.models_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    println!("Found file: {}", name);
                    if name.ends_with(".bin") {
                        let model_name = name.trim_end_matches(".bin");
                        println!("Model name from file: {}", model_name);
                        if let Some(model) = self.models.get_mut(model_name) {
                            #[cfg(debug_assertions)]
                            println!("Marking model {} as downloaded", model_name);
                            model.downloaded = true;
                        } else {
                            // File present but not in the predefined list—quietly ignore
                            #[cfg(debug_assertions)]
                            println!("[info] extra model file detected: {}.bin", model_name);
                        }
                    }
                }
            }
        } else {
            println!("Failed to read models directory or directory doesn't exist");
        }

        // Log final status
        for (name, info) in &self.models {
            println!("Model {}: downloaded = {}", name, info.downloaded);
        }
    }

    pub async fn download_model(
        &self,
        model_name: &str,
        progress_callback: impl Fn(u64, u64)
    ) -> Result<(), String> {
        println!("WhisperManager: Downloading model {}", model_name);

        let model = self.models.get(model_name)
            .ok_or(format!("Model '{}' not found in available models", model_name))?;

        println!("Model URL: {}", model.url);
        println!("Model size: {} bytes", model.size);

        // Create models directory if it doesn't exist
        fs::create_dir_all(&self.models_dir).await
            .map_err(|e| format!("Failed to create models directory: {}", e))?;

        let output_path = self.models_dir.join(format!("{}.bin", model_name));
        println!("Output path: {:?}", output_path);

        // Download the model
        let client = reqwest::Client::new();
        let response = client.get(&model.url)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let total_size = response.content_length()
            .unwrap_or(model.size);

        let mut file = fs::File::create(&output_path).await
            .map_err(|e| e.to_string())?;

        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();
        let mut last_progress_update = 0u64;
        let update_threshold = total_size / 100; // Update every 1%

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| e.to_string())?;
            file.write_all(&chunk).await
                .map_err(|e| e.to_string())?;

            downloaded += chunk.len() as u64;

            // Only update progress every 1% to avoid flooding the UI
            if downloaded - last_progress_update >= update_threshold || downloaded == total_size {
                progress_callback(downloaded, total_size);
                last_progress_update = downloaded;
            }
        }

        // Ensure file is flushed to disk
        file.flush().await.map_err(|e| e.to_string())?;
        drop(file);

        // Ensure final 100% progress is sent
        if downloaded < total_size {
            progress_callback(total_size, total_size);
        }

        // TODO: Implement checksum verification when official SHA256 checksums are available
        // For now, we trust the Hugging Face CDN and HTTPS connection
        // self.verify_checksum(&output_path, &model.sha256).await?;

        Ok(())
    }

    pub fn get_model_path(&self, model_name: &str) -> Option<PathBuf> {
        if self.models.get(model_name)?.downloaded {
            Some(self.models_dir.join(format!("{}.bin", model_name)))
        } else {
            None
        }
    }

    pub fn get_models_status(&self) -> HashMap<String, ModelInfo> {
        self.models.clone()
    }

    pub fn refresh_downloaded_status(&mut self) {
        // Reset all to not downloaded
        for model in self.models.values_mut() {
            model.downloaded = false;
        }
        // Check again
        self.check_downloaded_models();
    }

    /// Returns the names of every `.bin` file currently present in the models directory
    /// – even ones the manager doesn't actively track.
    pub fn list_downloaded_files(&self) -> Vec<String> {
        let mut names = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.models_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.ends_with(".bin") {
                        names.push(name.trim_end_matches(".bin").to_string());
                    }
                }
            }
        }
        names
    }

    /// Delete a model file from disk and refresh the downloaded status map.
    pub fn delete_model_file(&mut self, model_name: &str) -> Result<(), String> {
        let path = self.models_dir.join(format!("{}.bin", model_name));
        if !path.exists() {
            return Err("Model file not found".to_string());
        }
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;

        // update internal flags
        if let Some(info) = self.models.get_mut(model_name) {
            info.downloaded = false;
        }
        // Also refresh to catch any other changes
        self.refresh_downloaded_status();
        Ok(())
    }
}