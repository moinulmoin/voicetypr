use crate::ai::{AIEnhancementRequest, AIProviderConfig, AIProviderFactory, EnhancementOptions};
use crate::commands::audio::pill_toast;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri_plugin_store::StoreExt;

// In-memory cache for API keys to avoid system password prompts
// Keys are stored in Stronghold by frontend and cached here for backend use
static API_KEY_CACHE: Lazy<Mutex<HashMap<String, String>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const CUSTOM_BASE_URL_KEY: &str = "ai_custom_base_url";
const CUSTOM_NO_AUTH_KEY: &str = "ai_custom_no_auth";
const LEGACY_OPENAI_BASE_URL_KEY: &str = "ai_openai_base_url";
const LEGACY_OPENAI_NO_AUTH_KEY: &str = "ai_openai_no_auth";

// Helper: determine if we should consider that the app "has an API key" for a provider
// For OpenAI-compatible providers, a configured no_auth=true also counts as "has key"
fn check_has_api_key<R: tauri::Runtime>(
    provider: &str,
    store: &tauri_plugin_store::Store<R>,
    cache: &HashMap<String, String>,
) -> bool {
    if provider == "openai" {
        cache.contains_key("ai_api_key_openai") || store.get(LEGACY_OPENAI_BASE_URL_KEY).is_some()
    } else if provider == "custom" {
        let configured_base = store.get(CUSTOM_BASE_URL_KEY).is_some()
            || store.get(LEGACY_OPENAI_BASE_URL_KEY).is_some();
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

// Validation pattern for providers
lazy_static::lazy_static! {
    static ref PROVIDER_REGEX: regex::Regex = regex::Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap();
}

// Supported AI providers
const ALLOWED_PROVIDERS: &[&str] = &["gemini", "openai", "anthropic", "custom"];

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
        .unwrap_or_default(); // Empty by default, user must select

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

    log::info!("API key cached for provider: {}", provider);

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

    if provider == "openai" || provider == "custom" {
        let store = app.store("settings").map_err(|e| e.to_string())?;
        if let Some(url) = base_url.clone() {
            if provider == "custom" {
                store.set(CUSTOM_BASE_URL_KEY, serde_json::Value::String(url));
            } else {
                store.set(LEGACY_OPENAI_BASE_URL_KEY, serde_json::Value::String(url));
            }
        }
        store.set(
            if provider == "custom" {
                CUSTOM_NO_AUTH_KEY
            } else {
                LEGACY_OPENAI_NO_AUTH_KEY
            },
            serde_json::Value::Bool(inferred_no_auth),
        );
        if let Some(m) = model.clone() {
            store.set("ai_model", serde_json::Value::String(m));
        }
        store
            .save()
            .map_err(|e| format!("Failed to save AI settings: {}", e))?;
    }

    if provider == "openai" || provider == "custom" {
        let store = app.store("settings").map_err(|e| e.to_string())?;

        let base = base_url
            .clone()
            .or_else(|| {
                if provider == "custom" {
                    store
                        .get(CUSTOM_BASE_URL_KEY)
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                        .or_else(|| {
                            store
                                .get(LEGACY_OPENAI_BASE_URL_KEY)
                                .and_then(|v| v.as_str().map(|s| s.to_string()))
                        })
                } else {
                    store
                        .get(LEGACY_OPENAI_BASE_URL_KEY)
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                }
            })
            .unwrap_or_else(|| DEFAULT_OPENAI_BASE_URL.to_string());
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

    // Don't allow enabling without a model selected
    if enabled && model.is_empty() {
        log::warn!("Attempted to enable AI enhancement without a model selected");
        return Err("Please select a model before enabling AI enhancement".to_string());
    }

    // Check if API key exists when enabling
    if enabled {
        if provider == "custom" {
            let store = app.store("settings").map_err(|e| e.to_string())?;
            let cache_has_key = {
                let cache = API_KEY_CACHE
                    .lock()
                    .map_err(|_| "Failed to access cache".to_string())?;
                cache.contains_key("ai_api_key_custom")
            };
            let configured_base = store.get(CUSTOM_BASE_URL_KEY).is_some()
                || store.get(LEGACY_OPENAI_BASE_URL_KEY).is_some();

            if !(cache_has_key || configured_base) {
                log::warn!(
                    "Attempted to enable AI enhancement without cached API key or configured base URL for provider: {}",
                    provider
                );
                return Err("API key not found. Please add an API key first.".to_string());
            }
        } else if provider == "openai" {
            let store = app.store("settings").map_err(|e| e.to_string())?;
            let cache_has_key = {
                let cache = API_KEY_CACHE
                    .lock()
                    .map_err(|_| "Failed to access cache".to_string())?;
                cache.contains_key("ai_api_key_openai")
            };
            let legacy_custom_config = store.get(LEGACY_OPENAI_BASE_URL_KEY).is_some();

            if !(cache_has_key || legacy_custom_config) {
                log::warn!(
                    "Attempted to enable AI enhancement without cached API key for provider: {}",
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
        .unwrap_or_default(); // Empty by default

    let model = store
        .get("ai_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "".to_string()); // Empty by default

    // Don't enhance if no model or provider selected
    if model.is_empty() || provider.is_empty() {
        log::warn!(
            "AI enhancement enabled but no model/provider selected. Provider: {}",
            provider
        );
        return Ok(text);
    }

    // Determine provider-specific config
    let (factory_provider, api_key, options) = if provider == "openai" {
        let cache = API_KEY_CACHE.lock().map_err(|e| {
            log::error!("Failed to access API key cache: {}", e);
            "Failed to access cache".to_string()
        })?;

        let openai_cached = cache.get("ai_api_key_openai").cloned();
        let custom_cached = cache.get("ai_api_key_custom").cloned();
        drop(cache);

        if let Some(cached) = openai_cached {
            let mut opts = std::collections::HashMap::new();
            opts.insert(
                "base_url".into(),
                serde_json::Value::String(DEFAULT_OPENAI_BASE_URL.to_string()),
            );
            opts.insert("no_auth".into(), serde_json::Value::Bool(false));

            ("openai".to_string(), cached, opts)
        } else if let Some(legacy_base_url) = store
            .get(LEGACY_OPENAI_BASE_URL_KEY)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
        {
            log::warn!("Using legacy OpenAI-compatible configuration for openai provider");
            let mut opts = std::collections::HashMap::new();
            opts.insert(
                "base_url".into(),
                serde_json::Value::String(legacy_base_url),
            );
            opts.insert(
                "no_auth".into(),
                serde_json::Value::Bool(custom_cached.is_none()),
            );

            (
                "openai".to_string(),
                custom_cached.unwrap_or_default(),
                opts,
            )
        } else {
            log::error!(
                "API key not found in cache for OpenAI provider. Cache keys unavailable for OpenAI path"
            );
            return Err("API key not found in cache".to_string());
        }
    } else if provider == "custom" {
        let base_url = store
            .get(CUSTOM_BASE_URL_KEY)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .or_else(|| {
                store
                    .get(LEGACY_OPENAI_BASE_URL_KEY)
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
            })
            .unwrap_or_else(|| DEFAULT_OPENAI_BASE_URL.to_string());

        let cache = API_KEY_CACHE.lock().map_err(|e| {
            log::error!("Failed to access API key cache: {}", e);
            "Failed to access cache".to_string()
        })?;

        let cached = cache.get("ai_api_key_custom").cloned();

        if cached.is_some() {
            log::info!("Using cached API key for custom provider");
        } else {
            log::warn!("No cached API key found for custom provider, using no-auth mode");
            log::debug!(
                "Available cache keys: {:?}",
                cache.keys().collect::<Vec<_>>()
            );
        }
        drop(cache);

        let mut opts = std::collections::HashMap::new();
        opts.insert("base_url".into(), serde_json::Value::String(base_url));
        opts.insert("no_auth".into(), serde_json::Value::Bool(cached.is_none()));

        ("openai".to_string(), cached.unwrap_or_default(), opts)
    } else if provider == "gemini" || provider == "anthropic" {
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

        (provider.clone(), api_key, std::collections::HashMap::new())
    } else {
        return Err("Unsupported provider".to_string());
    };

    drop(store); // Release lock before async operation

    // Load enhancement options
    let enhancement_options = get_enhancement_options(app.clone()).await.ok();

    // Get the user's selected language for formatting output
    let language = {
        let lang_store = app.store("settings").map_err(|e| e.to_string())?;
        lang_store
            .get("language")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
    };

    log::info!(
        "Enhancing text with {} model {} (length: {}, options: {:?}, language: {:?})",
        provider,
        model,
        text.len(),
        enhancement_options,
        language
    );

    // Create provider config
    let config = AIProviderConfig {
        provider: factory_provider,
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
        language,
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
            log::error!("AI formatting failed: {}", e);
            // Emit formatting error via pill toast
            pill_toast(&app, "Formatting failed", 1500);
            Err(format!("AI formatting failed: {}", e))
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
        CUSTOM_BASE_URL_KEY,
        serde_json::Value::String(args.base_url),
    );
    if let Some(no_auth) = args.no_auth {
        // Backward-compatibility: accept but not required
        store.set(CUSTOM_NO_AUTH_KEY, serde_json::Value::Bool(no_auth));
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
        .get(CUSTOM_BASE_URL_KEY)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .or_else(|| {
            store
                .get(LEGACY_OPENAI_BASE_URL_KEY)
                .and_then(|v| v.as_str().map(|s| s.to_string()))
        })
        .unwrap_or_else(|| DEFAULT_OPENAI_BASE_URL.to_string());
    let no_auth = store
        .get(CUSTOM_NO_AUTH_KEY)
        .and_then(|v| v.as_bool())
        .or_else(|| {
            store
                .get(LEGACY_OPENAI_NO_AUTH_KEY)
                .and_then(|v| v.as_bool())
        })
        .unwrap_or(false);
    Ok(OpenAIConfig { base_url, no_auth })
}

// ============================================================================
// Curated Model List (Static - No API Fetching)
// ============================================================================

/// A model available from a provider
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderModel {
    pub id: String,
    pub name: String,
    pub recommended: bool,
}

/// Curated list of OpenAI models for text formatting
/// Using GPT-5 nano/mini with minimal reasoning for fast, cost-effective formatting
const OPENAI_MODELS: &[(&str, &str, bool)] = &[
    ("gpt-5-nano", "GPT-5 Nano", true),
    ("gpt-5-mini", "GPT-5 Mini", true),
];

/// Curated list of Anthropic Claude models for text formatting
/// Using Claude 4.5 Haiku/Sonnet - fast and balanced options
const ANTHROPIC_MODELS: &[(&str, &str, bool)] = &[
    ("claude-haiku-4-5-latest", "Claude 4.5 Haiku", true),
    ("claude-sonnet-4-5-latest", "Claude 4.5 Sonnet", true),
];

/// Curated list of Google Gemini models for text formatting
/// Using Flash variants - optimized for speed and cost
const GEMINI_MODELS: &[(&str, &str, bool)] = &[
    ("gemini-3-flash-preview", "Gemini 3 Flash", true),
    ("gemini-2.5-flash", "Gemini 2.5 Flash", true),
    ("gemini-2.5-flash-lite", "Gemini 2.5 Flash Lite", true),
];

/// Get curated models for a provider (no API call needed)
fn get_curated_models(provider: &str) -> Vec<ProviderModel> {
    let models: &[(&str, &str, bool)] = match provider {
        "openai" => OPENAI_MODELS,
        "anthropic" => ANTHROPIC_MODELS,
        "gemini" => GEMINI_MODELS,
        _ => return vec![],
    };

    models
        .iter()
        .map(|(id, name, recommended)| ProviderModel {
            id: id.to_string(),
            name: name.to_string(),
            recommended: *recommended,
        })
        .collect()
}

/// List available models for a provider
/// Returns curated static list - no API call needed
#[tauri::command]
pub async fn list_provider_models(
    provider: String,
    _app: tauri::AppHandle,
) -> Result<Vec<ProviderModel>, String> {
    // Validate provider
    if !["openai", "anthropic", "gemini"].contains(&provider.as_str()) {
        return Err(format!(
            "Unsupported provider for model listing: {}",
            provider
        ));
    }

    let models = get_curated_models(&provider);
    log::info!(
        "Returning {} curated models for provider {}",
        models.len(),
        provider
    );

    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_validation() {
        // Valid providers
        assert!(validate_provider_name("gemini").is_ok());
        assert!(validate_provider_name("openai").is_ok());
        assert!(validate_provider_name("anthropic").is_ok());
        assert!(validate_provider_name("custom").is_ok());

        // Groq is no longer supported
        assert!(validate_provider_name("groq").is_err());

        // Invalid formats
        assert!(validate_provider_name("test-provider").is_err());
        assert!(validate_provider_name("test_provider").is_err());
        assert!(validate_provider_name("test provider").is_err());
        assert!(validate_provider_name("test@provider").is_err());
        assert!(validate_provider_name("").is_err());
    }

    #[test]
    fn test_curated_models() {
        // OpenAI models
        let openai_models = get_curated_models("openai");
        assert_eq!(openai_models.len(), 2);
        assert!(openai_models.iter().any(|m| m.id == "gpt-5-nano"));
        assert!(openai_models.iter().any(|m| m.id == "gpt-5-mini"));

        // Anthropic models
        let anthropic_models = get_curated_models("anthropic");
        assert_eq!(anthropic_models.len(), 2);
        assert!(anthropic_models
            .iter()
            .any(|m| m.id == "claude-haiku-4-5-latest"));
        assert!(anthropic_models
            .iter()
            .any(|m| m.id == "claude-sonnet-4-5-latest"));

        // Gemini models
        let gemini_models = get_curated_models("gemini");
        assert_eq!(gemini_models.len(), 3);
        assert!(gemini_models
            .iter()
            .any(|m| m.id == "gemini-3-flash-preview"));
        assert!(gemini_models.iter().any(|m| m.id == "gemini-2.5-flash"));
        assert!(gemini_models
            .iter()
            .any(|m| m.id == "gemini-2.5-flash-lite"));

        // Unknown provider returns empty list
        let unknown_models = get_curated_models("unknown");
        assert!(unknown_models.is_empty());
    }
}
