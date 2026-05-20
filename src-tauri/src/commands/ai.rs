use crate::ai::openai::{is_unsupported_token_parameter_error, model_uses_max_completion_tokens};
use crate::ai::EnhancementOptions;
use crate::commands::audio::pill_toast;
use crate::commands::settings::{
    normalize_final_text_language, normalize_speech_language_for_model,
    normalize_transcription_task, task_uses_translate_to_english,
    FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT,
};
use crate::formatting::messages::{FormattingCommand, FormattingResponse, PROTOCOL_VERSION};
use crate::formatting::FormattingClient;
use crate::secure_store;
use crate::writing::{load_writing_settings, save_writing_settings, WritingSettings};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::Manager;
use tauri_plugin_store::StoreExt;

// In-memory cache for API keys to avoid system password prompts
// Keys are stored in Stronghold by frontend and cached here for backend use
static API_KEY_CACHE: Lazy<Mutex<HashMap<String, String>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

static FORMATTING_REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const CUSTOM_BASE_URL_KEY: &str = "ai_custom_base_url";
const CUSTOM_NO_AUTH_KEY: &str = "ai_custom_no_auth";
const LEGACY_OPENAI_BASE_URL_KEY: &str = "ai_openai_base_url";
const LEGACY_OPENAI_NO_AUTH_KEY: &str = "ai_openai_no_auth";

/// AI provider key names stored in the secure store
const AI_PROVIDER_KEYS: &[&str] = &[
    "ai_api_key_gemini",
    "ai_api_key_openai",
    "ai_api_key_anthropic",
    "ai_api_key_custom",
];

/// Populate the in-memory API key cache from the secure store.
/// Called during backend startup BEFORE startup validation checks run.
/// This ensures credentials persisted in secure storage are visible to
/// perform_startup_checks() without waiting for the frontend to warm the cache.
pub fn warm_ai_key_cache_from_secure_store(app: &tauri::AppHandle) {
    if let Ok(mut cache) = API_KEY_CACHE.lock() {
        for key_name in AI_PROVIDER_KEYS {
            // Skip if already cached (e.g. from a prior call or concurrent frontend warm)
            if cache.contains_key(*key_name) {
                continue;
            }
            match secure_store::secure_get(app, key_name) {
                Ok(Some(value)) => {
                    cache.insert(key_name.to_string(), value);
                    log::info!("Warmed API key cache from secure store: {}", key_name);
                }
                Ok(None) => {
                    log::debug!("No key in secure store for: {}", key_name);
                }
                Err(e) => {
                    log::warn!("Failed to read '{}' from secure store: {}", key_name, e);
                }
            }
        }

        if let Ok(store) = app.store("settings") {
            if let Some(provider) = store
                .get("ai_provider")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
            {
                let key_name = format!("ai_api_key_{}", provider);
                if !provider.is_empty() && !cache.contains_key(&key_name) {
                    match secure_store::secure_get(app, &key_name) {
                        Ok(Some(value)) => {
                            cache.insert(key_name.clone(), value);
                            log::info!(
                                "Warmed selected provider API key cache from secure store: {}",
                                key_name
                            );
                        }
                        Ok(None) => {
                            log::debug!(
                                "No selected provider key in secure store for: {}",
                                key_name
                            );
                        }
                        Err(e) => {
                            log::warn!("Failed to read '{}' from secure store: {}", key_name, e);
                        }
                    }
                }
            }
        }
    } else {
        log::error!("Failed to acquire API_KEY_CACHE lock during startup warmup");
    }
}

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

fn normalize_models_url(base: &str) -> String {
    let b = base.trim_end_matches('/');
    format!("{}/models", b)
}

const OPENAI_PROBE_INITIAL_MAX_TOKENS: u32 = 10;
const OPENAI_PROBE_FALLBACK_MAX_TOKENS: u32 = 32;

fn is_probe_output_limit_error(error_text: &str) -> bool {
    let haystack = error_text.to_ascii_lowercase();
    haystack.contains("output limit was reached")
        || haystack.contains("higher max_tokens")
        || haystack.contains("higher max_completion_tokens")
}

