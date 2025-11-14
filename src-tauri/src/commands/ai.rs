use crate::ai::{AIEnhancementRequest, AIProviderConfig, AIProviderFactory, EnhancementOptions};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri_plugin_store::StoreExt;

// In-memory cache for API keys to avoid system password prompts
// Keys are stored in Stronghold by frontend and cached here for backend use
static API_KEY_CACHE: Lazy<Mutex<HashMap<String, String>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// Helper: determine if we should consider that the app "has an API key" for a provider
// For OpenAI-compatible providers, a configured no_auth=true also counts as "has key"
fn check_has_api_key<R: tauri::Runtime>(
    provider: &str,
    store: &tauri_plugin_store::Store<R>,
    cache: &HashMap<String, String>,
) -> bool {
    if provider == "openai" {
        let configured_base = store.get("ai_openai_base_url").is_some();
        configured_base || cache.contains_key(&format!("ai_api_key_{}", provider))
    } else {
        cache.contains_key(&format!("ai_api_key_{}", provider))
    }
}

// Normalize base URL to a Chat Completions endpoint. Base should include version (e.g., .../v1).
fn normalize_chat_completions_url(base: &str) -> String {
    let b = base.trim_end_matches('/');
    format!("{}/chat/completions", b)
}

// removed unused validate_api_key helper

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

// Supported AI providers
const ALLOWED_PROVIDERS: &[&str] = &["groq", "gemini", "openai"];

