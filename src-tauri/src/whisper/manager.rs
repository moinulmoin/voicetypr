use futures_util::StreamExt;
use reqwest;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

// Type-safe size validation
#[derive(Debug, Clone, Copy)]
pub struct ModelSize(u64);

impl ModelSize {
    const MAX_MODEL_SIZE: u64 = 2 * 1024 * 1024 * 1024; // 2GB max as per your requirement
    const MIN_MODEL_SIZE: u64 = 10 * 1024 * 1024; // 10MB min (reasonable for smallest model)

    pub fn new(size: u64) -> Result<Self, String> {
        if size < Self::MIN_MODEL_SIZE {
            return Err(format!(
                "Model size {} bytes ({:.1}MB) is too small. Minimum size is 10MB",
                size,
                size as f64 / 1024.0 / 1024.0
            ));
        }
        if size > Self::MAX_MODEL_SIZE {
            return Err(format!(
                "Model size {} bytes ({:.1}GB) exceeds maximum allowed size of 2GB",
                size,
                size as f64 / 1024.0 / 1024.0 / 1024.0
            ));
        }
        Ok(ModelSize(size))
    }

    pub fn as_bytes(&self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub size: u64,
    pub url: String,
    pub sha256: String,
    pub downloaded: bool,
    pub speed_score: u8,    // 1-10, 10 being fastest
    pub accuracy_score: u8, // 1-10, 10 being most accurate
}

impl ModelInfo {
    /// Validate and get size as ModelSize
    pub fn validated_size(&self) -> Result<ModelSize, String> {
        ModelSize::new(self.size)
    }
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

        // OFFICIAL SHA1 CHECKSUMS from whisper.cpp repository
        // Source: https://github.com/ggml-org/whisper.cpp/blob/master/models/download-ggml-model.sh
        // These are SHA1 hashes (40 characters) as used by the official download script
        // Note: The field is named 'sha256' for historical reasons but contains SHA1 values

        // Multilingual models only (no .en variants)
        // Removed tiny, small, and medium models - keeping only base and large variants

        models.insert(
            "base.en".to_string(),
            ModelInfo {
                name: "base.en".to_string(),
                size: 148_897_792, // 142 MiB = 142 * 1024 * 1024 bytes
                url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin"
                    .to_string(),
                sha256: "137c40403d78fd54d454da0f9bd998f78703390c".to_string(), // SHA1 (correct)
                downloaded: false,
                speed_score: 8,    // Very fast
                accuracy_score: 5, // Basic accuracy
            },
        );

        models.insert(
            "large-v3".to_string(),
            ModelInfo {
                name: "large-v3".to_string(),
                size: 3_117_854_720, // 2.9 GiB = 2.9 * 1024 * 1024 * 1024 bytes
                url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin"
                    .to_string(),
                sha256: "ad82bf6a9043ceed055076d0fd39f5f186ff8062".to_string(), // SHA1 (correct)
                downloaded: false,
                speed_score: 2,     // Slowest
                accuracy_score: 9, // Best accuracy
            },
        );

        models.insert("large-v3-q5_0".to_string(), ModelInfo {
            name: "large-v3-q5_0".to_string(),
            size: 1_181_116_416, // 1.1 GiB = 1.1 * 1024 * 1024 * 1024 bytes
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-q5_0.bin".to_string(),
            sha256: "e6e2ed78495d403bef4b7cff42ef4aaadcfea8de".to_string(), // SHA1 (correct)
            downloaded: false,
            speed_score: 4,       // Quantized, faster than full large
            accuracy_score: 8,    // Slight degradation from quantization
        });

        models.insert("large-v3-turbo".to_string(), ModelInfo {
            name: "large-v3-turbo".to_string(),
            size: 1_610_612_736, // 1.5 GiB = 1.5 * 1024 * 1024 * 1024 bytes
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin".to_string(),
            sha256: "4af2b29d7ec73d781377bfd1758ca957a807e941".to_string(), // SHA1 (correct)
            downloaded: false,
            speed_score: 7,       // 6x faster than large-v3
            accuracy_score: 9,    // Comparable to large-v2
        });

        models.insert("large-v3-turbo-q5_0".to_string(), ModelInfo {
            name: "large-v3-turbo-q5_0".to_string(),
            size: 573_571_072, // 547 MiB = 547 * 1024 * 1024 bytes
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin".to_string(),
            sha256: "e050f7970618a659205450ad97eb95a18d69c9ee".to_string(), // SHA1 (correct)
            downloaded: false,
            speed_score: 8,       // Very fast, quantized turbo
            accuracy_score: 7,    // Good accuracy with turbo + quantization
        });

