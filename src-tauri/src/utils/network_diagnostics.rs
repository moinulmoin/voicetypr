use crate::utils::logger::{log_event, LogEvent, NetworkStatus, NetworkDetails};

/// Network error types for enhanced diagnostics
#[derive(Debug, Clone)]
pub enum NetworkError {
    Timeout { duration_ms: u64 },
    RateLimited { retry_after: Option<u64> },
    AuthenticationFailed { provider: String },
    DnsResolutionFailed { host: String },
    SslError { details: String },
    ConnectionRefused { endpoint: String },
    Unknown { message: String },
}

impl From<NetworkError> for NetworkStatus {
    fn from(error: NetworkError) -> Self {
        match error {
            NetworkError::Timeout { duration_ms } => NetworkStatus::Timeout { duration_ms },
            NetworkError::RateLimited { retry_after } => NetworkStatus::RateLimited { retry_after },
            NetworkError::AuthenticationFailed { provider } => {
                NetworkStatus::Failed { error: format!("Authentication failed for {}", provider) }
            }
            NetworkError::DnsResolutionFailed { host } => {
                NetworkStatus::Failed { error: format!("DNS resolution failed for {}", host) }
            }
            NetworkError::SslError { details } => {
                NetworkStatus::Failed { error: format!("SSL error: {}", details) }
            }
            NetworkError::ConnectionRefused { endpoint } => {
                NetworkStatus::Failed { error: format!("Connection refused: {}", endpoint) }
            }
            NetworkError::Unknown { message } => NetworkStatus::Failed { error: message },
        }
    }
}

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
    let status = if status_code == 200 {
        NetworkStatus::Success
    } else {
        NetworkStatus::Failed { error: format!("HTTP {}", status_code) }
    };
    
    let details = NetworkDetails {
        endpoint: endpoint.to_string(),
        method: method.to_string(),
        status_code: Some(status_code),
    };

    log_event(LogEvent::Network {
        operation: format!("API_{}", provider.to_uppercase()),
        status,
        duration_ms,
        details: Some(details),
    });

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

    log_event(LogEvent::Network {
        operation: operation.to_string(),
        status: error.clone().into(),
        duration_ms: final_duration,
        details: None,
    });

    // Log helpful suggestions
    match error {
        NetworkError::Timeout { duration_ms } => {
            log::error!("  • Suggestion: Check internet connection or increase timeout");
        }
        NetworkError::RateLimited { retry_after } => {
            if let Some(seconds) = retry_after {
                log::error!("  • Retry after: {} seconds", seconds);
            }
            log::error!("  • Suggestion: Reduce request frequency or upgrade plan");
        }
        NetworkError::AuthenticationFailed { provider } => {
            log::error!("  • Suggestion: Check API key in settings");
        }
        NetworkError::DnsResolutionFailed { host } => {
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