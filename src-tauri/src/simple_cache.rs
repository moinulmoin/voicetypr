use chrono::{DateTime, Utc};
use serde_json::Value;
use std::sync::Arc;
use tauri_plugin_store::StoreExt;

// Minimal replacement for tauri-plugin-cache with TTL support using tauri-plugin-store

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub struct SetItemOptions {
    pub ttl: Option<u64>,                   // seconds
    pub compress: Option<bool>,             // ignored
    pub compression_method: Option<String>, // ignored
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CacheEnvelope {
    #[serde(default)]
    __meta: Meta,
    data: Value,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Meta {
    #[serde(default)]
    cached_at: Option<DateTime<Utc>>, // RFC3339
    #[serde(default)]
    ttl: Option<u64>, // seconds
}

fn open_store<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<Arc<tauri_plugin_store::Store<R>>, String> {
    app.store("cache").map_err(|e| e.to_string())
}

pub fn get<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    key: &str,
) -> Result<Option<Value>, String> {
    let store = open_store(app)?;
    if let Some(v) = store.get(key) {
        // Try to interpret as envelope first
        if let Ok(env) = serde_json::from_value::<CacheEnvelope>(v.clone()) {
            if let Some(ttl) = env.__meta.ttl {
                if let Some(cached_at) = env.__meta.cached_at {
                    let age = Utc::now().signed_duration_since(cached_at).num_seconds();
                    if age >= ttl as i64 {
                        // Expired – delete and return None
                        let store = open_store(app)?;
                        store.delete(key);
                        let _ = store.save();
                        return Ok(None);
                    }
                }
            }
            return Ok(Some(env.data));
        }
        // Fallback: legacy raw value
        return Ok(Some(v));
    }
    Ok(None)
}

pub fn set<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    key: &str,
    value: Value,
    options: Option<SetItemOptions>,
) -> Result<(), String> {
    let opts = options.unwrap_or_default();
    let env = CacheEnvelope {
        __meta: Meta {
            cached_at: Some(Utc::now()),
            ttl: opts.ttl,
        },
        data: value,
    };
    let store = open_store(app)?;
    store.set(key, serde_json::to_value(env).unwrap_or(Value::Null));
    store.save().map_err(|e| e.to_string())
}

pub fn remove<R: tauri::Runtime>(app: &tauri::AppHandle<R>, key: &str) -> Result<(), String> {
    let store = open_store(app)?;
    store.delete(key);
    store.save().map_err(|e| e.to_string())
}