fn validate_provider_name(provider: &str) -> Result<(), String> {
    // First check format
    if !PROVIDER_REGEX.is_match(provider) {
        return Err("Invalid provider name format".to_string());
    }

    // Then check against allowlist
    if !ALLOWED_PROVIDERS.contains(&provider) {
        return Err(format!(
            "Unsupported provider: {}. Supported providers: {:?}",
            provider, ALLOWED_PROVIDERS
        ));
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

    let enabled = store
        .get("ai_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let provider = store
        .get("ai_provider")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "groq".to_string());

    let model = store
        .get("ai_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "".to_string()); // Empty by default

    // For OpenAI-compatible providers, treat no_auth as having a usable config
    let has_api_key = {
        let cache = API_KEY_CACHE
            .lock()
            .map_err(|_| "Failed to access cache".to_string())?;
        check_has_api_key(&provider, &store, &cache)
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
    app: tauri::AppHandle,
) -> Result<AISettings, String> {
    validate_provider_name(&provider)?;

    let store = app.store("settings").map_err(|e| e.to_string())?;

    let enabled = store
        .get("ai_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let model = store
        .get("ai_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "".to_string()); // Empty by default

    // For OpenAI-compatible providers, treat no_auth as having a usable config
    let has_api_key = {
        let cache = API_KEY_CACHE
            .lock()
            .map_err(|_| "Failed to access cache".to_string())?;
        check_has_api_key(&provider, &store, &cache)
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
#[derive(Deserialize)]
pub struct CacheApiKeyArgs {
    pub provider: String,
    #[serde(alias = "apiKey", alias = "api_key")]
    pub api_key: String,
}

#[tauri::command]
pub async fn cache_ai_api_key(_app: tauri::AppHandle, args: CacheApiKeyArgs) -> Result<(), String> {
    let CacheApiKeyArgs { provider, api_key } = args;
    validate_provider_name(&provider)?;

    if api_key.trim().is_empty() {
        log::warn!(
            "Attempted to cache empty API key for provider: {}",
            provider
        );
        return Err("API key cannot be empty".to_string());
    }

    // Don't validate here - this is called on startup when user might be offline
    // Validation happens when saving new keys in a separate command

    // Store API key in memory cache
    let mut cache = API_KEY_CACHE.lock().map_err(|e| {
        log::error!("Failed to acquire API key cache lock: {}", e);
        "Failed to access cache".to_string()
    })?;

    let key_name = format!("ai_api_key_{}", provider);
    cache.insert(key_name.clone(), api_key.clone());

    log::info!(
        "API key cached for provider: {} (key: {}...)",
        provider,
        if api_key.len() > 8 {
            &api_key[..8]
        } else {
            &api_key
        }
    );

    // Verify the key was actually stored
    if cache.contains_key(&key_name) {
        log::debug!("Verified API key is in cache for provider: {}", provider);
    } else {
        log::error!(
            "Failed to store API key in cache for provider: {}",
            provider
        );
        return Err("Failed to store API key in cache".to_string());
    }

    Ok(())
}

// Validate and save a new API key
#[derive(Deserialize)]
pub struct ValidateAndCacheApiKeyArgs {
    pub provider: String,
    #[serde(alias = "apiKey", alias = "api_key")]
    pub api_key: Option<String>,
    #[serde(alias = "baseUrl", alias = "base_url")]
    pub base_url: Option<String>,
    pub model: Option<String>,
    #[serde(alias = "noAuth", alias = "no_auth")]
    pub no_auth: Option<bool>,
}

#[tauri::command]
pub async fn validate_and_cache_api_key(
    app: tauri::AppHandle,
    args: ValidateAndCacheApiKeyArgs,
) -> Result<(), String> {
    let ValidateAndCacheApiKeyArgs {
        provider,
        api_key,
        base_url,
        model,
        no_auth,
    } = args;
    validate_provider_name(&provider)?;

    let provided_key = api_key.clone().unwrap_or_default();
    let inferred_no_auth = no_auth.unwrap_or(false) || provided_key.trim().is_empty();

    if provider == "openai" {
        let store = app.store("settings").map_err(|e| e.to_string())?;
        if let Some(url) = base_url.clone() {
            store.set("ai_openai_base_url", serde_json::Value::String(url));
        }
        store.set(
            "ai_openai_no_auth",
            serde_json::Value::Bool(inferred_no_auth),
        );
        if let Some(m) = model.clone() {
            store.set("ai_model", serde_json::Value::String(m));
        }
        store
            .save()
            .map_err(|e| format!("Failed to save AI settings: {}", e))?;
    }

    if provider == "openai" {
        let base = base_url
            .clone()
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        let validate_url = normalize_chat_completions_url(&base);

        let client = reqwest::Client::new();
        let mut req = client
            .post(&validate_url)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "model": model.clone().unwrap_or_else(|| "gpt-5-nano".to_string()),
                "messages": [{"role": "user", "content": "1"}],
                "max_tokens": 1,
                "temperature": 0
            }));

        if !inferred_no_auth {
            let key = provided_key.trim();
            if key.is_empty() {
                return Err(
                    "API key is required (leave empty to use no authentication)".to_string()
                );
            }
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let response = req
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            let snippet: String = body.chars().take(500).collect();
            log::error!(
                "OpenAI-compatible validate failed: status={} body_snippet={}",
                status,
                snippet
            );
            return Err(format!("HTTP {}: {}", status, snippet));
        }
    } else {
        return Err("Unsupported provider".to_string());
    }

    if !inferred_no_auth {
        let mut cache = API_KEY_CACHE
            .lock()
            .map_err(|_| "Failed to access cache".to_string())?;
        cache.insert(format!("ai_api_key_{}", provider), provided_key.clone());
        log::info!("API key validated and cached for provider: {}", provider);
    }

    Ok(())
}

/// Test an OpenAI-compatible endpoint without saving or caching anything.
#[tauri::command]
pub async fn test_openai_endpoint(
    base_url: String,
    model: String,
    api_key: Option<String>,
    no_auth: Option<bool>,
) -> Result<(), String> {
    let no_auth = no_auth.unwrap_or(false)
        || api_key
            .as_deref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true);

    let validate_url = normalize_chat_completions_url(&base_url);

    let client = reqwest::Client::new();
    let mut req = client
        .post(&validate_url)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": "1"}],
            "max_tokens": 1,
            "temperature": 0
        }));

    if !no_auth {
        let key = api_key.unwrap_or_default();
        if key.trim().is_empty() {
            return Err("API key is required (leave empty to use no authentication)".to_string());
        }
        req = req.header("Authorization", format!("Bearer {}", key));
    }

    let response = req
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status.is_success() {
        Ok(())
    } else {
        let snippet: String = body.chars().take(500).collect();
        log::error!(
            "OpenAI-compatible test failed: url={} status={} body_snippet={}",
            validate_url,
            status,
            snippet
        );
        Err(format!("HTTP {}: {}", status, snippet))
    }
}

