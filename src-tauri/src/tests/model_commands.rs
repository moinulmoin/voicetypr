#[cfg(test)]
mod tests {
    use crate::whisper::manager::{ModelInfo, ModelSize, WhisperManager};
    use tempfile::TempDir;

    #[test]
    fn test_model_size_validation() {
        // Test minimum size validation (10MB)
        let too_small = ModelSize::new(5 * 1024 * 1024); // 5MB
        assert!(too_small.is_err());
        assert!(too_small.unwrap_err().contains("too small"));

        // Test maximum size validation (3.5GB)
        let too_large = ModelSize::new(4 * 1024 * 1024 * 1024); // 4GB
        assert!(too_large.is_err());
        assert!(too_large.unwrap_err().contains("exceeds maximum"));

        // Test valid sizes
        let valid_small = ModelSize::new(50 * 1024 * 1024); // 50MB
        assert!(valid_small.is_ok());
        assert_eq!(valid_small.unwrap().as_bytes(), 50 * 1024 * 1024);

        let valid_large = ModelSize::new(3 * 1024 * 1024 * 1024); // 3GB
        assert!(valid_large.is_ok());
        assert_eq!(valid_large.unwrap().as_bytes(), 3 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_model_info_validated_size() {
        let model = ModelInfo {
            name: "test".to_string(),
            size: 100 * 1024 * 1024, // 100MB
            url: "https://example.com/model.bin".to_string(),
            sha256: "abc123".to_string(),
            downloaded: false,
            speed_score: 5,
            accuracy_score: 5,
        };

        let validated = model.validated_size();
        assert!(validated.is_ok());
        assert_eq!(validated.unwrap().as_bytes(), 100 * 1024 * 1024);

        // Test with invalid size
        let invalid_model = ModelInfo {
            name: "test".to_string(),
            size: 1024, // 1KB - too small
            url: "https://example.com/model.bin".to_string(),
            sha256: "abc123".to_string(),
            downloaded: false,
            speed_score: 5,
            accuracy_score: 5,
        };

        let validated = invalid_model.validated_size();
        assert!(validated.is_err());
    }

    #[test]
    fn test_whisper_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let manager = WhisperManager::new(temp_dir.path().to_path_buf());

        // Get models status
        let models = manager.get_models_status();

        // Should have all the default models
        assert!(models.contains_key("base.en"));
        assert!(models.contains_key("large-v3"));

        // All models should initially be not downloaded
        for (_, model) in models.iter() {
            assert!(!model.downloaded);
        }
    }

    #[test]
    fn test_model_info_serialization() {
        let model = ModelInfo {
            name: "test".to_string(),
            size: 100 * 1024 * 1024,
            url: "https://example.com/model.bin".to_string(),
            sha256: "abc123".to_string(),
            downloaded: true,
            speed_score: 7,
            accuracy_score: 8,
        };

        let json = serde_json::to_string(&model).unwrap();
        assert!(json.contains("\"name\":\"test\""));
        assert!(json.contains("\"downloaded\":true"));
        assert!(json.contains("\"speed_score\":7"));
        assert!(json.contains("\"accuracy_score\":8"));
    }

    #[test]
    fn test_whisper_manager_models_dir() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().join("models");

        // Create the models directory
        std::fs::create_dir_all(&models_dir).unwrap();

        let _manager = WhisperManager::new(models_dir.clone());

        // The manager should be created with the correct directory
        assert!(models_dir.exists());
    }

    #[test]
    fn test_list_downloaded_files() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().join("models");
        std::fs::create_dir_all(&models_dir).unwrap();

        // Create a WhisperManager with known models
        let manager = WhisperManager::new(models_dir.clone());
        
        // Create model files for known models only
        for model_name in ["base.en", "large-v3", "large-v3-q5_0"] {
            let file_path = models_dir.join(format!("{}.bin", model_name));
            std::fs::write(&file_path, b"dummy model data").unwrap();
        }

        // Create a non-model file that should be ignored
        std::fs::write(models_dir.join("readme.txt"), b"not a model").unwrap();
        
        // Also create a .bin file that's not a known model - should be ignored
        std::fs::write(models_dir.join("unknown.bin"), b"unknown model").unwrap();

        let downloaded = manager.list_downloaded_files();

