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
        };
        assert!(request.validate().is_err());
        
        // Whitespace only
        let request = AIEnhancementRequest {
            text: "   \n\t  ".to_string(),
            context: None,
        };
        assert!(request.validate().is_err());
        
        // Valid text
        let request = AIEnhancementRequest {
            text: "Hello, world!".to_string(),
            context: None,
        };
        assert!(request.validate().is_ok());
        
        // Text at max length
        let request = AIEnhancementRequest {
            text: "a".repeat(MAX_TEXT_LENGTH),
            context: None,
        };
        assert!(request.validate().is_ok());
        
        // Text exceeding max length
        let request = AIEnhancementRequest {
            text: "a".repeat(MAX_TEXT_LENGTH + 1),
            context: None,
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
    fn test_groq_provider_prompt_generation() {
        let prompt = groq::GroqProvider::build_enhancement_prompt(
            "hello world",
            Some("Casual conversation")
        );
        
        assert!(prompt.contains("hello world"));
        assert!(prompt.contains("Casual conversation"));
        assert!(prompt.contains("transcription enhancement assistant"));
    }
    
    #[test]
    fn test_gemini_provider_prompt_generation() {
        let prompt = gemini::GeminiProvider::build_enhancement_prompt(
            "hello world",
            Some("Casual conversation")
        );
        
        assert!(prompt.contains("hello world"));
        assert!(prompt.contains("Casual conversation"));
        assert!(prompt.contains("transcription enhancement assistant"));
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