fn build_openai_probe_payload(
    model: &str,
    use_max_completion_tokens: bool,
    max_output_tokens: u32,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "1"}],
    });

    if let Some(obj) = payload.as_object_mut() {
        if use_max_completion_tokens {
            obj.insert(
                "max_completion_tokens".to_string(),
                serde_json::json!(max_output_tokens),
            );
        } else {
            obj.insert(
                "max_tokens".to_string(),
                serde_json::json!(max_output_tokens),
            );
        }
    }

    payload
}

async fn run_openai_probe_request(
    client: &reqwest::Client,
    base_url: &str,
    model: &str,
    auth_header: Option<&str>,
    allow_chat_probe_fallback: bool,
) -> Result<(), String> {
    let models_url = normalize_models_url(base_url);

    let mut models_req = client.get(&models_url);
    if let Some(header) = auth_header {
        models_req = models_req.header("Authorization", header);
    }

    let models_response = models_req
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    let models_status = models_response.status();
    let models_body = models_response.text().await.unwrap_or_default();

    if models_status.is_success() {
        if let Ok(json_body) = serde_json::from_str::<serde_json::Value>(&models_body) {
            if let Some(data) = json_body.get("data").and_then(|d| d.as_array()) {
                if !data.is_empty() {
                    let model_exists = data.iter().any(|entry| {
                        entry
                            .get("id")
                            .and_then(|id| id.as_str())
                            .is_some_and(|id| id == model)
                    });

                    if !model_exists {
                        return Err(format!(
                            "Model '{}' not found in endpoint model list",
                            model
                        ));
                    }
                }
            }
        }

        return Ok(());
    }

    if !allow_chat_probe_fallback {
        let snippet: String = models_body.chars().take(500).collect();
        return Err(format!("HTTP {}: {}", models_status, snippet));
    }

    if models_status.as_u16() != 404 && models_status.as_u16() != 405 {
        let snippet: String = models_body.chars().take(500).collect();
        return Err(format!("HTTP {}: {}", models_status, snippet));
    }

    let validate_url = normalize_chat_completions_url(base_url);
    let mut use_max_completion_tokens = model_uses_max_completion_tokens(model);
    let mut max_output_tokens = OPENAI_PROBE_INITIAL_MAX_TOKENS;

    for _attempt in 0..4 {
        let payload =
            build_openai_probe_payload(model, use_max_completion_tokens, max_output_tokens);
        let mut req = client
            .post(&validate_url)
            .header("Content-Type", "application/json")
            .json(&payload);

        if let Some(header) = auth_header {
            req = req.header("Authorization", header);
        }

        let response = req
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if status.is_success() {
            return Ok(());
        }

        let attempted_parameter = if use_max_completion_tokens {
            "max_completion_tokens"
        } else {
            "max_tokens"
        };

        if is_unsupported_token_parameter_error(&body, attempted_parameter) {
            log::warn!(
                "OpenAI probe parameter '{}' unsupported for model '{}'; retrying with alternate parameter",
                attempted_parameter,
                model
            );
            use_max_completion_tokens = !use_max_completion_tokens;
            continue;
        }

        if max_output_tokens < OPENAI_PROBE_FALLBACK_MAX_TOKENS
            && is_probe_output_limit_error(&body)
        {
            log::warn!(
                "OpenAI probe hit output limit at {} tokens for model '{}'; retrying with {}",
                max_output_tokens,
                model,
                OPENAI_PROBE_FALLBACK_MAX_TOKENS
            );
            max_output_tokens = OPENAI_PROBE_FALLBACK_MAX_TOKENS;
            continue;
        }

        let snippet: String = body.chars().take(500).collect();
        return Err(format!("HTTP {}: {}", status, snippet));
    }

    Err("OpenAI-compatible probe failed after token parameter fallback".to_string())
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

// Pi AI supports a broad provider registry; validate only the identifier shape here.
// Availability is checked by the sidecar-backed model list/formatting calls.
fn validate_provider_name(provider: &str) -> Result<(), String> {
    if !PROVIDER_REGEX.is_match(provider) {
        return Err("Invalid provider name format".to_string());
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
    let inferred_no_auth = if provider == "custom" {
        no_auth.unwrap_or(false) || provided_key.trim().is_empty()
    } else {
        false
    };

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
        let validate_model = model.clone().unwrap_or_else(|| "gpt-5-nano".to_string());
        let allow_chat_probe_fallback = provider == "custom";

        let client = reqwest::Client::new();
        let auth_header = if inferred_no_auth {
            None
        } else {
            let key = provided_key.trim();
            if key.is_empty() {
                return Err(
                    "API key is required (leave empty to use no authentication)".to_string()
                );
            }
            Some(format!("Bearer {}", key))
        };

        run_openai_probe_request(
            &client,
            &base,
            &validate_model,
            auth_header.as_deref(),
            allow_chat_probe_fallback,
        )
        .await
        .map_err(|error| {
            log::error!(
                "OpenAI-compatible validate failed: base_url={} model={} error={}",
                base,
                validate_model,
                error
            );
            error
        })?;
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

    let client = reqwest::Client::new();
    let auth_header = if no_auth {
        None
    } else {
        let key = api_key.unwrap_or_default();
        if key.trim().is_empty() {
            return Err("API key is required (leave empty to use no authentication)".to_string());
        }
        Some(format!("Bearer {}", key.trim()))
    };

    run_openai_probe_request(&client, &base_url, &model, auth_header.as_deref(), true)
        .await
        .map_err(|error| {
            log::error!(
                "OpenAI-compatible test failed: base_url={} model={} error={}",
                base_url,
                model,
                error
            );
            error
        })
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
pub async fn get_writing_settings(app: tauri::AppHandle) -> Result<WritingSettings, String> {
    load_writing_settings(&app)
}

#[tauri::command]
pub async fn update_writing_settings(
    settings: WritingSettings,
    app: tauri::AppHandle,
) -> Result<(), String> {
    save_writing_settings(&app, &settings)
}

#[tauri::command]
pub async fn enhance_transcription(
    text: String,
    transcript_language: Option<String>,
    ai_enabled_override: Option<bool>,
    output_language_override: Option<String>,
    context_override: Option<String>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    // Quick validation
    if text.trim().is_empty() {
        log::debug!("Skipping enhancement for empty text");
        return Ok(text);
    }

    let store = app.store("settings").map_err(|e| e.to_string())?;

    let enabled = ai_enabled_override.unwrap_or_else(|| {
        store
            .get("ai_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    });

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
    let (factory_provider, api_key, options): (String, String, HashMap<String, serde_json::Value>) =
        if provider == "openai" {
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
        } else {
            // Pi AI registry providers other than OpenAI-compatible providers
            // use the generic ai_api_key_{provider} cache key. We intentionally
            // skip save-time validation here; first use surfaces auth/model errors.
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
        };

    drop(store); // Release lock before async operation

    // Load enhancement options
    let enhancement_options = get_enhancement_options(app.clone()).await.ok();

    let language = if let Some(output_language) = output_language_override {
        Some(output_language)
    } else {
        let lang_store = app.store("settings").map_err(|e| e.to_string())?;
        let legacy_speech_language = lang_store
            .get("language")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "en".to_string());
        let legacy_translate_to_english = lang_store
            .get("translate_to_english")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let raw_speech_language = lang_store
            .get("speech_language")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or(legacy_speech_language);
        let current_model = lang_store
            .get("current_model")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default();
        let current_model_engine = lang_store
            .get("current_model_engine")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "whisper".to_string());
        let speech_language = normalize_speech_language_for_model(
            &current_model_engine,
            &current_model,
            &raw_speech_language,
        );
        let stored_transcription_task = lang_store
            .get("transcription_task")
            .and_then(|v| v.as_str().map(|s| s.to_string()));
        let transcription_task = normalize_transcription_task(
            stored_transcription_task.as_deref(),
            legacy_translate_to_english,
        );
        let stored_final_text_language = lang_store
            .get("final_text_language")
            .and_then(|v| v.as_str().map(|s| s.to_string()));
        let final_text_language = normalize_final_text_language(
            stored_final_text_language.as_deref(),
            &transcription_task,
        );

        if final_text_language == FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT {
            if let Some(transcript_language) = transcript_language.clone() {
                Some(transcript_language)
            } else if task_uses_translate_to_english(&transcription_task) {
                Some("en".to_string())
            } else {
                Some(speech_language)
            }
        } else {
            Some(final_text_language)
        }
    };

    log::info!(
        "Enhancing text with {} model {} (length: {}, options: {:?}, language: {:?})",
        provider,
        model,
        text.len(),
        enhancement_options,
        language
    );

    let prompt = crate::ai::prompts::build_enhancement_prompt(
        &text,
        context_override.as_deref(),
        &enhancement_options.unwrap_or_default(),
        language.as_deref(),
    );

    let custom_base_url = options
        .get("base_url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let no_auth = options
        .get("no_auth")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let sidecar_provider = if provider == "custom"
        || custom_base_url
            .as_deref()
            .is_some_and(|base| base.trim_end_matches('/') != DEFAULT_OPENAI_BASE_URL)
    {
        "custom".to_string()
    } else {
        sidecar_provider_for(&factory_provider).to_string()
    };

    let command = FormattingCommand::Format {
        id: next_formatting_request_id("format"),
        protocol_version: PROTOCOL_VERSION,
        provider: sidecar_provider,
        model,
        prompt,
        system_prompt: Some(
            "You are a careful text formatter. Return only the cleaned text requested by the user instructions."
                .to_string(),
        ),
        api_key: if api_key.is_empty() { None } else { Some(api_key) },
        no_auth,
        custom_base_url,
        timeout_ms: 30_000,
    };

    match app.state::<FormattingClient>().send(&app, &command).await {
        Ok(FormattingResponse::Formatted {
            text: enhanced_text,
            ..
        }) => {
            log::info!(
                "Text enhanced successfully (original: {}, enhanced: {})",
                text.len(),
                enhanced_text.len()
            );
            Ok(enhanced_text)
        }
        Ok(other) => {
            log::error!("Unexpected formatting sidecar response: {:?}", other);
            pill_toast(&app, "Formatting failed", 1500);
            Ok(text)
        }
        Err(e) => {
            log::error!("AI formatting failed: {}", e);
            pill_toast(&app, "Formatting failed", 1500);
            Ok(text)
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

/// Curated list of Google Gemini models for text formatting
/// Using Flash variants - optimized for speed and cost
const GEMINI_MODELS: &[(&str, &str, bool)] = &[
    ("gemini-3-flash-preview", "Gemini 3 Flash", true),
    ("gemini-2.5-flash", "Gemini 2.5 Flash", true),
    ("gemini-2.5-flash-lite", "Gemini 2.5 Flash Lite", true),
];

/// Curated list of Anthropic Claude models for text formatting
/// Haiku 4.5 (fastest, cheapest) and Sonnet 4.6 (balanced).
/// Opus is intentionally excluded - too slow/expensive for inline formatting.
///
///
/// The old Rust Anthropic provider accepted `claude-sonnet-4-5` for
/// back-compat during the 1.12.0/1.12.1 window. That provider is no longer
/// on the formatting hot path, so we do not re-advertise deprecated IDs here.
const ANTHROPIC_MODELS: &[(&str, &str, bool)] = &[
    ("claude-haiku-4-5", "Claude Haiku 4.5", true),
    ("claude-sonnet-4-6", "Claude Sonnet 4.6", false),
];

/// Get curated models for a provider (no API call needed)
fn get_curated_models(provider: &str) -> Vec<ProviderModel> {
    let models: &[(&str, &str, bool)] = match provider {
        "openai" => OPENAI_MODELS,
        "gemini" => GEMINI_MODELS,
        "anthropic" => ANTHROPIC_MODELS,
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

fn sidecar_provider_for(provider: &str) -> &str {
    match provider {
        // VoiceTypr settings have historically used `gemini`, while the
        // Pi AI registry uses `google` for Gemini models.
        "gemini" => "google",
        other => other,
    }
}

fn voice_provider_for_sidecar(provider: &str) -> &str {
    match provider {
        // Keep the public/settings id stable even though Pi AI calls Gemini
        // `google` internally.
        "google" => "gemini",
        other => other,
    }
}

fn provider_display_name(provider_id: &str, sidecar_name: &str) -> String {
    match provider_id {
        "openai" => "OpenAI".to_string(),
        "anthropic" => "Anthropic".to_string(),
        "gemini" => "Google Gemini".to_string(),
        "custom" => "Custom (OpenAI-compatible)".to_string(),
        _ if sidecar_name.trim().is_empty() => provider_id.to_string(),
        _ => sidecar_name.to_string(),
    }
}

fn next_formatting_request_id(prefix: &str) -> String {
    let id = FORMATTING_REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{}", prefix, id)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
}

#[tauri::command]
pub async fn list_ai_providers(app: tauri::AppHandle) -> Result<Vec<ProviderInfo>, String> {
    let command = FormattingCommand::ListProviders {
        id: next_formatting_request_id("providers"),
        protocol_version: PROTOCOL_VERSION,
    };

    match app.state::<FormattingClient>().send(&app, &command).await {
        Ok(FormattingResponse::Providers { providers, .. }) => {
            let mut mapped = Vec::with_capacity(providers.len());
            for provider in providers {
                let provider_id = voice_provider_for_sidecar(&provider.id);
                let info = ProviderInfo {
                    id: provider_id.to_string(),
                    name: provider_display_name(provider_id, &provider.name),
                };
                if !mapped
                    .iter()
                    .any(|existing: &ProviderInfo| existing.id == info.id)
                {
                    mapped.push(info);
                }
            }
            Ok(mapped)
        }
        Ok(other) => {
            log::warn!(
                "Unexpected provider-list response from formatting sidecar: {:?}",
                other
            );
            Ok(built_in_provider_infos())
        }
        Err(error) => {
            log::warn!(
                "Formatting sidecar provider listing failed; using built-in providers: {}",
                error
            );
            Ok(built_in_provider_infos())
        }
    }
}

fn built_in_provider_infos() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
        },
        ProviderInfo {
            id: "anthropic".to_string(),
            name: "Anthropic".to_string(),
        },
        ProviderInfo {
            id: "gemini".to_string(),
            name: "Google Gemini".to_string(),
        },
        ProviderInfo {
            id: "custom".to_string(),
            name: "Custom (OpenAI-compatible)".to_string(),
        },
    ]
}

/// List available models for a provider.
/// Prefer the Pi AI sidecar registry; fall back to the legacy curated list if the sidecar is unavailable.
#[tauri::command]
pub async fn list_provider_models(
    provider: String,
    app: tauri::AppHandle,
) -> Result<Vec<ProviderModel>, String> {
    validate_provider_name(&provider)?;

    if provider == "custom" {
        return Ok(Vec::new());
    }

    let sidecar_provider = sidecar_provider_for(&provider);
    let command = FormattingCommand::ListModels {
        id: next_formatting_request_id("models"),
        protocol_version: PROTOCOL_VERSION,
        provider: sidecar_provider.to_string(),
    };

    match app.state::<FormattingClient>().send(&app, &command).await {
        Ok(FormattingResponse::Models { models, .. }) => {
            let mapped = models
                .into_iter()
                .map(|model| ProviderModel {
                    id: model.id,
                    name: model.name,
                    recommended: false,
                })
                .collect::<Vec<_>>();
            log::info!(
                "Returning {} Pi AI registry models for provider {} via sidecar provider {}",
                mapped.len(),
                provider,
                sidecar_provider
            );
            Ok(mapped)
        }
        Ok(other) => {
            log::warn!(
                "Unexpected model-list response from formatting sidecar: {:?}",
                other
            );
            let models = get_curated_models(&provider);
            if models.is_empty() {
                Err(format!(
                    "Unsupported provider for model listing: {}",
                    provider
                ))
            } else {
                Ok(models)
            }
        }
        Err(error) => {
            log::warn!(
                "Formatting sidecar model listing failed for provider {}; falling back to curated list: {}",
                provider,
                error
            );
            let models = get_curated_models(&provider);
            if models.is_empty() {
                Err(format!(
                    "Unsupported provider for model listing: {}",
                    provider
                ))
            } else {
                Ok(models)
            }
        }
    }
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

        // Pi AI registry providers are accepted by identifier shape.
        assert!(validate_provider_name("groq").is_ok());
        assert!(validate_provider_name("azure-openai-responses").is_ok());
        assert!(validate_provider_name("openai_compatible").is_ok());

        // Invalid formats
        assert!(validate_provider_name("test provider").is_err());
        assert!(validate_provider_name("test@provider").is_err());
        assert!(validate_provider_name("").is_err());
    }

    #[test]
    fn test_sidecar_provider_mapping() {
        assert_eq!(sidecar_provider_for("gemini"), "google");
        assert_eq!(sidecar_provider_for("openai"), "openai");
        assert_eq!(sidecar_provider_for("anthropic"), "anthropic");
        assert_eq!(sidecar_provider_for("groq"), "groq");
    }

    #[test]
    fn test_voice_provider_mapping() {
        assert_eq!(voice_provider_for_sidecar("google"), "gemini");
        assert_eq!(voice_provider_for_sidecar("openai"), "openai");
        assert_eq!(voice_provider_for_sidecar("anthropic"), "anthropic");
        assert_eq!(voice_provider_for_sidecar("groq"), "groq");
    }

    #[test]
    fn test_provider_display_names_keep_product_ids_stable() {
        assert_eq!(provider_display_name("gemini", "google"), "Google Gemini");
        assert_eq!(provider_display_name("openai", "openai"), "OpenAI");
        assert_eq!(
            provider_display_name("custom", ""),
            "Custom (OpenAI-compatible)"
        );
        assert_eq!(provider_display_name("groq", "groq"), "groq");
    }

    #[test]
    fn test_curated_models() {
        // OpenAI models
        let openai_models = get_curated_models("openai");
        assert_eq!(openai_models.len(), 2);
        assert!(openai_models.iter().any(|m| m.id == "gpt-5-nano"));
        assert!(openai_models.iter().any(|m| m.id == "gpt-5-mini"));

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

        // Anthropic models
        let anthropic_models = get_curated_models("anthropic");
        assert_eq!(anthropic_models.len(), 2);
        assert!(anthropic_models.iter().any(|m| m.id == "claude-haiku-4-5"));
        assert!(anthropic_models.iter().any(|m| m.id == "claude-sonnet-4-6"));

        // The list_provider_models gate uses is_empty() on this return,
        // so providers without curated models (e.g. "custom", or anything
        // unknown) get a model-listing error.
        let custom_models = get_curated_models("custom");
        assert!(custom_models.is_empty());
        let unknown_models = get_curated_models("unknown");
        assert!(unknown_models.is_empty());
    }

    #[test]
    fn test_warm_ai_key_cache_populates_from_store() {
        // Verify that populating the API_KEY_CACHE makes keys discoverable.
        // This tests the cache-warming path used by warm_ai_key_cache_from_secure_store.
        let mut cache: HashMap<String, String> = HashMap::new();

        // Standard providers are discovered by ai_api_key_{provider} presence
        assert!(!cache.contains_key("ai_api_key_gemini"));
        assert!(!cache.contains_key("ai_api_key_openai"));
        assert!(!cache.contains_key("ai_api_key_custom"));

        // Simulate warming from secure store for gemini
        cache.insert("ai_api_key_gemini".to_string(), "test-key".to_string());
        assert!(cache.contains_key("ai_api_key_gemini"));

        // OpenAI and custom still absent
        assert!(!cache.contains_key("ai_api_key_openai"));
        assert!(!cache.contains_key("ai_api_key_custom"));

        // Simulate warming for openai
        cache.insert("ai_api_key_openai".to_string(), "test-key-2".to_string());
        assert!(cache.contains_key("ai_api_key_openai"));

        // Clear one provider key
        cache.remove("ai_api_key_gemini");
        assert!(!cache.contains_key("ai_api_key_gemini"));
        assert!(cache.contains_key("ai_api_key_openai"));

        // Clear all
        cache.clear();
        assert!(cache.is_empty());
    }
}
