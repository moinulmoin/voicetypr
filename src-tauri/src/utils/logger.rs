use std::collections::HashMap;
use std::time::Instant;

/// Structured logging utilities for VoiceTypr debugging
/// 
/// This module provides consistent logging patterns across the application
/// to help diagnose production issues, performance bottlenecks, and silent failures.

/// Log function entry and exit with automatic timing
pub fn log_function<F, R>(name: &str, f: F) -> R 
where 
    F: FnOnce() -> R 
{
    let start = Instant::now();
    log::info!("→ {} START", name);
    let result = f();
    log::info!("← {} END ({}ms)", name, start.elapsed().as_millis());
    result
}

/// Log function entry and exit with context data
pub fn log_function_with_context<F, R>(
    name: &str, 
    context: &HashMap<String, String>, 
    f: F
) -> R 
where 
    F: FnOnce() -> R 
{
    let start = Instant::now();
    log::info!("→ {} START - Context: {:?}", name, context);
    let result = f();
    log::info!("← {} END ({}ms) - Context: {:?}", name, start.elapsed().as_millis(), context);
    result
}

/// Log async function with timing
pub async fn log_async_function<F, Fut, R>(name: &str, f: F) -> R
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = R>,
{
    let start = Instant::now();
    log::info!("→ {} START (async)", name);
    let result = f().await;
    log::info!("← {} END ({}ms) (async)", name, start.elapsed().as_millis());
    result
}

/// Log error with comprehensive context information
pub fn log_error_with_context(
    error: &str,
    context: &HashMap<String, String>
) {
    log::error!("❌ ERROR: {}", error);
    if !context.is_empty() {
        log::error!("   📋 Context: {:?}", context);
    }
}

/// Log performance metrics for operations
pub fn log_performance(
    operation: &str,
    duration_ms: u64,
    metadata: Option<&str>
) {
    let metadata_str = metadata.unwrap_or("");
    log::info!("⚡ PERF: {} took {}ms {}", operation, duration_ms, metadata_str);
}

/// Log performance with additional metrics
pub fn log_performance_detailed(
    operation: &str,
    duration_ms: u64,
    metrics: &HashMap<String, String>
) {
    log::info!("⚡ PERF: {} took {}ms", operation, duration_ms);
    if !metrics.is_empty() {
        log::info!("   📊 Metrics: {:?}", metrics);
    }
}

/// Log operation start with parameters
pub fn log_operation_start(operation: &str, params: &HashMap<String, String>) {
    log::info!("🚀 {} STARTING", operation);
    if !params.is_empty() {
        log::info!("   📝 Parameters: {:?}", params);
    }
}

/// Log operation completion with results
pub fn log_operation_complete(operation: &str, duration_ms: u64, results: &HashMap<String, String>) {
    log::info!("✅ {} COMPLETED ({}ms)", operation, duration_ms);
    if !results.is_empty() {
        log::info!("   📋 Results: {:?}", results);
    }
}

/// Log operation failure with context
pub fn log_operation_failed(operation: &str, error: &str, context: &HashMap<String, String>) {
    log::error!("❌ {} FAILED: {}", operation, error);
    if !context.is_empty() {
        log::error!("   📋 Context: {:?}", context);
    }
}

/// Log audio metrics in a structured way
pub fn log_audio_metrics(
    operation: &str,
    energy: f64,
    peak: f64,
    duration: f32,
    additional: Option<&HashMap<String, String>>
) {
    log::info!("🔊 AUDIO {}: energy={:.4}, peak={:.4}, duration={:.2}s", 
        operation, energy, peak, duration);
    if let Some(extra) = additional {
        if !extra.is_empty() {
            log::info!("   📊 Audio Metrics: {:?}", extra);
        }
    }
}

/// Log model operations with metadata
pub fn log_model_operation(
    operation: &str,
    model_name: &str,
    status: &str,
    metadata: Option<&HashMap<String, String>>
) {
    log::info!("🤖 MODEL {} - {}: {}", operation, model_name, status);
    if let Some(meta) = metadata {
        if !meta.is_empty() {
            log::info!("   📋 Model Info: {:?}", meta);
        }
    }
}

