#[cfg(test)]
mod tests {
    use super::super::*;
    use std::collections::HashMap;

    #[test]
    fn test_ai_error_display() {
        let err = AIError::ApiError("Test error".to_string());
        assert_eq!(err.to_string(), "API error: Test error");

        let err = AIError::ValidationError("Invalid input".to_string());
        assert_eq!(err.to_string(), "Validation error: Invalid input");

        let err = AIError::RateLimitExceeded;
        assert_eq!(err.to_string(), "Rate limit exceeded");
    }

    #[test]
    fn test_ai_enhancement_request_validation() {
        // Empty text
        let request = AIEnhancementRequest {
            text: "".to_string(),
            context: None,
            options: None,
        };
        assert!(request.validate().is_err());

        // Whitespace only
        let request = AIEnhancementRequest {
            text: "   \n\t  ".to_string(),
            context: None,
            options: None,
        };
        assert!(request.validate().is_err());

        // Valid text
        let request = AIEnhancementRequest {
            text: "Hello, world!".to_string(),
            context: None,
            options: None,
        };
        assert!(request.validate().is_ok());

        // Text at max length
        let request = AIEnhancementRequest {
            text: "a".repeat(MAX_TEXT_LENGTH),
            context: None,
            options: None,
        };
        assert!(request.validate().is_ok());

        // Text exceeding max length
        let request = AIEnhancementRequest {
            text: "a".repeat(MAX_TEXT_LENGTH + 1),
            context: None,
            options: None,
        };
        assert!(request.validate().is_err());
    }

    #[test]
    fn test_ai_provider_config_serialization() {
        let config = AIProviderConfig {
            provider: "groq".to_string(),
            model: "llama-3.3-70b-versatile".to_string(),
            api_key: "secret_key".to_string(),
            enabled: true,
            options: HashMap::new(),
        };

        // API key should not be serialized
        let serialized = serde_json::to_string(&config).unwrap();
        assert!(!serialized.contains("secret_key"));
        assert!(serialized.contains("groq"));
        assert!(serialized.contains("llama-3.3-70b-versatile"));
    }

    #[test]
    fn test_ai_provider_factory_validation() {
        let config = AIProviderConfig {
            provider: "invalid_provider".to_string(),
            model: "test".to_string(),
            api_key: "test".to_string(),
            enabled: true,
            options: HashMap::new(),
        };

        let result = AIProviderFactory::create(&config);
        assert!(result.is_err());

        if let Err(err) = result {
            match err {
                AIError::ProviderNotFound(provider) => {
                    assert_eq!(provider, "invalid_provider");
                }
                _ => panic!("Expected ProviderNotFound error"),
            }
        }
    }

    #[test]
    fn test_enhancement_prompt_generation() {
        use crate::ai::prompts::{build_enhancement_prompt, EnhancementOptions};

        // Test with default options
        let options = EnhancementOptions::default();
        let prompt = build_enhancement_prompt("hello world", None, &options);

        assert!(prompt.contains("hello world"));
        assert!(prompt.contains("APPLY THESE CORRECTIONS"));

        // Test with context
        let prompt_with_context =
            build_enhancement_prompt("hello world", Some("Casual conversation"), &options);

        assert!(prompt_with_context.contains("Context: Casual conversation"));

        // Test with custom vocabulary
        let mut options_with_vocab = EnhancementOptions::default();
        options_with_vocab.custom_vocabulary = vec!["TypeScript".to_string(), "React".to_string()];
        let prompt_with_vocab = build_enhancement_prompt("hello world", None, &options_with_vocab);

        assert!(prompt_with_vocab.contains("Recognize these terms: TypeScript, React"));
    }

    #[test]
    fn test_enhancement_presets() {
        use crate::ai::prompts::{build_enhancement_prompt, EnhancementOptions, EnhancementPreset};

        let text = "um hello world";

        // Test Default preset
        let default_options = EnhancementOptions::default();
        let default_prompt = build_enhancement_prompt(text, None, &default_options);
        assert!(default_prompt.contains("APPLY THESE CORRECTIONS"));

        // Test Prompts preset
        let mut prompts_options = EnhancementOptions::default();
        prompts_options.preset = EnhancementPreset::Prompts;
        let prompts_prompt = build_enhancement_prompt(text, None, &prompts_options);
        assert!(
            prompts_prompt.contains("transform the cleaned text into a well-structured AI prompt")
        );

        // Test Email preset
        let mut email_options = EnhancementOptions::default();
        email_options.preset = EnhancementPreset::Email;
        let email_prompt = build_enhancement_prompt(text, None, &email_options);
        assert!(
            email_prompt.contains("format the cleaned text as an email")
        );

        // Test Commit preset
        let mut commit_options = EnhancementOptions::default();
        commit_options.preset = EnhancementPreset::Commit;
        let commit_prompt = build_enhancement_prompt(text, None, &commit_options);
        assert!(commit_prompt.contains("convert to conventional commit format"));
    }

