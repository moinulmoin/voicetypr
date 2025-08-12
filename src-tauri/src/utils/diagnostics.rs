/// Unified diagnostics module for VoiceTypr
/// 
/// This module consolidates system monitoring, network diagnostics, and operational logging
/// into a cohesive interface that provides comprehensive debugging capabilities while
/// maintaining high performance and thread safety.

use crate::utils::logger::{log_event, LogEvent, NetworkStatus, NetworkDetails, SystemResources, OperationPhase, LogContext};
use sysinfo::System;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::collections::HashMap;
use std::time::Instant;

/// Global system instance (thread-safe singleton)
static SYSTEM: Lazy<Mutex<System>> = Lazy::new(|| {
    let mut system = System::new_all();
    system.refresh_all();
    Mutex::new(system)
});

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
            NetworkError::AuthenticationFailed { provider } => NetworkStatus::Failed { error: format!("Authentication failed for {}", provider) },
            NetworkError::DnsResolutionFailed { host } => NetworkStatus::Failed { error: format!("DNS resolution failed for {}", host) },
            NetworkError::SslError { details } => NetworkStatus::Failed { error: format!("SSL error: {}", details) },
            NetworkError::ConnectionRefused { endpoint } => NetworkStatus::Failed { error: format!("Connection refused: {}", endpoint) },
            NetworkError::Unknown { message } => NetworkStatus::Failed { error: message },
        }
    }
}

/// Operation tracking for comprehensive debugging
pub struct OperationTracker {
    name: String,
    start_time: Instant,
    context: HashMap<String, String>,
}

impl OperationTracker {
    pub fn start(name: &str, context: HashMap<String, String>) -> Self {
        let tracker = Self {
            name: name.to_string(),
            start_time: Instant::now(),
            context: context.clone(),
        };
        
        log_event(LogEvent::Operation {
            name: name.to_string(),
            phase: OperationPhase::Start,
            context: if context.is_empty() { 
                None 
            } else { 
                Some(LogContext { fields: context }) 
            },
        });
        
        tracker
    }
    
    pub fn complete(self) {
        let duration_ms = self.start_time.elapsed().as_millis() as u64;
        log_event(LogEvent::Operation {
            name: self.name,
            phase: OperationPhase::Complete { duration_ms },
            context: None,
        });
    }
    
    pub fn fail(self, error: &str) {
        log_event(LogEvent::Operation {
            name: self.name,
            phase: OperationPhase::Failed { error: error.to_string() },
            context: Some(LogContext { fields: self.context }),
        });
    }
}

/// System monitoring functions (stateless)
pub mod system {
    use super::*;
    
    /// Get current system resources
    fn get_current_resources() -> SystemResources {
        let mut system = SYSTEM.lock().unwrap();
        system.refresh_all();
        
        let cpu_usage = system.global_cpu_usage();
        let memory_used = system.used_memory();
        let memory_usage_mb = memory_used / 1_048_576; // Convert to MB
        
        // Get available disk space (placeholder for now)
        let disk_available_gb = get_available_disk_space();
        
        SystemResources {
            cpu_usage,
            memory_usage_mb,
            disk_available_gb,
        }
    }
    
    /// Get available disk space in GB (simplified implementation)
    fn get_available_disk_space() -> f64 {
        // In sysinfo 0.36, disks are accessed differently
        // For now, return a placeholder value since disk monitoring is less critical
        // TODO: Update when upgrading to newer sysinfo version or using direct system calls
        100.0 // Return 100GB as a default
    }
    
    /// Log system resources before intensive operations
    pub fn log_resources_before(operation: &str) {
        let resources = get_current_resources();
        
        log_event(LogEvent::System {
            operation: format!("{}_BEFORE", operation.to_uppercase()),
            resources: resources.clone(),
        });
        
        // Warn if resources are constrained
        if resources.cpu_usage > 80.0 {
            log::warn!("⚠️ HIGH_CPU_USAGE: {:.1}% - May affect performance", resources.cpu_usage);
        }
        
        let memory_total_mb = 16384; // Assume 16GB total for percentage calculation
        let memory_percent = (resources.memory_usage_mb as f64 / memory_total_mb as f64) * 100.0;
        if memory_percent > 85.0 {
            log::warn!("⚠️ HIGH_MEMORY_USAGE: {:.1}% - May cause issues", memory_percent);
        }
        
        if resources.disk_available_gb < 1.0 {
            log::error!("❌ LOW_DISK_SPACE: {:.2}GB - Recording may fail", resources.disk_available_gb);
        }
    }
    
    /// Log system resources after operations
    pub fn log_resources_after(operation: &str, duration_ms: u64) {
        let resources = get_current_resources();
        
        log_event(LogEvent::System {
            operation: format!("{}_AFTER_{}ms", operation.to_uppercase(), duration_ms),
            resources,
        });
    }
    
