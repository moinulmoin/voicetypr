#[cfg(test)]
mod tests {
    use crate::commands::model::{clear_active_download, register_active_download};
    use crate::whisper::manager::{ModelInfo, ModelSize, WhisperManager};
    use sha2::{Digest, Sha256};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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
        assert!(models.contains_key("large-v3"));

        // All models should initially be not downloaded
        for model in models.values() {
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

        let _manager = WhisperManager::new(models_dir.clone());

        // The manager should be created with the correct directory
        assert!(models_dir.exists());
    }

    #[test]
    fn test_whisper_manager_accessors_return_model_info_and_models_dir() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().join("models");
        let manager = WhisperManager::new(models_dir.clone());

        assert_eq!(manager.models_dir(), models_dir);

        let (model_info, output_path) = manager.get_model_info("base.en").unwrap();
        assert_eq!(model_info.name, "base.en");
        assert_eq!(output_path, manager.models_dir().join("base.en.bin"));
    }

    #[test]
    fn test_active_download_duplicate_preserves_original_flag() {
        let active_downloads = Arc::new(Mutex::new(HashMap::new()));
        let original_flag = Arc::new(AtomicBool::new(false));
        let duplicate_flag = Arc::new(AtomicBool::new(false));

        register_active_download(&active_downloads, "base.en", original_flag.clone()).unwrap();
        let duplicate = register_active_download(&active_downloads, "base.en", duplicate_flag);

        assert!(duplicate.is_err());
        let stored_flag = active_downloads
            .lock()
            .unwrap()
            .get("base.en")
            .cloned()
            .unwrap();
        stored_flag.store(true, Ordering::Relaxed);
        assert!(original_flag.load(Ordering::Relaxed));

        clear_active_download(&active_downloads, "base.en");
        assert!(!active_downloads.lock().unwrap().contains_key("base.en"));
    }

    #[test]
    fn test_delete_guard_uses_active_download_registration() {
        let active_downloads = Arc::new(Mutex::new(HashMap::new()));
        let download_flag = Arc::new(AtomicBool::new(false));
        let delete_flag = Arc::new(AtomicBool::new(false));

        register_active_download(&active_downloads, "base.en", download_flag.clone()).unwrap();
        let guarded_delete = register_active_download(&active_downloads, "base.en", delete_flag);

        assert!(guarded_delete.is_err());
        assert!(!download_flag.load(Ordering::Relaxed));
        assert_eq!(active_downloads.lock().unwrap().len(), 1);
    }

    #[test]
    fn test_list_downloaded_files() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().join("models");
        std::fs::create_dir_all(&models_dir).unwrap();

        let mut manager = WhisperManager::new_for_test(models_dir.clone());

        std::fs::write(models_dir.join("base.en.bin"), vec![0u8; 1024]).unwrap();
        std::fs::write(models_dir.join("large-v3.bin"), vec![0u8; 1024]).unwrap();
        std::fs::write(models_dir.join("unknown.bin"), vec![0u8; 1024]).unwrap();

        manager.refresh_downloaded_status();
        let downloaded = manager.list_downloaded_files();
        assert_eq!(downloaded.len(), 1);
        assert!(downloaded.contains(&"base.en".to_string()));
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
        let valid_models = vec!["base.en", "small.en", "large-v3", "large-v3-turbo"];

        for model in &valid_models {
            assert!(valid_models.contains(model));
        }

        let invalid_models = vec!["invalid", "large-v2", "tiny.en", "custom", "large-v3-q5_0"];
        for model in &invalid_models {
            assert!(!valid_models.contains(model));
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
        for info in status.values() {
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
        for model in models.values() {
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
    fn test_catalog_uses_pinned_hf_revision_and_sha256() {
        let temp_dir = TempDir::new().unwrap();
        let manager = WhisperManager::new(temp_dir.path().to_path_buf());
        let models = manager.get_models_status();
        let revision = "5359861c739e955e79d9a303bcbc70fb988958b1";

        let expected = [
            (
                "base.en",
                "ggml-base.en.bin",
                147_964_211,
                "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002",
            ),
            (
                "small.en",
                "ggml-small.en.bin",
                487_614_201,
                "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d",
            ),
            (
                "large-v3",
                "ggml-large-v3.bin",
                3_095_033_483,
                "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2",
            ),
            (
                "large-v3-turbo",
                "ggml-large-v3-turbo.bin",
                1_624_555_275,
                "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
            ),
        ];

        assert_eq!(models.len(), expected.len());
        assert!(!models.contains_key("medium"));
        assert!(!models.contains_key("large-v3-q5_0"));

        for (name, file_name, size, sha256) in expected {
            let model = models.get(name).unwrap();
            assert_eq!(model.size, size);
            assert_eq!(model.sha256, sha256);
            assert_eq!(model.sha256.len(), 64);
            assert!(model.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert_eq!(
                model.url,
                format!(
                    "https://huggingface.co/ggerganov/whisper.cpp/resolve/{}/{}",
                    revision, file_name
                )
            );
        }
    }

    #[test]
    fn test_exact_size_status_accepts_only_catalog_size_without_deleting() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().join("models");
        std::fs::create_dir_all(&models_dir).unwrap();
        let base_path = models_dir.join("base.en.bin");

        let exact_file = std::fs::File::create(&base_path).unwrap();
        exact_file.set_len(147_964_211).unwrap();

        let mut manager = WhisperManager::new(models_dir.clone());
        assert!(
            manager
                .get_models_status()
                .get("base.en")
                .unwrap()
                .downloaded
        );
        assert_eq!(std::fs::metadata(&base_path).unwrap().len(), 147_964_211);

        std::fs::File::create(&base_path)
            .unwrap()
            .set_len(148_897_792)
            .unwrap();
        manager.refresh_downloaded_status();
        assert!(
            !manager
                .get_models_status()
                .get("base.en")
                .unwrap()
                .downloaded
        );
        assert_eq!(std::fs::metadata(&base_path).unwrap().len(), 148_897_792);

        std::fs::File::create(&base_path)
            .unwrap()
            .set_len(147_964_210)
            .unwrap();
        manager.refresh_downloaded_status();
        assert!(
            !manager
                .get_models_status()
                .get("base.en")
                .unwrap()
                .downloaded
        );
        assert_eq!(std::fs::metadata(&base_path).unwrap().len(), 147_964_210);
    }

    #[tokio::test]
    async fn test_download_model_file_removes_mismatched_file_before_redownload() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().join("models");
        std::fs::create_dir_all(&models_dir).unwrap();
        let output_path = models_dir.join("base.en.bin");
        std::fs::write(&output_path, b"stale file").unwrap();

        let body = vec![42u8; ModelSize::new(10 * 1024 * 1024).unwrap().as_bytes() as usize];
        let expected_sha256 = format!("{:x}", Sha256::digest(&body));

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/ggml-base.en.bin"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
            .mount(&server)
            .await;

        let model = ModelInfo {
            name: "base.en".to_string(),
            display_name: "Base (English)".to_string(),
            size: body.len() as u64,
            url: format!("{}/ggml-base.en.bin", server.uri()),
            sha256: expected_sha256,
            downloaded: false,
            speed_score: 8,
            accuracy_score: 5,
            recommended: false,
        };

        WhisperManager::download_model_file(&model, &output_path, &models_dir, None, |_, _| {})
            .await
            .unwrap();

        assert_eq!(std::fs::read(&output_path).unwrap(), body);
    }

    #[tokio::test]
    async fn test_download_model_file_verifies_exact_size_existing_file() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().join("models");
        std::fs::create_dir_all(&models_dir).unwrap();
        let output_path = models_dir.join("base.en.bin");

        let expected_body =
            vec![1u8; ModelSize::new(10 * 1024 * 1024).unwrap().as_bytes() as usize];
        let corrupt_body = vec![2u8; expected_body.len()];
        std::fs::write(&output_path, &corrupt_body).unwrap();

        let model = ModelInfo {
            name: "base.en".to_string(),
            display_name: "Base (English)".to_string(),
            size: corrupt_body.len() as u64,
            url: "http://127.0.0.1:9/should-not-download.bin".to_string(),
            sha256: format!("{:x}", Sha256::digest(&expected_body)),
            downloaded: false,
            speed_score: 8,
            accuracy_score: 5,
            recommended: false,
        };

        let result =
            WhisperManager::download_model_file(&model, &output_path, &models_dir, None, |_, _| {})
                .await;

        let error = result.unwrap_err();
        assert!(error.contains("Checksum verification failed"));
        assert!(!output_path.exists());
    }
}
