/// Helper utilities for consistent and correct Sentry error tracking
use tauri_plugin_sentry::sentry;
use std::path::Path;

/// Capture a Sentry message with tags in a thread-safe way
#[macro_export]
macro_rules! capture_sentry_message {
    ($msg:expr, $level:expr, tags: { $($key:expr => $value:expr),* $(,)? }) => {
        tauri_plugin_sentry::sentry::with_scope(
            |scope| {
                $(
                    scope.set_tag($key, $value);
                )*
            },
            || {
                tauri_plugin_sentry::sentry::capture_message($msg, $level);
            }
        );
    };
}

/// Capture a Sentry message with tags and context
#[macro_export]
macro_rules! capture_sentry_with_context {
    (
        $msg:expr, 
        $level:expr, 
        tags: { $($key:expr => $value:expr),* $(,)? },
        context: $ctx_name:expr, $ctx_value:expr
    ) => {
        tauri_plugin_sentry::sentry::with_scope(
            |scope| {
                $(
                    scope.set_tag($key, $value);
                )*
                scope.set_context($ctx_name, $ctx_value);
            },
            || {
                tauri_plugin_sentry::sentry::capture_message($msg, $level);
            }
        );
    };
}

/// Sanitize file paths to remove sensitive information
pub fn sanitize_path(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Create a safe file context for Sentry without exposing full paths
pub fn create_safe_file_context(
    path: &Path,
    size_bytes: Option<u64>,
    exists: bool,
) -> sentry::protocol::Context {
    let mut map = std::collections::BTreeMap::new();
    
    map.insert(
        "file_name".to_string(), 
        serde_json::Value::from(sanitize_path(path))
    );
    
    map.insert(
        "exists".to_string(),
        serde_json::Value::from(exists)
    );
    
    if let Some(size) = size_bytes {
        map.insert(
            "size_bytes".to_string(),
            serde_json::Value::from(size)
        );
    }
    
    sentry::protocol::Context::Other(map)
}

/// Create a context from a BTreeMap (helper for complex contexts)
pub fn create_context_from_map(
    data: std::collections::BTreeMap<String, serde_json::Value>
) -> sentry::protocol::Context {
    sentry::protocol::Context::Other(data)
}