use crate::ai::{AIProviderConfig, AIProviderFactory, AIEnhancementRequest, EnhancementOptions};
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

/// Validate an API key by making a minimal test request
async fn validate_api_key(provider: &str, api_key: &str) -> Result<(), String> {
    match provider {
        "groq" => {
            // Make a minimal API call to validate the key
            let client = reqwest::Client::new();
            let response = client
                .post("https://api.groq.com/openai/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", api_key))
                .json(&serde_json::json!({
                    "model": "llama-3.1-8b-instant",
                    "messages": [{"role": "user", "content": "1"}],
                    "max_tokens": 1,
                    "temperature": 0
                }))
                .send()
                .await
                .map_err(|e| {
                    if e.is_connect() || e.is_timeout() {
                        "Network error: Unable to connect to Groq API".to_string()
                    } else {
                        "Network error".to_string()
                    }
                })?;
            
            match response.status().as_u16() {
                401 => Err("Invalid API key".to_string()),
                200 => Ok(()),
                429 => Err("Rate limit exceeded. Please try again later.".to_string()),
                500..=599 => Err("Groq API is temporarily unavailable".to_string()),
                _ => Err(format!("Unexpected error: {}", response.status()))
            }
        }
        "gemini" => {
            // Validate Gemini API key - use minimal tokens
            let url = "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash-lite:generateContent";
            let client = reqwest::Client::new();
            let response = client
                .post(url)
                .header("x-goog-api-key", api_key)
                .json(&serde_json::json!({
                    "contents": [{"parts": [{"text": "1"}]}],
                    "generationConfig": {
                        "maxOutputTokens": 1,
                        "temperature": 0
                    }
                }))
                .send()
                .await
                .map_err(|e| {
                    if e.is_connect() || e.is_timeout() {
                        "Network error: Unable to connect to Gemini API".to_string()
                    } else {
                        "Network error".to_string()
                    }
                })?;
                
            match response.status().as_u16() {
                400 | 401 | 403 => Err("Invalid API key".to_string()),
                200 => Ok(()),
                429 => Err("Rate limit exceeded. Please try again later.".to_string()),
                500..=599 => Err("Gemini API is temporarily unavailable".to_string()),
                _ => Err(format!("Unexpected error: {}", response.status()))
            }
        }
        _ => Ok(()) // Unknown providers pass through
    }
}

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
    _app: tauri::AppHandle,
    provider: String,
    api_key: String,
) -> Result<(), String> {
    validate_provider_name(&provider)?;
    
    if api_key.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }
    
    // Validate the API key with a test call
    validate_api_key(&provider, &api_key).await?;
    
    // Store API key in memory cache
    let mut cache = API_KEY_CACHE.lock().map_err(|_| "Failed to access cache".to_string())?;
    cache.insert(format!("ai_api_key_{}", provider), api_key);
    
    log::info!("API key cached for provider: {}", provider);
    
    Ok(())
}

// Frontend is responsible for removing API keys from Stronghold
// This command clears the cache
#[tauri::command]
pub async fn clear_ai_api_key_cache(
    _app: tauri::AppHandle,
    provider: String
) -> Result<(), String> {
    validate_provider_name(&provider)?;
    
    let mut cache = API_KEY_CACHE.lock().map_err(|_| "Failed to access cache".to_string())?;
    cache.remove(&format!("ai_api_key_{}", provider));
    
    log::info!("API key cache cleared for provider: {}", provider);
    
    Ok(())
}

// Clear entire API key cache (for reset)
pub fn clear_all_api_key_cache() -> Result<(), String> {
    let mut cache = API_KEY_CACHE.lock().map_err(|_| "Failed to access cache".to_string())?;
    cache.clear();
    log::info!("Cleared entire API key cache");
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
        log::warn!("Attempted to enable AI enhancement without a model selected");
        return Err("Please select a model before enabling AI enhancement".to_string());
    }
    
    // Check if API key exists when enabling
    if enabled {
        let cache_has_key = {
            let cache = API_KEY_CACHE.lock().map_err(|_| "Failed to access cache".to_string())?;
            cache.contains_key(&format!("ai_api_key_{}", provider))
        };
        
        if !cache_has_key {
            log::warn!("Attempted to enable AI enhancement without cached API key for provider: {}", provider);
            return Err("API key not found. Please add an API key first.".to_string());
        }
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
pub async fn get_enhancement_options(app: tauri::AppHandle) -> Result<EnhancementOptions, String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    
    // Load from store or return defaults
    if let Some(options_value) = store.get("enhancement_options") {
        serde_json::from_value(options_value.clone())
            .map_err(|e| format!("Failed to parse enhancement options: {}", e))
    } else {
        Ok(EnhancementOptions::default())
    }
}

#[tauri::command]
pub async fn update_enhancement_options(
    options: EnhancementOptions,
    app: tauri::AppHandle
) -> Result<(), String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    
    // Validate custom vocabulary
    for term in &options.custom_vocabulary {
        if term.trim().is_empty() {
            return Err("Custom vocabulary terms cannot be empty".to_string());
        }
        if term.len() > crate::ai::MAX_VOCABULARY_TERM_LENGTH {
            return Err(format!("Custom vocabulary terms must be less than {} characters", 
                crate::ai::MAX_VOCABULARY_TERM_LENGTH));
        }
    }
    
    if options.custom_vocabulary.len() > crate::ai::MAX_CUSTOM_VOCABULARY {
        return Err(format!("Maximum {} custom vocabulary terms allowed", 
            crate::ai::MAX_CUSTOM_VOCABULARY));
    }
    
    store.set("enhancement_options", serde_json::to_value(&options)
        .map_err(|e| format!("Failed to serialize options: {}", e))?);
    
    store.save()
        .map_err(|e| format!("Failed to save enhancement options: {}", e))?;
    
    log::info!("Enhancement options updated: preset={:?}, vocab_count={}", 
        options.preset, options.custom_vocabulary.len());
    
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
        log::warn!("AI enhancement enabled but no model selected. Provider: {}", provider);
        return Ok(text);
    }
    
    // Get API key from cache with proper locking
    let api_key = {
        let cache = API_KEY_CACHE.lock().map_err(|_| "Failed to access cache".to_string())?;
        let key_name = format!("ai_api_key_{}", provider);
        cache.get(&key_name)
            .cloned() // Clone to release lock early
            .ok_or_else(|| {
                log::error!("API key not found in cache for provider: {}. Cache keys: {:?}", 
                    provider, cache.keys().collect::<Vec<_>>());
                "API key not found in cache".to_string()
            })?
    };
    
    drop(store); // Release lock before async operation
    
    // Load enhancement options
    let enhancement_options = get_enhancement_options(app.clone()).await.ok();
    
    log::info!("Enhancing text with {} model {} (length: {}, options: {:?})", 
        provider, model, text.len(), enhancement_options);
    
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
        options: enhancement_options,
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