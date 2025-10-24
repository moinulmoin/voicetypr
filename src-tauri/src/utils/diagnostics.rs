/// Unified diagnostics module for VoiceTypr
///
/// This module consolidates system monitoring, network diagnostics, and operational logging
/// into a cohesive interface that provides comprehensive debugging capabilities while
/// maintaining high performance and thread safety.
use crate::utils::logger::*;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;
use sysinfo::System;

/// Global system instance (thread-safe singleton)
static SYSTEM: Lazy<Mutex<System>> = Lazy::new(|| {
    let mut system = System::new_all();
    system.refresh_all();
    Mutex::new(system)
});

// REMOVED: NetworkError enum - duplicate definition exists in network_diagnostics.rs
// Use crate::utils::network_diagnostics::NetworkError instead

// REMOVED: OperationTracker - replaced with direct logging calls
// Using explicit log_start/log_complete/log_failed provides better control

/// System monitoring functions (stateless)
pub mod system {
    use super::*;

    // SystemResources struct defined locally
    struct SystemResources {
        cpu_usage: f32,
        memory_usage_mb: u64,
        disk_available_gb: f64,
    }

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

        log::info!(
            "💻 {}_BEFORE | CPU: {:.1}% | Memory: {}MB | Disk: {:.1}GB",
            operation.to_uppercase(),
            resources.cpu_usage,
            resources.memory_usage_mb,
            resources.disk_available_gb
        );

        // Warn if resources are constrained
        if resources.cpu_usage > 80.0 {
            log::warn!(
                "⚠️ HIGH_CPU_USAGE: {:.1}% - May affect performance",
                resources.cpu_usage
            );
        }

        let memory_total_mb = 16384; // Assume 16GB total for percentage calculation
        let memory_percent = (resources.memory_usage_mb as f64 / memory_total_mb as f64) * 100.0;
        if memory_percent > 85.0 {
            log::warn!(
                "⚠️ HIGH_MEMORY_USAGE: {:.1}% - May cause issues",
                memory_percent
            );
        }

        if resources.disk_available_gb < 1.0 {
            log::error!(
                "❌ LOW_DISK_SPACE: {:.2}GB - Recording may fail",
                resources.disk_available_gb
            );
        }
    }

    /// Log system resources after operations
    pub fn log_resources_after(operation: &str, duration_ms: u64) {
        let resources = get_current_resources();

        log::info!(
            "💻 {}_AFTER_{}ms | CPU: {:.1}% | Memory: {}MB | Disk: {:.1}GB",
            operation.to_uppercase(),
            duration_ms,
            resources.cpu_usage,
            resources.memory_usage_mb,
            resources.disk_available_gb
        );
    }
}

// Network diagnostics moved to network_diagnostics.rs module
// Import from there when needed: use crate::utils::network_diagnostics::*;

/// Convenience functions for common operations
pub mod operations {
    use super::*;

    /// Track a complete operation with automatic resource monitoring
    #[allow(dead_code)] // Comprehensive tracking for future performance analysis
    pub fn track_intensive_operation<F, R>(
        name: &str,
        context: HashMap<String, String>,
        f: F,
    ) -> Result<R, String>
    where
        F: FnOnce() -> Result<R, String>,
    {
        // Log system resources before
        system::log_resources_before(name);

        // Start operation tracking with direct logging
        log_start(name);
        if !context.is_empty() {
            let ctx: Vec<(&str, &str)> = context
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            log_with_context(log::Level::Debug, &format!("{} context", name), &ctx);
        }
        let start_time = Instant::now();

        // Execute the operation
        match f() {
            Ok(result) => {
                let duration_ms = start_time.elapsed().as_millis() as u64;
                system::log_resources_after(name, duration_ms);
                log_complete(name, duration_ms);
                Ok(result)
            }
            Err(error) => {
                let duration_ms = start_time.elapsed().as_millis() as u64;
                system::log_resources_after(name, duration_ms);
                log_failed(name, &error);
                if !context.is_empty() {
                    let ctx: Vec<(&str, &str)> = context
                        .iter()
                        .map(|(k, v)| (k.as_str(), v.as_str()))
                        .collect();
                    log_with_context(
                        log::Level::Debug,
                        &format!("{} failure context", name),
                        &ctx,
                    );
                }
                Err(error)
            }
        }
    }

    /// Helper function to create context maps
    #[allow(dead_code)] // Utility for creating diagnostic contexts
    pub fn create_context(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
}