        let mut manager = Self { models_dir, models };
        manager.check_downloaded_models();
        manager
    }

    fn check_downloaded_models(&mut self) {
        log::debug!("Checking models directory: {:?}", self.models_dir);
        if let Ok(entries) = std::fs::read_dir(&self.models_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    log::trace!("Found file: {}", name);
                    if name.ends_with(".bin") {
                        let model_name = name.trim_end_matches(".bin");
                        log::trace!("Model name from file: {}", model_name);
                        if let Some(model) = self.models.get_mut(model_name) {
                            #[cfg(debug_assertions)]
                            log::debug!("Marking model {} as downloaded", model_name);
                            model.downloaded = true;
                        } else {
                            // File present but not in the predefined list—quietly ignore
                            #[cfg(debug_assertions)]
                            log::info!("Extra model file detected: {}.bin", model_name);
                        }
                    }
                }
            }
        } else {
            log::warn!("Failed to read models directory or directory doesn't exist");
        }

        // Log final status
        for (name, info) in &self.models {
            log::debug!("Model {}: downloaded = {}", name, info.downloaded);
        }
    }

    pub async fn download_model(
        &self,
        model_name: &str,
        cancel_flag: Option<Arc<AtomicBool>>,
        progress_callback: impl Fn(u64, u64),
    ) -> Result<(), String> {
        log::info!("WhisperManager: Downloading model {}", model_name);

        let model = self.models.get(model_name).ok_or(format!(
            "Model '{}' not found in available models",
            model_name
        ))?;

        // Validate model size before downloading
        let validated_size = model.validated_size()?;

        log::debug!("Model URL: {}", model.url);
        log::debug!(
            "Model size: {} bytes (validated)",
            validated_size.as_bytes()
        );

        // Create models directory if it doesn't exist
        fs::create_dir_all(&self.models_dir)
            .await
            .map_err(|e| format!("Failed to create models directory: {}", e))?;

        let output_path = self.models_dir.join(format!("{}.bin", model_name));
        log::debug!("Output path: {:?}", output_path);

        // Download the model
        let client = reqwest::Client::new();
        let response = client
            .get(&model.url)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let total_size = response.content_length().unwrap_or(model.size);

        // Validate reported size matches expected size (allow 10% variance for compression)
        let size_variance = (total_size as f64 - model.size as f64).abs() / model.size as f64;
        if size_variance > 0.1 {
            return Err(format!(
                "Model size mismatch: expected {} bytes, server reports {} bytes ({}% difference)",
                model.size,
                total_size,
                (size_variance * 100.0) as u32
            ));
        }

        // Validate the total size is within our limits
        let _ = ModelSize::new(total_size)?;

        let mut file = fs::File::create(&output_path)
            .await
            .map_err(|e| e.to_string())?;

        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();
        let mut last_progress_update = 0u64;
        let update_threshold = total_size / 100; // Update every 1%

        while let Some(chunk) = stream.next().await {
            // Check for cancellation
            if let Some(ref flag) = cancel_flag {
                if flag.load(Ordering::Relaxed) {
                    log::info!("Download cancelled by user for model: {}", model_name);
                    // Clean up partial download
                    drop(file);
                    let _ = fs::remove_file(&output_path).await;
                    return Err("Download cancelled by user".to_string());
                }
            }

            let chunk = chunk.map_err(|e| e.to_string())?;

            // Prevent downloading more than expected (with 1% tolerance)
            if downloaded + chunk.len() as u64 > (total_size as f64 * 1.01) as u64 {
                // Clean up partial download
                drop(file);
                let _ = fs::remove_file(&output_path).await;

                return Err(format!(
                    "Download exceeded expected size: downloaded {} bytes, expected {} bytes",
                    downloaded + chunk.len() as u64,
                    total_size
                ));
            }

            file.write_all(&chunk).await.map_err(|e| e.to_string())?;

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

        // Verify checksum if available
        if !model.sha256.is_empty() {
            log::info!("Verifying model checksum...");
            match model.sha256.len() {
                40 => {
                    // SHA1 checksum (legacy from whisper.cpp)
                    self.verify_sha1_checksum(&output_path, &model.sha256)
                        .await?;
                }
                64 => {
                    // SHA256 checksum (preferred)
                    self.verify_sha256_checksum(&output_path, &model.sha256)
                        .await?;
                }
                _ => {
                    log::warn!(
                        "Invalid checksum length for {}. Skipping verification.",
                        model_name
                    );
                    log::warn!(
                        "Expected SHA1 (40 chars) or SHA256 (64 chars), got {} chars.",
                        model.sha256.len()
                    );
                }
            }
        } else {
            log::warn!(
                "No checksum available for {}. Skipping verification.",
                model_name
            );
            log::warn!("File integrity cannot be guaranteed without checksum verification.");
        }

        Ok(())
    }

    /// Verify the SHA256 checksum of a downloaded file
    async fn verify_sha256_checksum(
        &self,
        file_path: &PathBuf,
        expected_checksum: &str,
    ) -> Result<(), String> {
        // Open the file
        let mut file = fs::File::open(file_path)
            .await
            .map_err(|e| format!("Failed to open file for checksum verification: {}", e))?;

        // Read file in chunks and calculate hash
        let mut hasher = Sha256::new();
        let mut buffer = vec![0; 8192]; // 8KB buffer

        loop {
            let bytes_read = file
                .read(&mut buffer)
                .await
                .map_err(|e| format!("Failed to read file for checksum: {}", e))?;

            if bytes_read == 0 {
                break;
            }

            hasher.update(&buffer[..bytes_read]);
        }

        // Get the final hash
        let result = hasher.finalize();
        let calculated_checksum = format!("{:x}", result);

        // Compare checksums
        if calculated_checksum != expected_checksum {
            // Delete the corrupted file
            let _ = fs::remove_file(file_path).await;
            return Err(format!(
                "Checksum verification failed!\nExpected: {}\nCalculated: {}\nFile has been deleted.",
                expected_checksum,
                calculated_checksum
            ));
        }

        log::info!("SHA256 checksum verified successfully!");
        Ok(())
    }

    /// Verify the SHA1 checksum of a downloaded file (legacy support for whisper.cpp models)
    async fn verify_sha1_checksum(
        &self,
        file_path: &PathBuf,
        expected_checksum: &str,
    ) -> Result<(), String> {
        // Open the file
        let mut file = fs::File::open(file_path)
            .await
            .map_err(|e| format!("Failed to open file for checksum verification: {}", e))?;

        // Read file in chunks and calculate hash
        let mut hasher = Sha1::new();
        let mut buffer = vec![0; 8192]; // 8KB buffer

        loop {
            let bytes_read = file
                .read(&mut buffer)
                .await
                .map_err(|e| format!("Failed to read file for checksum: {}", e))?;

            if bytes_read == 0 {
                break;
            }

            hasher.update(&buffer[..bytes_read]);
        }

        // Get the final hash
        let result = hasher.finalize();
        let calculated_checksum = format!("{:x}", result);

        // Compare checksums
        if calculated_checksum != expected_checksum {
            // Delete the corrupted file
            let _ = fs::remove_file(file_path).await;
            return Err(format!(
                "SHA1 checksum verification failed!\nExpected: {}\nCalculated: {}\nFile has been deleted.",
                expected_checksum,
                calculated_checksum
            ));
        }

        log::info!("SHA1 checksum verified successfully!");
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

    /// Calculate a balanced performance score (combines speed and accuracy)
    #[allow(dead_code)]
    pub fn calculate_balanced_score(speed: u8, accuracy: u8) -> f32 {
        // Weighted average: 40% speed, 60% accuracy
        // Most users want accuracy but also care about speed
        (speed as f32 * 0.4 + accuracy as f32 * 0.6) / 10.0 * 100.0
    }

    /// Get models sorted by a specific metric
    #[allow(dead_code)]
    pub fn get_models_sorted(&self, sort_by: &str) -> Vec<(String, ModelInfo)> {
        let mut models: Vec<(String, ModelInfo)> = self
            .models
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        match sort_by {
            "speed" => {
                models.sort_by(|a, b| b.1.speed_score.cmp(&a.1.speed_score));
            }
            "accuracy" => {
                models.sort_by(|a, b| b.1.accuracy_score.cmp(&a.1.accuracy_score));
            }
            "balanced" => {
                models.sort_by(|a, b| {
                    let score_a =
                        Self::calculate_balanced_score(a.1.speed_score, a.1.accuracy_score);
                    let score_b =
                        Self::calculate_balanced_score(b.1.speed_score, b.1.accuracy_score);
                    score_b
                        .partial_cmp(&score_a)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            "size" => {
                models.sort_by(|a, b| a.1.size.cmp(&b.1.size));
            }
            _ => {
                // Default: sort by balanced score
                models.sort_by(|a, b| {
                    let score_a =
                        Self::calculate_balanced_score(a.1.speed_score, a.1.accuracy_score);
                    let score_b =
                        Self::calculate_balanced_score(b.1.speed_score, b.1.accuracy_score);
                    score_b
                        .partial_cmp(&score_a)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }

        models
    }

}