// Frontend is responsible for removing API keys from Stronghold
// This command clears the cache
#[tauri::command]
pub async fn clear_ai_api_key_cache(
    _app: tauri::AppHandle,
    provider: String,
) -> Result<(), String> {
    // Skip validation if provider is empty (happens when clearing selection)
    if !provider.is_empty() {
        validate_provider_name(&provider)?;
    }

    let mut cache = API_KEY_CACHE
        .lock()
        .map_err(|_| "Failed to access cache".to_string())?;

    if !provider.is_empty() {
        cache.remove(&format!("ai_api_key_{}", provider));
        log::info!("API key cache cleared for provider: {}", provider);
    }

    Ok(())
}

// Clear entire API key cache (for reset)
pub fn clear_all_api_key_cache() -> Result<(), String> {
    let mut cache = API_KEY_CACHE
        .lock()
        .map_err(|_| "Failed to access cache".to_string())?;
    cache.clear();
    log::info!("Cleared entire API key cache");
    Ok(())
}

#[tauri::command]
pub async fn update_ai_settings(
    enabled: bool,
    provider: String,
    model: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    // Allow empty provider and model for deselection
    if !provider.is_empty() {
        validate_provider_name(&provider)?;
    }

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
        if provider == "openai" {
            let store = app.store("settings").map_err(|e| e.to_string())?;
            let cache_has_key = {
                let cache = API_KEY_CACHE
                    .lock()
                    .map_err(|_| "Failed to access cache".to_string())?;
                cache.contains_key(&format!("ai_api_key_{}", provider))
            };
            let configured_base = store.get("ai_openai_base_url").is_some();

            if !(cache_has_key || configured_base) {
                log::warn!(
                    "Attempted to enable AI enhancement without cached API key or configured base URL for provider: {}",
                    provider
                );
                return Err("API key not found. Please add an API key first.".to_string());
            }
        } else {
            let cache_has_key = {
                let cache = API_KEY_CACHE
                    .lock()
                    .map_err(|_| "Failed to access cache".to_string())?;
                cache.contains_key(&format!("ai_api_key_{}", provider))
            };
            if !cache_has_key {
                log::warn!(
                    "Attempted to enable AI enhancement without cached API key for provider: {}",
                    provider
                );
                return Err("API key not found. Please add an API key first.".to_string());
            }
        }
    }

    let store = app.store("settings").map_err(|e| e.to_string())?;

    store.set("ai_enabled", json!(enabled));
    store.set("ai_provider", json!(provider));
    store.set("ai_model", json!(model));

    store
        .save()
        .map_err(|e| format!("Failed to save AI settings: {}", e))?;

    // Invalidate recording config cache when AI settings change
    crate::commands::audio::invalidate_recording_config_cache(&app).await;

    log::info!(
        "AI settings updated: enabled={}, provider={}, model={}",
        enabled,
        provider,
        model
    );

    Ok(())
}

#[tauri::command]
pub async fn disable_ai_enhancement(app: tauri::AppHandle) -> Result<(), String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;

    store.set("ai_enabled", json!(false));

    store
        .save()
        .map_err(|e| format!("Failed to save AI settings: {}", e))?;

    // Invalidate recording config cache when AI settings change
    crate::commands::audio::invalidate_recording_config_cache(&app).await;

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
    app: tauri::AppHandle,
) -> Result<(), String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;

    store.set(
        "enhancement_options",
        serde_json::to_value(&options)
            .map_err(|e| format!("Failed to serialize options: {}", e))?,
    );

    store
        .save()
        .map_err(|e| format!("Failed to save enhancement options: {}", e))?;

    log::info!("Enhancement options updated: preset={:?}", options.preset);

    Ok(())
}