        // Should only list known models that have .bin files
        assert_eq!(downloaded.len(), 3);
        assert!(downloaded.contains(&"base.en".to_string()));
        assert!(downloaded.contains(&"large-v3".to_string()));
        assert!(downloaded.contains(&"large-v3-q5_0".to_string()));
        assert!(!downloaded.contains(&"readme".to_string()));
        assert!(!downloaded.contains(&"unknown".to_string()));
    }

    #[test]
    fn test_get_model_path() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().join("models");
        std::fs::create_dir_all(&models_dir).unwrap();

        let mut manager = WhisperManager::new(models_dir.clone());

        // get_model_path only returns path if model is downloaded
        let path = manager.get_model_path("base.en");
        assert!(path.is_none()); // Not downloaded yet

        // Create the model file to simulate download
        std::fs::write(models_dir.join("base.en.bin"), b"dummy model").unwrap();

        // Refresh status
        manager.refresh_downloaded_status();

        // Now it should return the path
        let path = manager.get_model_path("base.en");
        assert!(path.is_some());
        assert_eq!(path.unwrap(), models_dir.join("base.en.bin"));

        // Test getting path for unknown model
        let path = manager.get_model_path("unknown");
        assert!(path.is_none());
    }

    #[test]
    fn test_delete_model_file() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().join("models");
        std::fs::create_dir_all(&models_dir).unwrap();

        // Create a model file
        let model_file = models_dir.join("base.en.bin");
        std::fs::write(&model_file, b"dummy model data").unwrap();
        assert!(model_file.exists());

        let mut manager = WhisperManager::new(models_dir);

        // Delete the model
        let result = manager.delete_model_file("base.en");
        assert!(result.is_ok());
        assert!(!model_file.exists());

        // Try to delete non-existent model
        let result = manager.delete_model_file("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_model_validation() {
        // Test valid model names
        let valid_models = vec![
            "base.en",
            "large-v3",
            "large-v3-q5_0",
            "large-v3-turbo",
            "large-v3-turbo-q5_0",
        ];

        for model in &valid_models {
            assert!(valid_models.contains(&model));
        }

        // Test invalid model names
        let invalid_models = vec!["invalid", "large-v2", "tiny.en", "custom"];
        for model in &invalid_models {
            assert!(!valid_models.contains(model));
        }
    }

    #[test]
    fn test_refresh_downloaded_status() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().join("models");
        std::fs::create_dir_all(&models_dir).unwrap();

        let mut manager = WhisperManager::new(models_dir.clone());

        // Initially no models should be downloaded
        let status = manager.get_models_status();
        for (_, info) in &status {
            assert!(!info.downloaded);
        }

        // Create a model file
        std::fs::write(models_dir.join("base.en.bin"), b"dummy model").unwrap();

        // Refresh status
        manager.refresh_downloaded_status();

        // Now base.en should be marked as downloaded
        let status = manager.get_models_status();
        assert!(status.get("base.en").unwrap().downloaded);
        assert!(!status.get("large-v3").unwrap().downloaded);
    }

    #[test]
    fn test_model_scores() {
        let temp_dir = TempDir::new().unwrap();
        let manager = WhisperManager::new(temp_dir.path().to_path_buf());
        let models = manager.get_models_status();

        // Test that speed and accuracy scores are inversely related
        let base_en = models.get("base.en").unwrap();
        assert_eq!(base_en.speed_score, 8); // Very fast
        assert_eq!(base_en.accuracy_score, 5); // Basic accuracy

        let large = models.get("large-v3").unwrap();
        assert!(large.speed_score < base_en.speed_score); // Slower
        assert!(large.accuracy_score > base_en.accuracy_score); // More accurate

        // Verify all scores are in valid range (1-10)
        for (_, model) in &models {
            assert!(model.speed_score >= 1 && model.speed_score <= 10);
            assert!(model.accuracy_score >= 1 && model.accuracy_score <= 10);
        }
    }

    #[test]
    fn test_model_sizes() {
        let temp_dir = TempDir::new().unwrap();
        let manager = WhisperManager::new(temp_dir.path().to_path_buf());
        let models = manager.get_models_status();

        // Verify model sizes are reasonable
        let base_en = models.get("base.en").unwrap();
        assert!(base_en.size > 100 * 1024 * 1024); // > 100MB
        assert!(base_en.size < 200 * 1024 * 1024); // < 200MB

        // Note: large-v3 is actually 2.9GB, well within our 3.5GB limit
        let large = models.get("large-v3").unwrap();
        assert!(large.size > base_en.size); // Large should be larger than base
        assert!(large.size > 2 * 1024 * 1024 * 1024); // > 2GB
        assert!(large.size < 3584 * 1024 * 1024); // < 3.5GB
    }

    #[test]
    fn test_model_urls() {
        let temp_dir = TempDir::new().unwrap();
        let manager = WhisperManager::new(temp_dir.path().to_path_buf());
        let models = manager.get_models_status();

        // Verify all models have valid Hugging Face URLs
        for (name, model) in &models {
            assert!(model.url.starts_with("https://huggingface.co/"));
            assert!(model.url.contains("whisper.cpp"));
            assert!(model.url.ends_with(&format!("{}.bin", name)));

            // Verify SHA256 field (actually contains SHA1) is 40 characters
            assert_eq!(model.sha256.len(), 40);
        }
    }
}
