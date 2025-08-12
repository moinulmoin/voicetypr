use std::collections::HashMap;
use std::time::Instant;
use serde::Serialize;

/// Structured logging utilities for VoiceTypr debugging
/// 
/// This module provides consistent logging patterns across the application
/// to help diagnose production issues, performance bottlenecks, and silent failures.
/// 
/// ## Performance Strategy
/// 
/// This module uses a **selective performance optimization** approach:
/// 
/// ### Production Logging (What Users See)
/// - **Major operations**: ALWAYS logged (recording, transcription, errors)
/// - **Hot paths**: Sampled at 1% or disabled
/// - **Debug details**: Only when explicitly enabled via log level
/// 
/// ### Performance Optimizations
/// 1. **Log level checking**: Skip expensive ops if logs filtered out
/// 2. **Sampling**: Hot paths use `log_context_sampled!` (1% sample rate)
/// 3. **Lazy evaluation**: Only format strings if actually logging
/// 
/// ### Usage Guidelines
/// 
/// For different code paths:
/// - **User operations** (record/transcribe): Use normal logging
/// - **Hot paths** (audio processing): Use `log_simple!` or sampling
/// - **Error paths**: ALWAYS log with full context
/// - **Performance tests**: Use specialized test macros

/// Structured log event types to replace HashMap contexts
#[derive(Debug, Clone, Serialize)]
pub enum LogEvent {
    Operation {
        name: String,
        phase: OperationPhase,
        #[serde(skip_serializing_if = "Option::is_none")]
        context: Option<LogContext>,
    },
    Network {
        operation: String,
        status: NetworkStatus,
        duration_ms: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<NetworkDetails>,
    },
    System {
        operation: String,
        resources: SystemResources,
    },
    Audio {
        operation: String,
        metrics: AudioMetrics,
    },
    Model {
        operation: String,
        model: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<ModelStatus>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub enum OperationPhase {
    Start,
    Progress(u8),
    Complete { duration_ms: u64 },
    Failed { error: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct LogContext {
    #[serde(flatten)]
    pub fields: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub enum NetworkStatus {
    Success,
    RateLimited { retry_after: Option<u64> },
    Timeout { duration_ms: u64 },
    Failed { error: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkDetails {
    pub endpoint: String,
    pub method: String,
    pub status_code: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemResources {
    pub cpu_usage: f32,
    pub memory_usage_mb: u64,
    pub disk_available_gb: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioMetrics {
    pub duration_seconds: f32,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, Clone, Serialize)]
pub enum ModelStatus {
    Downloading { progress: u8 },
    Downloaded,
    Failed { error: String },
}

/// Unified logging function to replace all specialized functions
pub fn log_event(event: LogEvent) {
    match event {
        LogEvent::Operation { name, phase, context } => {
            let ctx_str = context.map(|c| format!(" | {:?}", c.fields)).unwrap_or_default();
            match phase {
                OperationPhase::Start => {
                    log::info!("🚀 {} STARTING{}", name, ctx_str);
                }
                OperationPhase::Progress(percent) => {
                    log::info!("📊 {} PROGRESS: {}%{}", name, percent, ctx_str);
                }
                OperationPhase::Complete { duration_ms } => {
                    log::info!("✅ {} COMPLETE in {}ms{}", name, duration_ms, ctx_str);
                }
                OperationPhase::Failed { error } => {
                    log::error!("❌ {} FAILED: {}{}", name, error, ctx_str);
                }
            }
        }
        LogEvent::Network { operation, status, duration_ms, details } => {
            let detail_str = details.map(|d| format!(" | {} {}", d.method, d.endpoint)).unwrap_or_default();
            match status {
                NetworkStatus::Success => {
                    log::info!("🌐 {} SUCCESS in {}ms{}", operation, duration_ms, detail_str);
                }
                NetworkStatus::RateLimited { retry_after } => {
                    let retry_str = retry_after.map(|r| format!(" (retry after {}s)", r)).unwrap_or_default();
                    log::warn!("⚠️ {} RATE LIMITED{}{}", operation, retry_str, detail_str);
                }
                NetworkStatus::Timeout { duration_ms: timeout } => {
                    log::error!("⏱️ {} TIMEOUT after {}ms{}", operation, timeout, detail_str);
                }
                NetworkStatus::Failed { error } => {
                    log::error!("❌ {} FAILED: {}{}", operation, error, detail_str);
                }
            }
        }
        LogEvent::System { operation, resources } => {
            log::info!("💻 {} | CPU: {:.1}% | Memory: {}MB | Disk: {:.1}GB", 
                operation, resources.cpu_usage, resources.memory_usage_mb, resources.disk_available_gb);
        }
        LogEvent::Audio { operation, metrics } => {
            log::info!("🎵 {} | Duration: {:.2}s | Rate: {}Hz | Channels: {}", 
                operation, metrics.duration_seconds, metrics.sample_rate, metrics.channels);
        }
        LogEvent::Model { operation, model, status } => {
            let status_str = status.map(|s| match s {
                ModelStatus::Downloading { progress } => format!(" | Downloading: {}%", progress),
                ModelStatus::Downloaded => " | Downloaded".to_string(),
                ModelStatus::Failed { error } => format!(" | Failed: {}", error),
            }).unwrap_or_default();
            log::info!("🤖 {} | Model: {}{}", operation, model, status_str);
        }
    }
}

/// Log function entry and exit with automatic timing
#[inline]
pub fn log_function<F, R>(name: &str, f: F) -> R 
where 
    F: FnOnce() -> R 
{
    // Always log in production for debugging user issues
    // The overhead is acceptable for non-hot-path functions
    if log::log_enabled!(log::Level::Info) {
        let start = Instant::now();
        log::info!("→ {} START", name);
        let result = f();
        log::info!("← {} END ({}ms)", name, start.elapsed().as_millis());
        result
    } else {
        f()
    }
}

/// Log function entry and exit with context data (zero-cost in release)
#[inline]
pub fn log_function_with_context<F, R>(
    name: &str, 
    context: &HashMap<String, String>, 
    f: F
) -> R 
where 
    F: FnOnce() -> R 
{
    #[cfg(debug_assertions)]
    {
        if log::log_enabled!(log::Level::Info) {
            let start = Instant::now();
            log::info!("→ {} START - Context: {:?}", name, context);
            let result = f();
            log::info!("← {} END ({}ms) - Context: {:?}", name, start.elapsed().as_millis(), context);
            result
        } else {
            f()
        }
    }
    
    #[cfg(not(debug_assertions))]
    {
        f()
    }
}

/// Log async function with timing
#[inline]
pub async fn log_async_function<F, Fut, R>(name: &str, f: F) -> R
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = R>,
{
    // Keep logging in production for debugging user issues
    if log::log_enabled!(log::Level::Info) {
        let start = Instant::now();
        log::info!("→ {} START (async)", name);
        let result = f().await;
        log::info!("← {} END ({}ms) (async)", name, start.elapsed().as_millis());
        result
    } else {
        f().await
    }
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

// Removed: log_performance_detailed - simplified to basic log_performance

/// Log operation start with parameters (COMPATIBILITY WRAPPER)
#[inline]
pub fn log_operation_start(operation: &str, params: &HashMap<String, String>) {
    log_event(LogEvent::Operation {
        name: operation.to_string(),
        phase: OperationPhase::Start,
        context: if params.is_empty() { 
            None 
        } else { 
            Some(LogContext { fields: params.clone() }) 
        },
    });
}

/// Log operation completion with results (COMPATIBILITY WRAPPER)
#[inline]
pub fn log_operation_complete(operation: &str, duration_ms: u64, results: &HashMap<String, String>) {
    log_event(LogEvent::Operation {
        name: operation.to_string(),
        phase: OperationPhase::Complete { duration_ms },
        context: if results.is_empty() { 
            None 
        } else { 
            Some(LogContext { fields: results.clone() }) 
        },
    });
}

/// Log operation failure with context (COMPATIBILITY WRAPPER)
pub fn log_operation_failed(operation: &str, error: &str, context: &HashMap<String, String>) {
    log_event(LogEvent::Operation {
        name: operation.to_string(),
        phase: OperationPhase::Failed { error: error.to_string() },
        context: if context.is_empty() { 
            None 
        } else { 
            Some(LogContext { fields: context.clone() }) 
        },
    });
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

// Removed: log_system_resources - use diagnostics::system module instead

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

// Removed: log_network_operation - use diagnostics::network module instead

// Removed: create_context - use diagnostics::create_context instead

/// Lightweight logging macro for hot paths (no HashMap allocation)
/// Use this instead of log_context! in performance-critical code
#[macro_export]
macro_rules! log_simple {
    ($operation:expr) => {
        #[cfg(debug_assertions)]
        {
            if log::log_enabled!(log::Level::Debug) {
                log::debug!("{}", $operation);
            }
        }
    };
    ($operation:expr, $($arg:expr),*) => {
        #[cfg(debug_assertions)]
        {
            if log::log_enabled!(log::Level::Debug) {
                log::debug!("{}: {}", $operation, format!($($arg),*));
            }
        }
    };
}

/// Macro for quick context creation (optimized for performance)
/// For hot paths (called 1000+ times/sec), use log_context_sampled! instead
#[macro_export]
macro_rules! log_context {
    ($($key:expr => $value:expr),* $(,)?) => {
        {
            // Only create HashMap if logging is actually enabled
            // This prevents allocation when logs are filtered out
            if log::log_enabled!(log::Level::Debug) {
                let mut context = std::collections::HashMap::new();
                $(
                    context.insert($key.to_string(), $value.to_string());
                )*
                context
            } else {
                std::collections::HashMap::new()
            }
        }
    };
}

/// Sampled context creation for hot paths (only creates context 1% of the time)
#[macro_export]
macro_rules! log_context_sampled {
    ($($key:expr => $value:expr),* $(,)?) => {
        {
            // Sample at 1% rate for hot paths (thread-safe)
            use once_cell::sync::Lazy;
            use std::sync::atomic::{AtomicUsize, Ordering};
            
            static SAMPLE_COUNTER: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));
            let count = SAMPLE_COUNTER.fetch_add(1, Ordering::Relaxed);
            
            if count % 100 == 0 && log::log_enabled!(log::Level::Debug) {
                let mut context = std::collections::HashMap::new();
                $(
                    context.insert($key.to_string(), $value.to_string());
                )*
                context
            } else {
                std::collections::HashMap::new()
            }
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
        // Local helper for test
        fn create_context(pairs: &[(&str, &str)]) -> HashMap<String, String> {
            pairs.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        }
        
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
        // Context is created only if logging is enabled
        // This prevents unnecessary allocations when logs are filtered
        let context = log_context! {
            "operation" => "transcription",
            "model" => "whisper-base",
            "duration" => "2.3s"
        };
        
        // The context creation depends on log level
        if log::log_enabled!(log::Level::Debug) {
            assert_eq!(context.len(), 3);
            assert_eq!(context.get("operation"), Some(&"transcription".to_string()));
        } else {
            // Context is empty when logging is disabled (performance optimization)
            assert_eq!(context.len(), 0);
        }
    }
}