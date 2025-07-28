use crate::ai::{AIProviderConfig, AIProviderFactory, AIEnhancementRequest};
use serde::{Deserialize, Serialize};
use tauri_plugin_store::StoreExt;
use regex::Regex;
use std::sync::Mutex;
use std::collections::HashMap;
use once_cell::sync::Lazy;

// In-memory cache for API keys to avoid system password prompts
// Keys are stored in Stronghold by frontend and cached here for backend use
static API_KEY_CACHE: Lazy<Mutex<HashMap<String, String>>> = Lazy::new(|| {
    Mutex::new(HashMap::new())
});

#[derive(Debug, Serialize, Deserialize)]
pub struct AISettings {
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    #[serde(rename = "hasApiKey")]
    pub has_api_key: bool,
}

// Validation patterns
lazy_static::lazy_static! {
    static ref PROVIDER_REGEX: Regex = Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap();
    static ref MODEL_REGEX: Regex = Regex::new(r"^[a-zA-Z0-9_.-]+$").unwrap();
}

fn validate_provider_name(provider: &str) -> Result<(), String> {
    if !PROVIDER_REGEX.is_match(provider) {
        return Err("Invalid provider name format".to_string());
    }
    Ok(())
}

fn validate_model_name(model: &str) -> Result<(), String> {
    if !MODEL_REGEX.is_match(model) {
        return Err("Invalid model name format".to_string());
    }
    Ok(())
}

#[tauri::command]
pub async fn get_ai_settings(app: tauri::AppHandle) -> Result<AISettings, String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    
    let enabled = store.get("ai_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
        
    let provider = store.get("ai_provider")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "groq".to_string());
        
    let model = store.get("ai_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "".to_string()); // Empty by default
    
    // Check if API key exists in cache
    // Frontend will check Stronghold and update this via cache_ai_api_key
    let has_api_key = {
        let cache = API_KEY_CACHE.lock().map_err(|_| "Failed to access cache".to_string())?;
        cache.contains_key(&format!("ai_api_key_{}", provider))
    };
    
    Ok(AISettings {
        enabled,
        provider,
        model,
        has_api_key,
    })
}

#[tauri::command]
pub async fn get_ai_settings_for_provider(
    provider: String,
    app: tauri::AppHandle
) -> Result<AISettings, String> {
    validate_provider_name(&provider)?;
    
    let store = app.store("settings").map_err(|e| e.to_string())?;
    
    let enabled = store.get("ai_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
        
    let model = store.get("ai_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "".to_string()); // Empty by default
    
    // Check if API key exists in cache
    // Frontend will check Stronghold and update this via cache_ai_api_key
    let has_api_key = {
        let cache = API_KEY_CACHE.lock().map_err(|_| "Failed to access cache".to_string())?;
        cache.contains_key(&format!("ai_api_key_{}", provider))
    };
    
    Ok(AISettings {
        enabled,
        provider,
        model,
        has_api_key,
    })
}

// Frontend is responsible for saving API keys to Stronghold
// This command caches the key for backend use
#[tauri::command]
pub async fn cache_ai_api_key(
    provider: String,
    api_key: String,
) -> Result<(), String> {
    validate_provider_name(&provider)?;
    
    if api_key.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }
    
    // Store API key in memory cache
    let mut cache = API_KEY_CACHE.lock().map_err(|_| "Failed to access cache".to_string())?;
    cache.insert(format!("ai_api_key_{}", provider), api_key);
    
    log::info!("API key cached for provider: {}", provider);
    Ok(())
}

// Frontend is responsible for removing API keys from Stronghold
// This command clears the cache
#[tauri::command]
pub async fn clear_ai_api_key_cache(provider: String) -> Result<(), String> {
    validate_provider_name(&provider)?;
    
    let mut cache = API_KEY_CACHE.lock().map_err(|_| "Failed to access cache".to_string())?;
    cache.remove(&format!("ai_api_key_{}", provider));
    
    log::info!("API key cache cleared for provider: {}", provider);
    Ok(())
}

#[tauri::command]
pub async fn update_ai_settings(
    enabled: bool,
    provider: String,
    model: String,
    app: tauri::AppHandle
) -> Result<(), String> {
    validate_provider_name(&provider)?;
    
    // Allow empty model (for deselection) but validate if not empty
    if !model.is_empty() {
        validate_model_name(&model)?;
    }
    
    // Don't allow enabling without a model selected
    if enabled && model.is_empty() {
        return Err("Please select a model before enabling AI enhancement".to_string());
    }
    
    let store = app.store("settings").map_err(|e| e.to_string())?;
    
    store.set("ai_enabled", serde_json::Value::Bool(enabled));
    store.set("ai_provider", serde_json::Value::String(provider.clone()));
    store.set("ai_model", serde_json::Value::String(model.clone()));
    
    store.save()
        .map_err(|e| format!("Failed to save AI settings: {}", e))?;
    
    log::info!("AI settings updated: enabled={}, provider={}, model={}", enabled, provider, model);
    Ok(())
}

#[tauri::command]
pub async fn disable_ai_enhancement(app: tauri::AppHandle) -> Result<(), String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    
    store.set("ai_enabled", serde_json::Value::Bool(false));
    
    store.save()
        .map_err(|e| format!("Failed to save AI settings: {}", e))?;
    
    log::info!("AI enhancement disabled");
    Ok(())
}