    /// Check for thermal throttling
    pub fn check_thermal_state() -> bool {
        #[cfg(target_os = "macos")]
        {
            let mut system = SYSTEM.lock().unwrap();
            system.refresh_cpu_all();
            
            if !system.cpus().is_empty() {
                let cpu_freq = system.cpus()[0].frequency();
                
                if cpu_freq < 2000 { // Approximate threshold
                    log::warn!("⚠️ THERMAL_THROTTLING_DETECTED - CPU frequency: {}MHz", cpu_freq);
                    return true;
                }
            }
        }
        
        false
    }
    
    /// Detect if running in virtual machine
    pub fn detect_virtual_environment() -> Option<String> {
        let vm_indicators = vec![
            ("VirtualBox", vec!["vbox", "virtualbox"]),
            ("VMware", vec!["vmware"]),
            ("Parallels", vec!["parallels"]),
            ("QEMU", vec!["qemu"]),
            ("Hyper-V", vec!["hyperv", "microsoft corporation"]),
        ];
        
        let system_vendor = System::name().unwrap_or_default().to_lowercase();
        
        for (vm_name, indicators) in vm_indicators {
            for indicator in indicators {
                if system_vendor.contains(indicator) {
                    log::warn!("⚠️ VIRTUAL_ENVIRONMENT_DETECTED: {}", vm_name);
                    return Some(vm_name.to_string());
                }
            }
        }
        
        None
    }
}

/// Network diagnostics functions (stateless)
pub mod network {
    use super::*;
    
    /// Log AI API request with full context
    pub fn log_api_request(provider: &str, model: &str, token_count: usize) {
        log::info!("🌐 API_REQUEST_START:");
        log::info!("  • Provider: {}", provider);
        log::info!("  • Model: {}", model);
        log::info!("  • Token Count: {}", token_count);
        
        if token_count > 3000 {
            log::warn!("⚠️ HIGH_TOKEN_COUNT: {} tokens may cause issues", token_count);
        }
    }
    
    /// Log API response using structured logging
    pub fn log_api_response(provider: &str, method: &str, endpoint: &str, status_code: u16, duration_ms: u64, tokens_used: Option<usize>) {
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
            operation: format!("API_REQUEST_{}", provider.to_uppercase()),
            status,
            duration_ms,
            details: Some(details),
        });
        
        if let Some(tokens) = tokens_used {
            log::info!("  • Tokens Used: {}", tokens);
        }
        
        if duration_ms > 5000 {
            log::warn!("⚠️ SLOW_API_RESPONSE: {}ms from {}", duration_ms, provider);
        }
    }
    
    /// Log network error with categorization
    pub fn log_network_error(operation: &str, error: NetworkError) {
        let status = NetworkStatus::from(error.clone());
        
        log_event(LogEvent::Network {
            operation: operation.to_string(),
            status,
            duration_ms: 0, // Not applicable for errors
            details: None,
        });
        
        // Additional context-specific suggestions
        match error {
            NetworkError::Timeout { .. } => {
                log::error!("  • Suggestion: Check internet connection or increase timeout");
            },
            NetworkError::RateLimited { .. } => {
                log::error!("  • Suggestion: Reduce request frequency or upgrade plan");
            },
            NetworkError::AuthenticationFailed { .. } => {
                log::error!("  • Suggestion: Check API key in settings");
            },
            NetworkError::DnsResolutionFailed { .. } => {
                log::error!("  • Suggestion: Check DNS settings or internet connection");
            },
            NetworkError::SslError { .. } => {
                log::error!("  • Suggestion: Check system time or proxy settings");
            },
            NetworkError::ConnectionRefused { .. } => {
                log::error!("  • Suggestion: Check firewall or proxy settings");
            },
            NetworkError::Unknown { .. } => {}
        }
    }
    
    /// Log retry attempt
    pub fn log_retry_attempt(operation: &str, attempt_number: u32, max_attempts: u32) {
        log::info!("🔄 RETRY_ATTEMPT: {} (attempt {}/{})", operation, attempt_number, max_attempts);
    }
}

/// Convenience functions for common operations
pub mod operations {
    use super::*;
    
    /// Track a complete operation with automatic resource monitoring
    pub fn track_intensive_operation<F, R>(name: &str, context: HashMap<String, String>, f: F) -> Result<R, String>
    where
        F: FnOnce() -> Result<R, String>,
    {
        // Log system resources before
        system::log_resources_before(name);
        
        // Start operation tracking
        let tracker = OperationTracker::start(name, context);
        let start_time = Instant::now();
        
        // Execute the operation
        match f() {
            Ok(result) => {
                let duration_ms = start_time.elapsed().as_millis() as u64;
                system::log_resources_after(name, duration_ms);
                tracker.complete();
                Ok(result)
            }
            Err(error) => {
                let duration_ms = start_time.elapsed().as_millis() as u64;
                system::log_resources_after(name, duration_ms);
                tracker.fail(&error);
                Err(error)
            }
        }
    }
    
    /// Helper function to create context maps
    pub fn create_context(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
}

