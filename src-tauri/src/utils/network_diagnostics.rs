
use crate::utils::logger::*;

/// Network error types for enhanced diagnostics
#[derive(Debug, Clone)]
#[allow(dead_code)] // Comprehensive error types for network diagnostics
pub enum NetworkError {
    Timeout { duration_ms: u64 },
    RateLimited { retry_after: Option<u64> },
    AuthenticationFailed { provider: String },
    DnsResolutionFailed { host: String },
    SslError { details: String },
    ConnectionRefused { endpoint: String },
    Unknown { message: String },
}

// NetworkStatus conversion removed - using simple logging functions instead

/// Log AI API request with full context (stateless)
pub fn log_api_request(provider: &str, model: &str, token_count: usize) {
    log::info!("🌐 API_REQUEST_START:");
    log::info!("  • Provider: {}", provider);
    log::info!("  • Model: {}", model);
    log::info!("  • Token Count: {}", token_count);
    
    // Warn if high token count
    if token_count > 3000 {
        log::warn!("⚠️ HIGH_TOKEN_COUNT: {} tokens may cause issues", token_count);
    }
}

/// Log API response with details using structured logging
pub fn log_api_response(
    provider: &str,
    method: &str,
    endpoint: &str,
    status_code: u16,
    duration_ms: u64,
    tokens_used: Option<usize>,
) {
    let status_str = if status_code == 200 {
        "SUCCESS"
    } else if status_code == 429 {
        "RATE_LIMITED"  
    } else if status_code >= 500 {
        "SERVER_ERROR"
    } else if status_code == 401 || status_code == 403 {
        "AUTH_ERROR"
    } else {
        "ERROR"
    };
    
    log::info!("🌐 API_{} {} in {}ms | {} {} | Status: {}", 
        provider.to_uppercase(), status_str, duration_ms, method, endpoint, status_code);

    if let Some(tokens) = tokens_used {
        log::info!("  • Tokens Used: {}", tokens);
    }
    
    // Log performance warnings
    if duration_ms > 5000 {
        log::warn!("⚠️ SLOW_API_RESPONSE: {}ms from {}", duration_ms, provider);
    }
}

/// Log network error with categorization and optional timing
pub fn log_network_error_with_duration(error: NetworkError, duration_ms: Option<u64>) {
    let operation = match &error {
        NetworkError::Timeout { .. } => "NETWORK_TIMEOUT",
        NetworkError::RateLimited { .. } => "RATE_LIMITED",
        NetworkError::AuthenticationFailed { .. } => "AUTH_FAILED",
        NetworkError::DnsResolutionFailed { .. } => "DNS_FAILED",
        NetworkError::SslError { .. } => "SSL_ERROR",
        NetworkError::ConnectionRefused { .. } => "CONNECTION_REFUSED",
        NetworkError::Unknown { .. } => "NETWORK_ERROR",
    };
    
    // For timeout errors, use the duration from the error if not provided
    let final_duration = duration_ms.unwrap_or_else(|| {
        if let NetworkError::Timeout { duration_ms } = &error {
            *duration_ms
        } else {
            0
        }
    });

    let error_str = match &error {
        NetworkError::Timeout { duration_ms } => format!("Timeout after {}ms", duration_ms),
        NetworkError::RateLimited { retry_after } => {
            if let Some(seconds) = retry_after {
                format!("Rate limited, retry after {}s", seconds)
            } else {
                "Rate limited".to_string()
            }
        }
        NetworkError::AuthenticationFailed { provider } => format!("Auth failed for {}", provider),
        NetworkError::DnsResolutionFailed { host } => format!("DNS failed for {}", host),
        NetworkError::SslError { details } => format!("SSL error: {}", details),
        NetworkError::ConnectionRefused { endpoint } => format!("Connection refused: {}", endpoint),
        NetworkError::Unknown { message } => message.clone(),
    };
    
    log::error!("❌ {} FAILED: {} ({}ms)", operation, error_str, final_duration);

    // Log helpful suggestions
    match error {
        NetworkError::Timeout { duration_ms: _ } => {
            log::error!("  • Suggestion: Check internet connection or increase timeout");
        }
        NetworkError::RateLimited { retry_after } => {
            if let Some(seconds) = retry_after {
                log::error!("  • Retry after: {} seconds", seconds);
            }
            log::error!("  • Suggestion: Reduce request frequency or upgrade plan");
        }
        NetworkError::AuthenticationFailed { provider: _ } => {
            log::error!("  • Suggestion: Check API key in settings");
        }
        NetworkError::DnsResolutionFailed { host: _ } => {
            log::error!("  • Suggestion: Check DNS settings or internet connection");
        }
        NetworkError::SslError { .. } => {
            log::error!("  • Suggestion: Check system time or proxy settings");
        }
        NetworkError::ConnectionRefused { .. } => {
            log::error!("  • Suggestion: Check firewall or proxy settings");
        }
        NetworkError::Unknown { .. } => {}
    }
}

/// Log network error with categorization (backward compatibility)
pub fn log_network_error(error: NetworkError) {
    log_network_error_with_duration(error, None);
}

/// Log retry attempt for network operations
pub fn log_retry_attempt(operation: &str, attempt: u32, max_attempts: u32) {
    log::info!("🔄 RETRY_ATTEMPT: {} (attempt {}/{})", operation, attempt, max_attempts);
}

/// Log network connectivity status check
#[allow(dead_code)] // Available for network diagnostics
pub fn log_connectivity_check(host: &str, success: bool, duration_ms: u64) {
    if success {
        log_with_context(log::Level::Info, "Network connectivity verified", &[
            ("operation", "CONNECTIVITY_CHECK"),
            ("host", host),
            ("result", "success"),
            ("duration_ms", &duration_ms.to_string().as_str())
        ]);
        log::info!("🌐 CONNECTIVITY_OK: {} reachable in {}ms", host, duration_ms);
    } else {
        log_with_context(log::Level::Error, "Network connectivity failed", &[
            ("operation", "CONNECTIVITY_CHECK"),
            ("host", host),
            ("result", "failed"),
            ("duration_ms", &duration_ms.to_string().as_str())
        ]);
        log::error!("❌ CONNECTIVITY_FAILED: {} not reachable ({}ms)", host, duration_ms);
        log::error!("  • Suggestion: Check internet connection and DNS settings");
    }
}

/// Log network interface information
#[allow(dead_code)] // Available for network diagnostics
pub fn log_network_interfaces() {
    log_with_context(log::Level::Debug, "Network interface enumeration", &[
        ("operation", "NETWORK_INTERFACES"),
        ("platform", std::env::consts::OS)
    ]);
    // This would require additional dependencies to implement fully
    log::debug!("📡 NETWORK_INTERFACES: Enumeration requested for debugging");
}