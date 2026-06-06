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
            display_name: "Test Model".to_string(),
            size: 100 * 1024 * 1024, // 100MB
            url: "https://example.com/model.bin".to_string(),
            sha256: "abc123".to_string(),
            downloaded: false,
            speed_score: 5,
            accuracy_score: 5,
            recommended: false,
        };

        let validated = model.validated_size();
        assert!(validated.is_ok());
        assert_eq!(validated.unwrap().as_bytes(), 100 * 1024 * 1024);

        // Test with invalid size
        let invalid_model = ModelInfo {
            name: "test".to_string(),
            display_name: "Test Model".to_string(),
            size: 1024, // 1KB - too small
            url: "https://example.com/model.bin".to_string(),
            sha256: "abc123".to_string(),
            downloaded: false,
            speed_score: 5,
            accuracy_score: 5,
            recommended: false,
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
        assert!(models.contains_key("small.en"));
        assert!(models.contains_key("medium"));
        assert!(models.contains_key("large-v3"));
        assert!(models.contains_key("large-v3-q5_0"));
        assert!(models.contains_key("large-v3-turbo"));

        // All models should initially be not downloaded
        for (_, model) in models.iter() {
            assert!(!model.downloaded);
        }
    }

    #[test]
    fn test_model_info_serialization() {
        let model = ModelInfo {
            name: "test".to_string(),
            display_name: "Test Model".to_string(),
            size: 100 * 1024 * 1024,
            url: "https://example.com/model.bin".to_string(),
            sha256: "abc123".to_string(),
            downloaded: true,
            speed_score: 7,
            accuracy_score: 8,
            recommended: false,
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

        let manager = WhisperManager::new(models_dir.clone());

        assert!(models_dir.exists());
        assert_eq!(manager.models_dir(), models_dir);
    }

    #[test]
    fn test_get_model_info_for_download_prep() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().join("models");
        std::fs::create_dir_all(&models_dir).unwrap();

        let manager = WhisperManager::new(models_dir.clone());
        let (model_info, output_path) = manager.get_model_info("base.en").unwrap();

        assert_eq!(model_info.name, "base.en");
        assert_eq!(output_path, models_dir.join("base.en.bin"));
        assert_eq!(manager.models_dir(), models_dir);
    }

    #[test]
    fn test_active_download_guard_rejects_duplicate_operations() {
        use crate::commands::model::register_active_download;
        use std::collections::HashMap;
        use std::sync::atomic::AtomicBool;
        use std::sync::{Arc, Mutex as StdMutex};

        let active_downloads = Arc::new(StdMutex::new(HashMap::<String, Arc<AtomicBool>>::new()));
        let first_flag = Arc::new(AtomicBool::new(false));
        let second_flag = Arc::new(AtomicBool::new(false));

        register_active_download(&active_downloads, "base.en", first_flag.clone()).unwrap();

        let err = register_active_download(&active_downloads, "base.en", second_flag.clone())
            .unwrap_err();
        assert!(err.contains("already in progress"));

        let downloads = active_downloads.lock().unwrap();
        assert!(std::ptr::eq(
            downloads.get("base.en").unwrap().as_ref(),
            first_flag.as_ref()
        ));
        assert!(!std::ptr::eq(
            downloads.get("base.en").unwrap().as_ref(),
            second_flag.as_ref()
        ));
    }

    #[test]
    fn test_list_downloaded_files_only_returns_exact_catalog_matches() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().join("models");
        std::fs::create_dir_all(&models_dir).unwrap();

        let mut manager = WhisperManager::new_for_test(models_dir.clone());

        std::fs::write(models_dir.join("base.en.bin"), vec![0u8; 1024]).unwrap();
        std::fs::write(models_dir.join("large-v3.bin"), vec![0u8; 2048]).unwrap();
        std::fs::write(models_dir.join("large-v3-q5_0.bin"), b"wrong size").unwrap();
        std::fs::write(models_dir.join("unknown.bin"), b"unknown model").unwrap();

        manager.refresh_downloaded_status();
        let downloaded = manager.list_downloaded_files();

        assert_eq!(downloaded.len(), 2);
        assert!(downloaded.contains(&"base.en".to_string()));
        assert!(downloaded.contains(&"large-v3".to_string()));
        assert!(!downloaded.contains(&"large-v3-q5_0".to_string()));
        assert!(!downloaded.contains(&"unknown".to_string()));
    }

    #[test]
    fn test_get_model_path() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().join("models");
        std::fs::create_dir_all(&models_dir).unwrap();

        let mut manager = WhisperManager::new_for_test(models_dir.clone());

        // get_model_path only returns path if model is downloaded
        let path = manager.get_model_path("base.en");
        assert!(path.is_none()); // Not downloaded yet

        // Create the model file to simulate download
        // For test models, create exactly 1KB file
        let dummy_data = vec![0u8; 1024];
        std::fs::write(models_dir.join("base.en.bin"), dummy_data).unwrap();

        // Refresh status
        manager.refresh_downloaded_status();

        // Debug: Check the status after refresh
        let status = manager.get_models_status();
        println!("Model status after refresh: {:?}", status.get("base.en"));

        // Now it should return the path
        let path = manager.get_model_path("base.en");
        println!("Path returned: {:?}", path);
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
        let temp_dir = TempDir::new().unwrap();
        let manager = WhisperManager::new(temp_dir.path().to_path_buf());
        let models = manager.get_models_status();

        for name in [
            "base.en",
            "small.en",
            "medium",
            "large-v3",
            "large-v3-q5_0",
            "large-v3-turbo",
        ] {
            assert!(models.contains_key(name), "missing catalog model: {name}");
        }

        for name in ["invalid", "large-v2", "tiny.en", "custom"] {
            assert!(
                !models.contains_key(name),
                "unexpected catalog model: {name}"
            );
        }
    }

    #[test]
    fn test_refresh_downloaded_status() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().join("models");
        std::fs::create_dir_all(&models_dir).unwrap();

        let mut manager = WhisperManager::new_for_test(models_dir.clone());

        // Initially no models should be downloaded
        let status = manager.get_models_status();
        for (_, info) in &status {
            assert!(!info.downloaded);
        }

        // Create a model file
        // For test models, create exactly 1KB file
        let dummy_data = vec![0u8; 1024];
        std::fs::write(models_dir.join("base.en.bin"), dummy_data).unwrap();

        // Refresh status
        manager.refresh_downloaded_status();

        // Now base.en should be marked as downloaded
        let status = manager.get_models_status();
        assert!(status.get("base.en").unwrap().downloaded);
        assert!(!status.get("large-v3").unwrap().downloaded);

        // Wrong-size files must not count as downloaded
        std::fs::write(models_dir.join("large-v3.bin"), vec![0u8; 512]).unwrap();
        manager.refresh_downloaded_status();
        let status = manager.get_models_status();
        assert!(!status.get("large-v3").unwrap().downloaded);

        // Exact-size files must count as downloaded
        std::fs::write(models_dir.join("large-v3.bin"), vec![0u8; 2048]).unwrap();
        manager.refresh_downloaded_status();
        let status = manager.get_models_status();
        assert!(status.get("large-v3").unwrap().downloaded);
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

        let medium = models.get("medium").unwrap();
        assert_eq!(medium.speed_score, 5);
        assert_eq!(medium.accuracy_score, 7);
        assert!(!medium.recommended);

        let large = models.get("large-v3").unwrap();
        assert!(large.speed_score < base_en.speed_score); // Slower
        assert!(large.accuracy_score > base_en.accuracy_score); // More accurate

        let q5 = models.get("large-v3-q5_0").unwrap();
        let turbo = models.get("large-v3-turbo").unwrap();
        assert!(q5.speed_score > large.speed_score);
        assert!(q5.speed_score < turbo.speed_score);
        assert!(q5.accuracy_score < large.accuracy_score);
        assert!(q5.accuracy_score >= turbo.accuracy_score - 1);
        assert!(q5.recommended);

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

        let base_en = models.get("base.en").unwrap();
        assert_eq!(base_en.size, 147_964_211);

        let small_en = models.get("small.en").unwrap();
        assert_eq!(small_en.size, 487_614_201);
        assert!(small_en.size > base_en.size);

        let medium = models.get("medium").unwrap();
        assert_eq!(medium.size, 1_533_763_059);
        assert!(medium.size > small_en.size);

        let large = models.get("large-v3").unwrap();
        assert_eq!(large.size, 3_095_033_483);

        let q5 = models.get("large-v3-q5_0").unwrap();
        assert_eq!(q5.size, 1_081_140_203);
        assert!(q5.size > small_en.size);
        assert!(q5.size < large.size);

        let turbo = models.get("large-v3-turbo").unwrap();
        assert_eq!(turbo.size, 1_624_555_275);
        assert!(turbo.size > q5.size);
        assert!(turbo.size < large.size);
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
            assert!(model.url.ends_with(&format!("ggml-{name}.bin")));

            // Verify SHA256 field (actually contains SHA1) is 40 characters
            assert_eq!(model.sha256.len(), 40);
        }

        let base_en = models.get("base.en").unwrap();
        assert_eq!(base_en.display_name, "Base (English)");
        assert_eq!(base_en.sha256, "137c40403d78fd54d454da0f9bd998f78703390c");

        let small_en = models.get("small.en").unwrap();
        assert_eq!(small_en.display_name, "Small (English)");
        assert_eq!(small_en.sha256, "db8a495a91d927739e50b3fc1cc4c6b8f6c2d022");

        let medium = models.get("medium").unwrap();
        assert_eq!(medium.display_name, "Medium");
        assert_eq!(medium.sha256, "fd9727b6e1217c2f614f9b698455c4ffd82463b4");
        assert!(medium.url.ends_with("ggml-medium.bin"));

        let q5 = models.get("large-v3-q5_0").unwrap();
        assert_eq!(q5.display_name, "Large v3 (Q5)");
        assert_eq!(q5.sha256, "e6e2ed78495d403bef4b7cff42ef4aaadcfea8de");
    }

    #[test]
    fn test_production_catalog_sparse_file_size_regression() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().join("models");
        std::fs::create_dir_all(&models_dir).unwrap();

        let model_path = models_dir.join("base.en.bin");
        {
            let file = std::fs::File::create(&model_path).unwrap();
            file.set_len(147_964_211).unwrap();
        }

        let mut manager = WhisperManager::new(models_dir.clone());
        let status = manager.get_models_status();
        assert!(status.get("base.en").unwrap().downloaded);

        let path = manager.get_model_path("base.en");
        assert!(path.is_some());
        assert_eq!(path.unwrap(), model_path);

        {
            let file = std::fs::OpenOptions::new()
                .write(true)
                .open(&model_path)
                .unwrap();
            file.set_len(148_897_792).unwrap();
        }

        manager.refresh_downloaded_status();
        let status = manager.get_models_status();
        assert!(!status.get("base.en").unwrap().downloaded);
        assert!(manager.get_model_path("base.en").is_none());
    }
}
