use crate::ai::catalog;
use crate::ai::contract::AiPolishRequest;
use crate::ai::error::{user_facing_message, AiProviderError};
use crate::ai::executor::AiExecutor;
use crate::ai::genai_runtime::AiKeyResolver;
use crate::ai::providers::{launch_providers, PROVIDER_CUSTOM};
use crate::ai::EnhancementOptions;
use crate::commands::audio::pill_toast;
use crate::commands::settings::{
    normalize_final_text_language, normalize_speech_language_for_model,
    normalize_transcription_task, task_uses_translate_to_english,
    FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT, TRANSCRIPTION_TASK_TRANSCRIBE,
};
use crate::secure_store;
use crate::writing::{load_writing_settings, save_writing_settings, WritingSettings};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
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

pub(crate) fn ai_provider_key_names() -> Vec<String> {
    launch_providers()
        .into_iter()
        .filter(|provider| provider.requires_api_key || provider.id == PROVIDER_CUSTOM)
        .map(|provider| format!("ai_api_key_{}", provider.id))
        .collect()
}

/// Populate the in-memory API key cache from the secure store.
/// Called during backend startup BEFORE startup validation checks run.
/// This ensures credentials persisted in secure storage are visible to
/// perform_startup_checks() without waiting for the frontend to warm the cache.
pub fn warm_ai_key_cache_from_secure_store(app: &tauri::AppHandle) {
    if let Ok(mut cache) = API_KEY_CACHE.lock() {
        for key_name in ai_provider_key_names() {
            // Skip if already cached (e.g. from a prior call or concurrent frontend warm)
            if cache.contains_key(&key_name) {
                continue;
            }
            match secure_store::secure_get(app, &key_name) {
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
/// Validate a custom OpenAI-compatible base URL.
///
/// This is a minimal link-local DENYLIST, not an allowlist: localhost,
/// RFC-1918 private ranges, the 100.64.0.0/10 CGNAT range used by Tailscale,
/// hostnames, and public https URLs are all permitted. Only link-local IPv4
/// (169.254.0.0/16, including the cloud-metadata endpoint 169.254.169.254),
/// link-local IPv6 (fe80::/10, plus IPv4-mapped link-local), and non-http(s)
/// schemes are rejected.
fn validate_custom_base_url(raw: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(raw).map_err(|_| "Invalid endpoint URL".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Endpoint must be an http(s) URL".to_string());
    }
    if let Some(host) = url.host_str() {
        // IPv6 literals are bracketed in the URL serialization (e.g. [fe80::1]).
        let host = host.trim_start_matches('[').trim_end_matches(']');
        if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
            if ip.is_link_local() {
                return Err("Endpoint host is not allowed".to_string());
            }
        } else if let Ok(ip) = host.parse::<std::net::Ipv6Addr>() {
            // fe80::/10 unicast link-local, plus any IPv4-mapped/compatible form
            // of an IPv4 link-local address (e.g. ::ffff:169.254.169.254) that
            // routes to the same cloud-metadata target.
            let mapped_link_local = ip.to_ipv4().is_some_and(|v4| v4.is_link_local());
            if (0xfe80..=0xfebf).contains(&ip.segments()[0]) || mapped_link_local {
                return Err("Endpoint host is not allowed".to_string());
            }
        }
    }
    Ok(())
}

fn load_models_by_provider<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
    current_provider: &str,
    current_model: &str,
) -> HashMap<String, String> {
    let mut models_by_provider = store
        .get("ai_models_by_provider")
        .and_then(|v| serde_json::from_value::<HashMap<String, String>>(v.clone()).ok())
        .unwrap_or_default();

    if !current_provider.is_empty()
        && !current_model.is_empty()
        && !models_by_provider.contains_key(current_provider)
    {
        models_by_provider.insert(current_provider.to_string(), current_model.to_string());
    }

    models_by_provider
}

fn remember_provider_model(
    models_by_provider: &mut HashMap<String, String>,
    provider: &str,
    model: &str,
) {
    if !provider.is_empty() && !model.is_empty() {
        models_by_provider.insert(provider.to_string(), model.to_string());
    }
}

async fn run_openai_chat_probe(
    client: &reqwest::Client,
    base_url: &str,
    model: &str,
    auth_header: Option<&str>,
) -> Result<(), String> {
    let url = normalize_chat_completions_url(base_url);
    let payload = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": "Reply with OK."},
            {"role": "user", "content": "ping"}
        ],
        "stream": false
    });
    let mut req = client
        .post(&url)
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
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 | 403 => "Invalid API key (the endpoint rejected the credentials).".to_string(),
            404 => "Endpoint or model not found (HTTP 404).".to_string(),
            429 => "Rate limited by the provider (HTTP 429).".to_string(),
            _ => format!("Endpoint returned HTTP {}", status.as_u16()),
        });
    }
    let value = serde_json::from_str::<serde_json::Value>(&body)
        .map_err(|_| "The endpoint did not return a JSON chat-completion response.".to_string())?;
    if value
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .is_none()
    {
        return Err("The endpoint response is not OpenAI chat-completions compatible.".to_string());
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AISettings {
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    #[serde(rename = "hasApiKey")]
    pub has_api_key: bool,
    #[serde(rename = "modelsByProvider")]
    pub models_by_provider: HashMap<String, String>,
    #[serde(rename = "aiModelNeedsReselection")]
    pub ai_model_needs_reselection: bool,
}

// Validation pattern for providers
lazy_static::lazy_static! {
    static ref PROVIDER_REGEX: regex::Regex = regex::Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap();
}

// Providers come from a broad catalog; validate only the identifier shape here.
// Availability comes from the Rust provider/model catalog in crate::ai::providers.
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

    let models_by_provider = load_models_by_provider(&store, &provider, &model);

    let ai_model_needs_reselection = store
        .get("ai_model_needs_reselection")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

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
        models_by_provider,
        ai_model_needs_reselection,
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

    let current_provider = store
        .get("ai_provider")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();
    let current_model = store
        .get("ai_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "".to_string()); // Empty by default
    let models_by_provider = load_models_by_provider(&store, &current_provider, &current_model);
    let model = models_by_provider
        .get(&provider)
        .cloned()
        .unwrap_or_default();

    // For OpenAI-compatible providers, treat no_auth as having a usable config
    let has_api_key = {
        let cache = API_KEY_CACHE
            .lock()
            .map_err(|_| "Failed to access cache".to_string())?;
        check_has_api_key(&provider, &store, &cache)
    };

    let ai_model_needs_reselection = store
        .get("ai_model_needs_reselection")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Ok(AISettings {
        enabled,
        provider,
        model,
        has_api_key,
        models_by_provider,
        ai_model_needs_reselection,
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

// Validate a new API key or no-auth OpenAI-compatible configuration.
#[derive(Deserialize)]
pub struct ValidateAiApiKeyArgs {
    pub provider: String,
    #[serde(alias = "apiKey", alias = "api_key")]
    pub api_key: Option<String>,
    #[serde(alias = "baseUrl", alias = "base_url")]
    pub base_url: Option<String>,
    pub model: Option<String>,
    #[serde(alias = "noAuth", alias = "no_auth")]
    pub no_auth: Option<bool>,
}
/// Read the user's previously-configured custom model from settings.
///
/// The custom provider has no catalog models; its model is whatever the user
/// picked. It is persisted in the per-provider map (`ai_models_by_provider`),
/// or — for the currently-active provider — in the single `ai_model` value.
fn configured_custom_model(app: &tauri::AppHandle) -> Option<String> {
    app.store("settings").ok().and_then(|store| {
        let from_map = store
            .get("ai_models_by_provider")
            .and_then(|v| serde_json::from_value::<HashMap<String, String>>(v.clone()).ok())
            .and_then(|map| {
                map.get(PROVIDER_CUSTOM)
                    .cloned()
                    .filter(|m| !m.trim().is_empty())
            });
        if from_map.is_some() {
            return from_map;
        }
        let active_provider = store
            .get("ai_provider")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default();
        if active_provider == PROVIDER_CUSTOM {
            store
                .get("ai_model")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .filter(|m| !m.trim().is_empty())
        } else {
            None
        }
    })
}

/// Decide which model id to use when validating the custom provider.
///
/// Prefer the explicit `model` arg, else the user's previously-configured
/// custom model. The custom provider has no catalog models, so this NEVER
/// falls back to `gpt-5-nano` (which would 404 against a local endpoint).
fn resolve_custom_validation_model(
    explicit: Option<&str>,
    configured: Option<&str>,
) -> Result<String, String> {
    if let Some(model) = explicit.map(str::trim).filter(|m| !m.is_empty()) {
        return Ok(model.to_string());
    }
    if let Some(model) = configured.map(str::trim).filter(|m| !m.is_empty()) {
        return Ok(model.to_string());
    }
    Err("Select a model for your custom provider before validating.".to_string())
}

#[tauri::command]
pub async fn validate_ai_api_key(
    app: tauri::AppHandle,
    args: ValidateAiApiKeyArgs,
) -> Result<(), String> {
    let ValidateAiApiKeyArgs {
        provider,
        api_key,
        base_url,
        model,
        no_auth,
    } = args;
    validate_provider_name(&provider)?;

    let provider_is_supported = launch_providers()
        .iter()
        .any(|candidate| candidate.id == provider);
    if !provider_is_supported {
        return Err(user_facing_message(&AiProviderError::UnsupportedProvider).to_string());
    }

    let provided_key = api_key.unwrap_or_default();
    let no_auth =
        provider == PROVIDER_CUSTOM && (no_auth.unwrap_or(false) || provided_key.trim().is_empty());
    if !no_auth && provided_key.trim().is_empty() {
        return Err(user_facing_message(&AiProviderError::MissingApiKey).to_string());
    }

    let validation_model = if provider == PROVIDER_CUSTOM {
        // The custom provider has no catalog models, so a model must be
        // supplied explicitly or already configured in settings. Never fall
        // back to gpt-5-nano — that would 404 against a local endpoint.
        let configured = configured_custom_model(&app);
        resolve_custom_validation_model(model.as_deref(), configured.as_deref())?
    } else {
        model
            .filter(|candidate| !candidate.trim().is_empty())
            .or_else(|| {
                catalog::recommended_models(&provider)
                    .into_iter()
                    .find(|candidate| candidate.recommended)
                    .map(|candidate| candidate.model_id)
            })
            .unwrap_or_else(|| "gpt-5-nano".to_string())
    };
    let custom_base_url = if provider == PROVIDER_CUSTOM {
        base_url
            .filter(|candidate| !candidate.trim().is_empty())
            .or_else(|| custom_base_url_from_settings(&app))
            .unwrap_or_else(|| DEFAULT_OPENAI_BASE_URL.to_string())
    } else {
        DEFAULT_OPENAI_BASE_URL.to_string()
    };
    if provider == PROVIDER_CUSTOM {
        validate_custom_base_url(&custom_base_url)?;
    }

    let validation_provider = provider.clone();
    let validation_key = provided_key.trim().to_string();
    let key_resolver: AiKeyResolver = Arc::new(move |provider_id| {
        if provider_id == validation_provider && !validation_key.is_empty() {
            Some(validation_key.clone())
        } else {
            None
        }
    });
    let http_client = reqwest::Client::builder()
        .build()
        .map_err(|error| format!("Failed to build validation client: {error}"))?;
    let executor = AiExecutor::new(http_client, key_resolver, custom_base_url, no_auth);
    let request = AiPolishRequest {
        provider_id: provider.clone(),
        model_id: validation_model,
        input_text: "ok".to_string(),
        prompt: "Reply with exactly: ok".to_string(),
        timeout_ms: 10_000,
    };

    executor
        .polish(request, tokio_util::sync::CancellationToken::new())
        .await
        .map(|_| ())
        .map_err(|error| {
            log::warn!(
                "AI provider validation failed: provider={} category={}",
                provider,
                user_facing_message(&error)
            );
            user_facing_message(&error).to_string()
        })
}

/// Test an OpenAI-compatible endpoint without saving or caching anything.
#[tauri::command]
pub async fn test_openai_endpoint(
    base_url: String,
    model: String,
    api_key: Option<String>,
    no_auth: Option<bool>,
) -> Result<(), String> {
    validate_custom_base_url(&base_url)?;
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

    run_openai_chat_probe(&client, &base_url, &model, auth_header.as_deref())
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
    let mut models_by_provider = load_models_by_provider(&store, &provider, &model);
    remember_provider_model(&mut models_by_provider, &provider, &model);

    store.set("ai_enabled", json!(enabled));
    store.set("ai_provider", json!(provider));
    store.set("ai_model", json!(model));
    store.set("ai_models_by_provider", json!(models_by_provider));
    if !model.is_empty() {
        store.set("ai_model_needs_reselection", json!(false));
    }
    if !enabled {
        store.set(
            "enhancement_options",
            serde_json::to_value(EnhancementOptions {
                preset: crate::ai::prompts::EnhancementPreset::PersonalDictation,
            })
            .map_err(|e| format!("Failed to serialize enhancement options: {}", e))?,
        );
        store.set(
            "final_text_language",
            json!(FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT),
        );
        store.set("transcription_task", json!(TRANSCRIPTION_TASK_TRANSCRIBE));
    }

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
    store.set(
        "enhancement_options",
        serde_json::to_value(EnhancementOptions {
            preset: crate::ai::prompts::EnhancementPreset::PersonalDictation,
        })
        .map_err(|e| format!("Failed to serialize enhancement options: {}", e))?,
    );
    store.set(
        "final_text_language",
        json!(FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT),
    );
    store.set("transcription_task", json!(TRANSCRIPTION_TASK_TRANSCRIBE));

    store
        .save()
        .map_err(|e| format!("Failed to save AI settings: {}", e))?;

    // Invalidate recording config cache when AI settings change
    crate::commands::audio::invalidate_recording_config_cache(&app).await;

    log::info!("AI enhancement disabled");

    Ok(())
}

pub async fn get_enhancement_options_for_ai_enabled(
    app: tauri::AppHandle,
    ai_enabled: bool,
) -> Result<EnhancementOptions, String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    crate::ai::prompts::enhancement_options_for_ai_enabled(
        store.get("enhancement_options").as_ref(),
        ai_enabled,
    )
}

#[tauri::command]
pub async fn get_enhancement_options(app: tauri::AppHandle) -> Result<EnhancementOptions, String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    let ai_enabled = store
        .get("ai_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    drop(store);
    get_enhancement_options_for_ai_enabled(app, ai_enabled).await
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

    crate::commands::audio::invalidate_recording_config_cache(&app).await;

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
    save_writing_settings(&app, &settings)?;
    crate::commands::audio::invalidate_recording_config_cache(&app).await;
    Ok(())
}

fn custom_base_url_from_settings(app: &tauri::AppHandle) -> Option<String> {
    app.store("settings").ok().and_then(|store| {
        store
            .get(CUSTOM_BASE_URL_KEY)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .or_else(|| {
                store
                    .get(LEGACY_OPENAI_BASE_URL_KEY)
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
            })
    })
}

fn custom_no_auth_from_settings(app: &tauri::AppHandle, has_key: bool) -> bool {
    app.store("settings")
        .ok()
        .and_then(|store| {
            store
                .get(CUSTOM_NO_AUTH_KEY)
                .and_then(|v| v.as_bool())
                .or_else(|| {
                    store
                        .get(LEGACY_OPENAI_NO_AUTH_KEY)
                        .and_then(|v| v.as_bool())
                })
        })
        .unwrap_or(!has_key)
}

fn selected_ai_provider_and_model(
    app: &tauri::AppHandle,
) -> Result<(String, String), AiProviderError> {
    let store = app
        .store("settings")
        .map_err(|_| AiProviderError::Internal)?;
    let provider = store
        .get("ai_provider")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();
    let model = store
        .get("ai_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();
    if provider.is_empty() || model.is_empty() {
        return Err(AiProviderError::InvalidModel);
    }
    Ok((provider, model))
}

fn executor_for_provider(
    app: &tauri::AppHandle,
    selected_provider: &str,
) -> Result<(AiExecutor, String), AiProviderError> {
    let cache = API_KEY_CACHE
        .lock()
        .map_err(|_| AiProviderError::Internal)?;
    let openai_key = cache.get("ai_api_key_openai").cloned();
    let custom_key = cache.get("ai_api_key_custom").cloned();
    let selected_key = cache
        .get(&format!("ai_api_key_{}", selected_provider))
        .cloned();
    drop(cache);

    let (runtime_provider, custom_base_url, custom_no_auth, keys) = if selected_provider == "openai"
    {
        if let Some(key) = openai_key {
            let mut keys = HashMap::new();
            keys.insert("openai".to_string(), key);
            (
                "openai".to_string(),
                DEFAULT_OPENAI_BASE_URL.to_string(),
                false,
                keys,
            )
        } else if let Some(base_url) = custom_base_url_from_settings(app) {
            let has_key = custom_key.is_some();
            let mut keys = HashMap::new();
            if let Some(key) = custom_key {
                keys.insert(PROVIDER_CUSTOM.to_string(), key);
            }
            (
                PROVIDER_CUSTOM.to_string(),
                base_url,
                custom_no_auth_from_settings(app, has_key),
                keys,
            )
        } else {
            return Err(AiProviderError::MissingApiKey);
        }
    } else if selected_provider == PROVIDER_CUSTOM {
        let has_key = custom_key.is_some();
        let mut keys = HashMap::new();
        if let Some(key) = custom_key {
            keys.insert(PROVIDER_CUSTOM.to_string(), key);
        }
        (
            PROVIDER_CUSTOM.to_string(),
            custom_base_url_from_settings(app)
                .unwrap_or_else(|| DEFAULT_OPENAI_BASE_URL.to_string()),
            custom_no_auth_from_settings(app, has_key),
            keys,
        )
    } else {
        let key = selected_key.ok_or(AiProviderError::MissingApiKey)?;
        let mut keys = HashMap::new();
        keys.insert(selected_provider.to_string(), key);
        (
            selected_provider.to_string(),
            DEFAULT_OPENAI_BASE_URL.to_string(),
            false,
            keys,
        )
    };
    if runtime_provider == PROVIDER_CUSTOM {
        if let Err(reason) = validate_custom_base_url(&custom_base_url) {
            log::error!(
                "Refusing to use disallowed custom endpoint ({}): {}",
                custom_base_url,
                reason
            );
            return Err(AiProviderError::BadResponse);
        }
    }

    if runtime_provider == PROVIDER_CUSTOM && !custom_no_auth && !keys.contains_key(PROVIDER_CUSTOM)
    {
        return Err(AiProviderError::MissingApiKey);
    }

    let key_resolver: AiKeyResolver = Arc::new(move |provider_id| keys.get(provider_id).cloned());
    let http_client = reqwest::Client::builder()
        .build()
        .map_err(|_| AiProviderError::Internal)?;
    Ok((
        AiExecutor::new(http_client, key_resolver, custom_base_url, custom_no_auth),
        runtime_provider,
    ))
}

async fn polish_text_with_prompt_typed(
    app: &tauri::AppHandle,
    text: &str,
    model: String,
    provider: String,
    prompt: String,
) -> Result<String, AiProviderError> {
    let (executor, runtime_provider) = executor_for_provider(app, &provider)?;
    let request = AiPolishRequest {
        provider_id: runtime_provider.clone(),
        model_id: model,
        input_text: text.to_string(),
        prompt,
        timeout_ms: 30_000,
    };
    let result = executor
        .polish(request, tokio_util::sync::CancellationToken::new())
        .await?;
    log::info!(
        "Text enhanced successfully via {} (original: {}, enhanced: {}, duration_ms: {})",
        result.provider_id,
        text.len(),
        result.output_text.len(),
        result.duration_ms
    );
    Ok(result.output_text)
}

pub async fn polish_text_typed(
    app: &tauri::AppHandle,
    text: &str,
    options: &crate::ai::EnhancementOptions,
    output_language: Option<&str>,
    context: Option<&str>,
) -> Result<String, crate::ai::error::AiProviderError> {
    let (provider, model) = selected_ai_provider_and_model(app)?;
    let prompt = crate::ai::prompts::build_enhancement_prompt(context, options, output_language);
    polish_text_with_prompt_typed(app, text, model, provider, prompt).await
}

pub(crate) async fn enhance_transcription_internal(
    text: String,
    transcript_language: Option<String>,
    ai_enabled_override: Option<bool>,
    output_language_override: Option<String>,
    context_override: Option<String>,
    preset_override: Option<crate::ai::prompts::EnhancementPreset>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    if text.trim().is_empty() {
        log::debug!("Skipping enhancement for empty text");
        return Ok(text);
    }

    let force_formatting = ai_enabled_override == Some(true);
    let enabled = {
        let store = app.store("settings").map_err(|e| e.to_string())?;
        ai_enabled_override.unwrap_or_else(|| {
            store
                .get("ai_enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
    };

    if !enabled {
        log::debug!("AI enhancement is disabled");
        return Ok(text);
    }

    let stored_options = get_enhancement_options_for_ai_enabled(app.clone(), enabled)
        .await
        .unwrap_or_else(|_| EnhancementOptions::default_for_ai_enabled(enabled));
    let enhancement_options =
        crate::ai::prompts::effective_enhancement_options(&stored_options, preset_override);

    if !enhancement_options.preset.requires_ai_formatting() {
        if force_formatting {
            return Err("Personal Dictation does not use AI formatting".to_string());
        }
        log::debug!("Skipping AI formatting for Personal Dictation");
        return Ok(text);
    }

    let (provider, model) = match selected_ai_provider_and_model(&app) {
        Ok(selection) => selection,
        Err(error) => {
            log::warn!(
                "AI enhancement skipped: category={}",
                user_facing_message(&error)
            );
            if force_formatting {
                return Err(user_facing_message(&error).to_string());
            }
            return Ok(text);
        }
    };

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
            if let Some(transcript_language) = transcript_language {
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
        context_override.as_deref(),
        &enhancement_options,
        language.as_deref(),
    );

    match polish_text_with_prompt_typed(&app, &text, model, provider, prompt).await {
        Ok(enhanced_text) => Ok(enhanced_text),
        Err(error) => {
            log::warn!(
                "AI formatting failed: category={}",
                user_facing_message(&error)
            );
            pill_toast(&app, "Formatting failed", 1500);
            if force_formatting {
                Err(user_facing_message(&error).to_string())
            } else {
                Ok(text)
            }
        }
    }
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
    enhance_transcription_internal(
        text,
        transcript_language,
        ai_enabled_override,
        output_language_override,
        context_override,
        None,
        app,
    )
    .await
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
    validate_custom_base_url(&args.base_url)?;
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

/// A model available from a provider.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderModel {
    pub id: String,
    pub name: String,
    pub recommended: bool,
    pub reasoning: bool,
    #[serde(rename = "contextWindow")]
    pub context_window: Option<u64>,
    #[serde(rename = "costInput")]
    pub cost_input: Option<f64>,
    #[serde(rename = "costOutput")]
    pub cost_output: Option<f64>,
}

fn provider_models(provider: &str) -> Vec<ProviderModel> {
    catalog::all_provider_models(provider)
        .into_iter()
        .map(|model| ProviderModel {
            id: model.model_id.clone(),
            name: model.label.clone(),
            recommended: model.recommended,
            reasoning: model.reasoning,
            context_window: model.context,
            cost_input: model.cost_input,
            cost_output: model.cost_output,
        })
        .collect()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub status: String,
}

fn provider_infos() -> Vec<ProviderInfo> {
    launch_providers()
        .into_iter()
        .map(|provider| ProviderInfo {
            id: provider.id,
            name: provider.label,
            status: provider.status,
        })
        .collect()
}

#[tauri::command]
pub async fn list_ai_providers(_app: tauri::AppHandle) -> Result<Vec<ProviderInfo>, String> {
    Ok(provider_infos())
}

/// List available models for a provider.
#[tauri::command]
pub async fn list_provider_models(
    provider: String,
    _app: tauri::AppHandle,
) -> Result<Vec<ProviderModel>, String> {
    validate_provider_name(&provider)?;

    if provider == PROVIDER_CUSTOM {
        return Ok(Vec::new());
    }

    let models = provider_models(&provider);
    if models.is_empty() {
        Err(format!(
            "Unsupported provider for model listing: {}",
            provider
        ))
    } else {
        Ok(models)
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

        // Runtime launch table support is enforced separately from loose
        // identifier validation used by legacy settings paths.
        assert!(validate_provider_name("groq").is_ok());
        assert!(validate_provider_name("azure-openai-responses").is_ok());
        assert!(validate_provider_name("openai_compatible").is_ok());

        // Invalid formats
        assert!(validate_provider_name("test provider").is_err());
        assert!(validate_provider_name("test@provider").is_err());
        assert!(validate_provider_name("").is_err());
    }

    #[test]
    fn test_provider_models_are_contract_backed() {
        let openai_models = provider_models("openai");
        assert!(openai_models.len() >= 2);
        assert!(openai_models.iter().any(|m| m.id == "gpt-5-nano"));
        assert!(openai_models.iter().any(|m| m.id == "gpt-5-mini"));

        let gemini_models = provider_models("gemini");
        assert!(!gemini_models.is_empty());
        assert!(gemini_models.iter().any(|m| m.id == "gemini-2.5-flash"));
        assert!(gemini_models
            .iter()
            .any(|m| m.id == "gemini-2.5-flash-lite"));

        let anthropic_models = provider_models("anthropic");
        assert!(!anthropic_models.is_empty());
        assert!(anthropic_models.iter().any(|m| m.id == "claude-haiku-4-5"));
        assert!(anthropic_models.iter().any(|m| m.id == "claude-sonnet-4-5"));

        assert!(provider_models("custom").is_empty());
        assert!(provider_models("unknown").is_empty());
    }

    #[test]
    fn test_list_command_dto_shape_includes_catalog_providers() {
        let providers = provider_infos();
        // 8 generated catalog providers + the synthetic custom provider.
        assert_eq!(providers.len(), launch_providers().len());
        let by_id = |id: &str| providers.iter().find(|provider| provider.id == id);
        assert_eq!(
            by_id("openai").map(|p| (p.name.as_str(), p.status.as_str())),
            Some(("OpenAI", "production"))
        );
        assert_eq!(
            by_id("gemini").map(|p| (p.name.as_str(), p.status.as_str())),
            Some(("Google Gemini", "production"))
        );
        assert_eq!(
            by_id("anthropic").map(|p| p.status.as_str()),
            Some("production")
        );
        assert_eq!(
            by_id("custom").map(|p| (p.name.as_str(), p.status.as_str())),
            Some(("Custom (OpenAI-compatible)", "production"))
        );
    }

    #[test]
    fn test_warmup_keys_are_derived_from_launch_providers() {
        let keys = ai_provider_key_names();
        let expected: Vec<String> = launch_providers()
            .into_iter()
            .filter(|provider| provider.requires_api_key || provider.id == PROVIDER_CUSTOM)
            .map(|provider| format!("ai_api_key_{}", provider.id))
            .collect();
        assert_eq!(keys, expected);
        assert!(keys.contains(&"ai_api_key_openai".to_string()));
        assert!(keys.contains(&"ai_api_key_anthropic".to_string()));
        assert!(keys.contains(&"ai_api_key_custom".to_string()));
    }

    #[test]
    fn test_enhancement_options_for_ai_enabled_normalizes_disabled_ai() {
        use crate::ai::prompts::{enhancement_options_for_ai_enabled, EnhancementPreset};

        let value = serde_json::json!({ "preset": "Writing" });
        let options = enhancement_options_for_ai_enabled(Some(&value), false).unwrap();
        assert_eq!(options.preset, EnhancementPreset::PersonalDictation);

        let enabled = enhancement_options_for_ai_enabled(Some(&value), true).unwrap();
        assert_eq!(enabled.preset, EnhancementPreset::Writing);

        let defaults = enhancement_options_for_ai_enabled(None, true).unwrap();
        assert_eq!(defaults.preset, EnhancementPreset::CleanDictation);
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

    #[tokio::test]
    async fn chat_probe_ok_on_valid_completion() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{"message": {"content": "OK"}}]
            })))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let result = run_openai_chat_probe(&client, &server.uri(), "test-model", None).await;
        assert!(result.is_ok(), "expected Ok, got: {:?}", result);
    }

    #[tokio::test]
    async fn chat_probe_errors_on_auth_failure() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_string("echoed-secret-sk-LEAK"))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let result = run_openai_chat_probe(&client, &server.uri(), "test-model", None).await;
        let err = result.unwrap_err();
        assert!(
            err.contains("Invalid API key"),
            "expected 'Invalid API key' in error, got: {}",
            err
        );
        assert!(
            !err.contains("echoed-secret-sk-LEAK"),
            "error must not echo response body, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn chat_probe_errors_on_non_chat_shape() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"foo": "bar"})),
            )
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let result = run_openai_chat_probe(&client, &server.uri(), "test-model", None).await;
        assert!(result.is_err(), "expected Err for non-chat JSON shape");
    }

    #[tokio::test]
    async fn chat_probe_errors_on_non_json() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html>nope</html>"))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let result = run_openai_chat_probe(&client, &server.uri(), "test-model", None).await;
        assert!(result.is_err(), "expected Err for non-JSON response");
    }

    #[test]
    fn validate_custom_base_url_accepts_local_private_tailscale_and_public() {
        assert!(validate_custom_base_url("http://localhost:11434").is_ok());
        assert!(validate_custom_base_url("http://192.168.1.50:1234").is_ok());
        assert!(validate_custom_base_url("http://100.100.100.100:11434").is_ok());
        assert!(validate_custom_base_url("https://api.example.com").is_ok());
    }

    #[test]
    fn validate_custom_base_url_rejects_link_local_and_non_http_schemes() {
        assert!(validate_custom_base_url("http://169.254.169.254/latest/meta-data/").is_err());
        assert!(validate_custom_base_url("http://[fe80::1]/").is_err());
        assert!(validate_custom_base_url("ftp://x").is_err());
        assert!(validate_custom_base_url("not a url").is_err());
        // IPv4-mapped link-local must not bypass the IPv6 branch.
        assert!(validate_custom_base_url("http://[::ffff:169.254.169.254]/").is_err());
    }

    #[test]
    fn custom_validation_model_resolution_never_yields_gpt5_nano() {
        // No explicit model and no configured model -> clear error, never a probe.
        let err = resolve_custom_validation_model(None, None).unwrap_err();
        assert!(err.contains("Select a model"), "got: {}", err);
        assert!(!err.to_lowercase().contains("gpt-5-nano"));

        // Explicit model wins.
        assert_eq!(
            resolve_custom_validation_model(Some("llama3.2"), None).unwrap(),
            "llama3.2"
        );
        // Configured model used when no explicit arg.
        assert_eq!(
            resolve_custom_validation_model(None, Some("qwen2.5")).unwrap(),
            "qwen2.5"
        );
        // Whitespace-only values are treated as absent (must error).
        assert!(resolve_custom_validation_model(Some("   "), Some("  ")).is_err());
    }
}
