use futures_util::StreamExt;
use reqwest;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use sha2::{Sha256, Digest};
use sha1::Sha1;
use tokio::io::AsyncReadExt;

#[derive(Clone, serde::Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub size: u64,
    pub url: String,
    pub sha256: String,
    pub downloaded: bool,
    pub speed_score: u8,    // 1-10, 10 being fastest
    pub accuracy_score: u8, // 1-10, 10 being most accurate
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
        models.insert(
            "tiny".to_string(),
            ModelInfo {
                name: "tiny".to_string(),
                size: 75_000_000, // 75MB
                url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin"
                    .to_string(),
                sha256: "bd577a113a864445d4c299885e0cb97d4ba92b5f".to_string(), // SHA1 (correct)
                downloaded: false,
                speed_score: 10,   // Fastest
                accuracy_score: 3, // Lowest accuracy
            },
        );

        models.insert(
            "base".to_string(),
            ModelInfo {
                name: "base".to_string(),
                size: 142_000_000, // 142MB
                url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin"
                    .to_string(),
                sha256: "465707469ff3a37a2b9b8d8f89f2f99de7299dac".to_string(), // SHA1 (correct)
                downloaded: false,
                speed_score: 8,    // Very fast
                accuracy_score: 5, // Basic accuracy
            },
        );

        models.insert(
            "small".to_string(),
            ModelInfo {
                name: "small".to_string(),
                size: 466_000_000, // 466MB
                url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin"
                    .to_string(),
                sha256: "55356645c2b361a969dfd0ef2c5a50d530afd8d5".to_string(), // SHA1 (correct)
                downloaded: false,
                speed_score: 6,    // Good speed
                accuracy_score: 7, // Good accuracy, best balance
            },
        );

        models.insert(
            "medium".to_string(),
            ModelInfo {
                name: "medium".to_string(),
                size: 1_500_000_000, // 1.5GB
                url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin"
                    .to_string(),
                sha256: "fd9727b6e1217c2f614f9b698455c4ffd82463b4".to_string(), // SHA1 (correct)
                downloaded: false,
                speed_score: 4,    // Slower
                accuracy_score: 8, // Very good accuracy
            },
        );

        models.insert(
            "large-v3".to_string(),
            ModelInfo {
                name: "large-v3".to_string(),
                size: 2_900_000_000, // 2.9GB
                url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin"
                    .to_string(),
                sha256: "ad82bf6a9043ceed055076d0fd39f5f186ff8062".to_string(), // SHA1 (correct)
                downloaded: false,
                speed_score: 2,     // Slowest
                accuracy_score: 10, // Best accuracy
            },
        );

        models.insert("large-v3-q5_0".to_string(), ModelInfo {
            name: "large-v3-q5_0".to_string(),
            size: 1_100_000_000, // 1.1GB
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-q5_0.bin".to_string(),
            sha256: "e6e2ed78495d403bef4b7cff42ef4aaadcfea8de".to_string(), // SHA1 (correct)
            downloaded: false,
            speed_score: 3,       // Quantized, faster than full large
            accuracy_score: 9,    // Slight degradation from quantization
        });

        models.insert("large-v3-turbo".to_string(), ModelInfo {
            name: "large-v3-turbo".to_string(),
            size: 1_500_000_000, // 1.5GB
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin".to_string(),
            sha256: "4af2b29d7ec73d781377bfd1758ca957a807e941".to_string(), // SHA1 (correct)
            downloaded: false,
            speed_score: 7,       // 6x faster than large-v3
            accuracy_score: 9,    // Comparable to large-v2
        });

        models.insert("large-v3-turbo-q5_0".to_string(), ModelInfo {
            name: "large-v3-turbo-q5_0".to_string(),
            size: 547_000_000, // 547MB
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin".to_string(),
            sha256: "e050f7970618a659205450ad97eb95a18d69c9ee".to_string(), // SHA1 (correct)
            downloaded: false,
            speed_score: 8,       // Very fast, quantized turbo
            accuracy_score: 8,    // Good accuracy with turbo + quantization
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
        progress_callback: impl Fn(u64, u64),
    ) -> Result<(), String> {
        println!("WhisperManager: Downloading model {}", model_name);

        let model = self.models.get(model_name).ok_or(format!(
            "Model '{}' not found in available models",
            model_name
        ))?;

        println!("Model URL: {}", model.url);
        println!("Model size: {} bytes", model.size);

        // Create models directory if it doesn't exist
        fs::create_dir_all(&self.models_dir)
            .await
            .map_err(|e| format!("Failed to create models directory: {}", e))?;

        let output_path = self.models_dir.join(format!("{}.bin", model_name));
        println!("Output path: {:?}", output_path);

        // Download the model
        let client = reqwest::Client::new();
        let response = client
            .get(&model.url)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let total_size = response.content_length().unwrap_or(model.size);

        let mut file = fs::File::create(&output_path)
            .await
            .map_err(|e| e.to_string())?;

        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();
        let mut last_progress_update = 0u64;
        let update_threshold = total_size / 100; // Update every 1%

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| e.to_string())?;
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
            println!("Verifying model checksum...");
            match model.sha256.len() {
                40 => {
                    // SHA1 checksum (legacy from whisper.cpp)
                    self.verify_sha1_checksum(&output_path, &model.sha256).await?;
                }
                64 => {
                    // SHA256 checksum (preferred)
                    self.verify_sha256_checksum(&output_path, &model.sha256).await?;
                }
                _ => {
                    println!("WARNING: Invalid checksum length for {}. Skipping verification.", model_name);
                    println!("         Expected SHA1 (40 chars) or SHA256 (64 chars), got {} chars.", model.sha256.len());
                }
            }
        } else {
            println!("WARNING: No checksum available for {}. Skipping verification.", model_name);
            println!("         File integrity cannot be guaranteed without checksum verification.");
        }

        Ok(())
    }

    /// Verify the SHA256 checksum of a downloaded file
    async fn verify_sha256_checksum(&self, file_path: &PathBuf, expected_checksum: &str) -> Result<(), String> {
        // Open the file
        let mut file = fs::File::open(file_path)
            .await
            .map_err(|e| format!("Failed to open file for checksum verification: {}", e))?;

        // Read file in chunks and calculate hash
        let mut hasher = Sha256::new();
        let mut buffer = vec![0; 8192]; // 8KB buffer
        
        loop {
            let bytes_read = file.read(&mut buffer)
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

        println!("SHA256 checksum verified successfully!");
        Ok(())
    }

    /// Verify the SHA1 checksum of a downloaded file (legacy support for whisper.cpp models)
    async fn verify_sha1_checksum(&self, file_path: &PathBuf, expected_checksum: &str) -> Result<(), String> {
        // Open the file
        let mut file = fs::File::open(file_path)
            .await
            .map_err(|e| format!("Failed to open file for checksum verification: {}", e))?;

        // Read file in chunks and calculate hash
        let mut hasher = Sha1::new();
        let mut buffer = vec![0; 8192]; // 8KB buffer
        
        loop {
            let bytes_read = file.read(&mut buffer)
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

        println!("SHA1 checksum verified successfully!");
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