#[tauri::command]
pub async fn enhance_transcription(text: String, app: tauri::AppHandle) -> Result<String, String> {
    // Quick validation
    if text.trim().is_empty() {
        log::debug!("Skipping enhancement for empty text");
        return Ok(text);
    }

    let store = app.store("settings").map_err(|e| e.to_string())?;

    let enabled = store
        .get("ai_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !enabled {
        log::debug!("AI enhancement is disabled");
        return Ok(text); // Return original text if AI is not enabled
    }

    let provider = store
        .get("ai_provider")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "groq".to_string());

    let model = store
        .get("ai_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "".to_string()); // Empty by default

    // Don't enhance if no model selected
    if model.is_empty() {
        log::warn!(
            "AI enhancement enabled but no model selected. Provider: {}",
            provider
        );
        return Ok(text);
    }

    // Determine provider-specific config
    let (api_key, options) = if provider == "openai" {
        let base_url = store
            .get("ai_openai_base_url")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

        // Send Authorization only if a key is cached
        let cache = API_KEY_CACHE.lock().map_err(|e| {
            log::error!("Failed to access API key cache: {}", e);
            "Failed to access cache".to_string()
        })?;
        let key_name = format!("ai_api_key_{}", provider);
        let cached = cache.get(&key_name).cloned();

        // Log detailed information about API key lookup
        if cached.is_some() {
            log::info!("Using cached API key for OpenAI provider");
        } else {
            log::warn!("No cached API key found for OpenAI provider, using no-auth mode");
            log::debug!(
                "Available cache keys: {:?}",
                cache.keys().collect::<Vec<_>>()
            );
        }
        drop(cache);

        let mut opts = std::collections::HashMap::new();
        opts.insert("base_url".into(), serde_json::Value::String(base_url));
        opts.insert("no_auth".into(), serde_json::Value::Bool(cached.is_none()));

        (cached.unwrap_or_default(), opts)
    } else if provider == "groq" || provider == "gemini" {
        // Require API key from in-memory cache
        let cache = API_KEY_CACHE
            .lock()
            .map_err(|_| "Failed to access cache".to_string())?;
        let key_name = format!("ai_api_key_{}", provider);
        let api_key = cache.get(&key_name).cloned().ok_or_else(|| {
            log::error!(
                "API key not found in cache for provider: {}. Cache keys: {:?}",
                provider,
                cache.keys().collect::<Vec<_>>()
            );
            "API key not found in cache".to_string()
        })?;

        (api_key, std::collections::HashMap::new())
    } else {
        return Err("Unsupported provider".to_string());
    };

    drop(store); // Release lock before async operation

    // Load enhancement options
    let enhancement_options = get_enhancement_options(app.clone()).await.ok();

    log::info!(
        "Enhancing text with {} model {} (length: {}, options: {:?})",
        provider,
        model,
        text.len(),
        enhancement_options
    );

    // Create provider config
    let config = AIProviderConfig {
        provider,
        model,
        api_key,
        enabled: true,
        options,
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
            log::info!(
                "Text enhanced successfully (original: {}, enhanced: {})",
                text.len(),
                response.enhanced_text.len()
            );
            Ok(response.enhanced_text)
        }
        Err(e) => {
            log::error!("AI enhancement failed: {}", e);
            // Emit to pill window with short message only
            let _ = crate::emit_to_window(&app, "pill", "formatting-error", "Formatting failed");
            Err(format!("AI enhancement failed: {}", e))
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OpenAIConfig {
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    #[serde(rename = "noAuth")]
    pub no_auth: bool,
}

#[derive(Deserialize)]
pub struct SetOpenAIConfigArgs {
    #[serde(alias = "baseUrl", alias = "base_url")]
    pub base_url: String,
    #[serde(alias = "noAuth", alias = "no_auth")]
    pub no_auth: Option<bool>,
}

#[tauri::command]
pub async fn set_openai_config(
    app: tauri::AppHandle,
    args: SetOpenAIConfigArgs,
) -> Result<(), String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    store.set(
        "ai_openai_base_url",
        serde_json::Value::String(args.base_url),
    );
    if let Some(no_auth) = args.no_auth {
        // Backward-compatibility: accept but not required
        store.set("ai_openai_no_auth", serde_json::Value::Bool(no_auth));
    }
    store
        .save()
        .map_err(|e| format!("Failed to save AI settings: {}", e))?;
    Ok(())
}

#[tauri::command]
pub async fn get_openai_config(app: tauri::AppHandle) -> Result<OpenAIConfig, String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    let base_url = store
        .get("ai_openai_base_url")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
    let no_auth = store
        .get("ai_openai_no_auth")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    Ok(OpenAIConfig { base_url, no_auth })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_validation() {
        assert!(validate_provider_name("groq").is_ok());
        assert!(validate_provider_name("gemini").is_ok());
        assert!(validate_provider_name("openai").is_ok());
        assert!(validate_provider_name("test-provider").is_err());
        assert!(validate_provider_name("test_provider").is_err());
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