    #[test]
    fn test_self_correction_rules_in_all_presets() {
        use crate::ai::prompts::{build_enhancement_prompt, EnhancementOptions, EnhancementPreset};

        let test_text = "send it to john... to mary";

        // Test that ALL presets include self-correction rules
        let presets = vec![
            EnhancementPreset::Default,
            EnhancementPreset::Prompts,
            EnhancementPreset::Email,
            EnhancementPreset::Commit,
        ];

        for preset in presets {
            let mut options = EnhancementOptions::default();
            options.preset = preset.clone();
            let prompt = build_enhancement_prompt(test_text, None, &options);
            
            // All prompts should include self-correction rules
            assert!(
                prompt.contains("handle natural speech self-corrections"),
                "Preset {:?} should include self-correction rules",
                preset
            );
            assert!(
                prompt.contains("Immediate replacement"),
                "Preset {:?} should include immediate replacement pattern",
                preset
            );
        }
    }

    #[test]
    fn test_layered_architecture() {
        use crate::ai::prompts::{build_enhancement_prompt, EnhancementOptions, EnhancementPreset};

        let test_text = "test";
        
        // Test that all presets include base processing
        let presets = vec![
            EnhancementPreset::Default,
            EnhancementPreset::Prompts,
            EnhancementPreset::Email,
            EnhancementPreset::Commit,
        ];

        for preset in presets {
            let mut options = EnhancementOptions::default();
            options.preset = preset.clone();
            let prompt = build_enhancement_prompt(test_text, None, &options);
            
            // All should include self-correction rules
            assert!(
                prompt.contains("handle natural speech self-corrections"),
                "Preset {:?} should include self-correction rules",
                preset
            );
            
            // All should include default processing
            assert!(
                prompt.contains("APPLY THESE CORRECTIONS"),
                "Preset {:?} should include default processing",
                preset
            );
            
            // Non-default presets should have FINALLY instruction
            if !matches!(preset, EnhancementPreset::Default) {
                assert!(
                    prompt.contains("FINALLY"),
                    "Preset {:?} should have FINALLY transformation",
                    preset
                );
            }
        }
    }

    #[test]
    fn test_default_prompt_comprehensive_features() {
        use crate::ai::prompts::{build_enhancement_prompt, EnhancementOptions};

        let test_text = "test transcription";
        let options = EnhancementOptions::default();
        let prompt = build_enhancement_prompt(test_text, None, &options);

        // Test that Default prompt includes all comprehensive features
        
        // 1. Speech artifacts removal
        assert!(prompt.contains("Filler words"), "Should include filler word removal");
        assert!(prompt.contains("stutters"), "Should handle stutters");
        
        // 2. Homophone correction
        assert!(prompt.contains("Homophones"), "Should handle homophones");
        assert!(prompt.contains("there/their/they're"), "Should include common homophone examples");
        
        // 3. Number and time formatting
        assert!(prompt.contains("Times:"), "Should format times");
        assert!(prompt.contains("Dates:"), "Should format dates");
        assert!(prompt.contains("Numbers:"), "Should format numbers");
        
        // 4. Spoken punctuation
        assert!(prompt.contains("spoken punctuation"), "Should handle spoken punctuation");
        assert!(prompt.contains("comma"), "Should handle spoken comma");
        assert!(prompt.contains("question mark"), "Should handle spoken question mark");
        
        // 5. List detection
        assert!(prompt.contains("Format lists"), "Should format detected lists");
        
        // 6. Technical terms
        assert!(prompt.contains("Technical terms"), "Should preserve technical terms");
        assert!(prompt.contains("JavaScript"), "Should correct JavaScript");
    }

    #[test]
    fn test_ai_model_serialization() {
        let model = AIModel {
            id: "test-model".to_string(),
            name: "Test Model".to_string(),
            description: Some("A test model".to_string()),
        };

        let json = serde_json::to_string(&model).unwrap();
        assert!(json.contains("test-model"));
        assert!(json.contains("Test Model"));
        assert!(json.contains("A test model"));
    }
}