#[tauri::command]
pub async fn enhance_transcription(
    text: String,
    app: tauri::AppHandle
) -> Result<String, String> {
    // Quick validation
    if text.trim().is_empty() {
        log::debug!("Skipping enhancement for empty text");
        return Ok(text);
    }
    
    let store = app.store("settings").map_err(|e| e.to_string())?;
    
    let enabled = store.get("ai_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
        
    if !enabled {
        log::debug!("AI enhancement is disabled");
        return Ok(text); // Return original text if AI is not enabled
    }
    
    let provider = store.get("ai_provider")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "groq".to_string());
        
    let model = store.get("ai_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "".to_string()); // Empty by default
    
    // Don't enhance if no model selected
    if model.is_empty() {
        log::debug!("No AI model selected, skipping enhancement");
        return Ok(text);
    }
    
    // Get API key from cache
    let api_key = {
        let cache = API_KEY_CACHE.lock().map_err(|_| "Failed to access cache".to_string())?;
        cache.get(&format!("ai_api_key_{}", provider))
            .ok_or_else(|| "API key not found in cache".to_string())?
            .clone()
    };
    
    drop(store); // Release lock before async operation
    
    log::info!("Enhancing text with {} model {} (length: {})", provider, model, text.len());
    
    // Create provider config
    let config = AIProviderConfig {
        provider,
        model,
        api_key,
        enabled: true,
        options: Default::default(),
    };
    
    // Create provider and enhance text
    let provider = AIProviderFactory::create(&config)
        .map_err(|e| format!("Failed to create AI provider: {}", e))?;
        
    let request = AIEnhancementRequest {
        text: text.clone(),
        context: None,
    };
    
    match provider.enhance_text(request).await {
        Ok(response) => {
            log::info!("Text enhanced successfully (original: {}, enhanced: {})", 
                text.len(), response.enhanced_text.len());
            Ok(response.enhanced_text)
        },
        Err(e) => {
            log::error!("AI enhancement failed: {}", e);
            Err(format!("AI enhancement failed: {}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_provider_validation() {
        assert!(validate_provider_name("groq").is_ok());
        assert!(validate_provider_name("openai").is_ok());
        assert!(validate_provider_name("test-provider").is_ok());
        assert!(validate_provider_name("test_provider").is_ok());
        
        assert!(validate_provider_name("test provider").is_err());
        assert!(validate_provider_name("test@provider").is_err());
        assert!(validate_provider_name("").is_err());
    }
    
    #[test]
    fn test_model_validation() {
        assert!(validate_model_name("llama-3.1-8b-instant").is_ok());
        assert!(validate_model_name("gpt-4").is_ok());
        assert!(validate_model_name("model.v1").is_ok());
        
        assert!(validate_model_name("model with spaces").is_err());
        assert!(validate_model_name("model@v1").is_err());
        assert!(validate_model_name("").is_err());
    }
}