/// Log system resource usage
pub fn log_system_resources(
    context: &str,
    memory_mb: Option<u64>,
    cpu_percent: Option<f32>,
    additional: Option<&HashMap<String, String>>
) {
    let mut parts = vec![format!("💻 SYSTEM [{}]", context)];
    
    if let Some(mem) = memory_mb {
        parts.push(format!("memory={}MB", mem));
    }
    if let Some(cpu) = cpu_percent {
        parts.push(format!("cpu={:.1}%", cpu));
    }
    
    log::info!("{}", parts.join(" - "));
    
    if let Some(extra) = additional {
        if !extra.is_empty() {
            log::info!("   📊 System Info: {:?}", extra);
        }
    }
}

/// Log state transitions with validation
pub fn log_state_transition(
    component: &str,
    from_state: &str,
    to_state: &str,
    valid: bool,
    context: Option<&HashMap<String, String>>
) {
    let status = if valid { "✅ VALID" } else { "⚠️  INVALID" };
    log::info!("🔄 STATE [{}]: {} → {} ({})", component, from_state, to_state, status);
    
    if let Some(ctx) = context {
        if !ctx.is_empty() {
            log::info!("   📋 State Context: {:?}", ctx);
        }
    }
}

/// Log GPU/Hardware information
pub fn log_hardware_info(
    component: &str,
    info: &HashMap<String, String>
) {
    log::info!("🎮 HARDWARE [{}]", component);
    if !info.is_empty() {
        log::info!("   📊 Hardware Info: {:?}", info);
    }
}

/// Log file operations with details
pub fn log_file_operation(
    operation: &str,
    path: &str,
    success: bool,
    size_bytes: Option<u64>,
    error: Option<&str>
) {
    let status = if success { "✅" } else { "❌" };
    let mut log_msg = format!("{} FILE {} - {}", status, operation.to_uppercase(), path);
    
    if let Some(size) = size_bytes {
        log_msg.push_str(&format!(" ({}KB)", size / 1024));
    }
    
    log::info!("{}", log_msg);
    
    if let Some(err) = error {
        log::error!("   ❌ Error: {}", err);
    }
}

/// Log network operations (for AI API calls)
pub fn log_network_operation(
    operation: &str,
    url: &str,
    method: &str,
    status_code: Option<u16>,
    duration_ms: u64,
    error: Option<&str>
) {
    let status = if error.is_none() { "✅" } else { "❌" };
    log::info!("{} NETWORK {} {} {} ({}ms)", 
        status, method, operation, url, duration_ms);
    
    if let Some(code) = status_code {
        log::info!("   📡 HTTP Status: {}", code);
    }
    
    if let Some(err) = error {
        log::error!("   ❌ Error: {}", err);
    }
}

/// Create a context map from key-value pairs for logging
pub fn create_context(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs.iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// Macro for quick context creation
#[macro_export]
macro_rules! log_context {
    ($($key:expr => $value:expr),* $(,)?) => {
        {
            let mut context = std::collections::HashMap::new();
            $(
                context.insert($key.to_string(), $value.to_string());
            )*
            context
        }
    };
}

/// Log application lifecycle events
pub fn log_lifecycle_event(
    event: &str,
    version: Option<&str>,
    context: Option<&HashMap<String, String>>
) {
    let version_str = version.unwrap_or("unknown");
    log::info!("🚀 LIFECYCLE {} - Version: {}", event, version_str);
    
    if let Some(ctx) = context {
        if !ctx.is_empty() {
            log::info!("   📋 Context: {:?}", ctx);
        }
    }
}

/// Log security/permission events
pub fn log_permission_event(
    permission: &str,
    granted: bool,
    context: Option<&HashMap<String, String>>
) {
    let status = if granted { "✅ GRANTED" } else { "❌ DENIED" };
    log::info!("🔒 PERMISSION {} - {}", permission, status);
    
    if let Some(ctx) = context {
        if !ctx.is_empty() {
            log::info!("   📋 Permission Context: {:?}", ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_context() {
        let context = create_context(&[
            ("model", "base.en"),
            ("language", "auto"),
            ("duration", "3.5s")
        ]);
        
        assert_eq!(context.len(), 3);
        assert_eq!(context.get("model"), Some(&"base.en".to_string()));
        assert_eq!(context.get("language"), Some(&"auto".to_string()));
        assert_eq!(context.get("duration"), Some(&"3.5s".to_string()));
    }

    #[test]
    fn test_log_context_macro() {
        let context = log_context! {
            "operation" => "transcription",
            "model" => "whisper-base",
            "duration" => "2.3s"
        };
        
        assert_eq!(context.len(), 3);
        assert_eq!(context.get("operation"), Some(&"transcription".to_string()));
    }
}