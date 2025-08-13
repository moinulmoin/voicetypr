use sysinfo::{System, Disks};
use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::path::Path;

/// System resource thresholds (configurable via environment variables)
mod thresholds {
    /// CPU usage warning threshold (default: 80%)
    pub fn cpu_warning_percent() -> f32 {
        std::env::var("VOICETYPR_CPU_THRESHOLD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(80.0)
    }
    
    /// Memory usage warning threshold (default: 85%)
    pub fn memory_warning_percent() -> u64 {
        std::env::var("VOICETYPR_MEMORY_THRESHOLD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(85)
    }
    
    /// Disk space critical threshold in GB (default: 1.0 GB)
    pub fn disk_critical_gb() -> f64 {
        std::env::var("VOICETYPR_DISK_THRESHOLD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1.0)
    }
    
    /// CPU frequency threshold for thermal throttling detection (MHz)
    #[allow(dead_code)]
    pub fn cpu_throttle_mhz() -> u64 {
        std::env::var("VOICETYPR_CPU_THROTTLE_MHZ")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2000)
    }
}

/// Global system instance (thread-safe singleton)
static SYSTEM: Lazy<Mutex<System>> = Lazy::new(|| {
    let mut system = System::new_all();
    system.refresh_all();
    Mutex::new(system)
});

/// Simple struct for system resources
struct SystemResources {
    cpu_usage: f32,
    memory_usage_mb: u64,
    disk_available_gb: f64,
}

/// Get current system resources
fn get_current_resources() -> SystemResources {
    let mut system = match SYSTEM.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log::error!("System monitor lock poisoned, recovering");
            poisoned.into_inner()
        }
    };
    system.refresh_all();
    
    let cpu_usage = system.global_cpu_usage();
    let memory_used = system.used_memory();
    let _memory_total = system.total_memory();
    let memory_usage_mb = memory_used / 1_048_576; // Convert to MB
    
    // Get actual disk space for the current working directory
    let disk_available_gb = get_available_disk_space();
    
    SystemResources {
        cpu_usage,
        memory_usage_mb,
        disk_available_gb,
    }
}

/// Get available disk space in GB for the current working directory
fn get_available_disk_space() -> f64 {
    // Try to get disk space for the current directory
    let current_dir = std::env::current_dir().unwrap_or_else(|_| Path::new("/").to_path_buf());
    
    // Create a new Disks instance and refresh it
    let disks = Disks::new_with_refreshed_list();
    
    // Find the disk that contains our current directory
    for disk in disks.list() {
        // Check if current directory is on this disk
        if current_dir.starts_with(disk.mount_point()) {
            let available_bytes = disk.available_space();
            let available_gb = available_bytes as f64 / 1_073_741_824.0; // Convert bytes to GB
            
            log::debug!(
                "Disk space check - Mount: {:?}, Available: {:.2}GB", 
                disk.mount_point(), 
                available_gb
            );
            
            return available_gb;
        }
    }
    
    // Fallback: If we can't find the specific disk, return the total available space
    // across all non-removable disks
    let total_available: u64 = disks.list()
        .iter()
        .filter(|disk| !disk.is_removable())
        .map(|disk| disk.available_space())
        .sum();
    
    let available_gb = total_available as f64 / 1_073_741_824.0;
    
    log::warn!(
        "Could not find specific disk for {:?}, using total available: {:.2}GB", 
        current_dir,
        available_gb
    );
    
    available_gb
}

/// Log system resources before intensive operations (stateless)
pub fn log_resources_before_operation(operation: &str) {
    let resources = get_current_resources();
    
    log::info!("💻 {}_BEFORE | CPU: {:.1}% | Memory: {}MB | Disk: {:.1}GB", 
        operation, resources.cpu_usage, resources.memory_usage_mb, resources.disk_available_gb);
    
    // Warn if resources are constrained (using configurable thresholds)
    if resources.cpu_usage > thresholds::cpu_warning_percent() {
        log::warn!(
            "⚠️ HIGH_CPU_USAGE: {:.1}% (threshold: {:.0}%) - May affect performance", 
            resources.cpu_usage,
            thresholds::cpu_warning_percent()
        );
    }
    
    let total_memory_mb = match SYSTEM.lock() {
        Ok(system) => system.total_memory() / 1_048_576,
        Err(poisoned) => {
            log::error!("System monitor lock poisoned during memory check");
            poisoned.into_inner().total_memory() / 1_048_576
        }
    };
    
    let memory_threshold = thresholds::memory_warning_percent();
    if resources.memory_usage_mb > total_memory_mb * memory_threshold / 100 {
        log::warn!(
            "⚠️ HIGH_MEMORY_USAGE: {}MB of {}MB (>{:.0}%) - May cause issues",
            resources.memory_usage_mb,
            total_memory_mb,
            memory_threshold
        );
    }
    
    if resources.disk_available_gb < thresholds::disk_critical_gb() {
        log::error!(
            "❌ LOW_DISK_SPACE: {:.2}GB (threshold: {:.1}GB) - Recording may fail", 
            resources.disk_available_gb,
            thresholds::disk_critical_gb()
        );
    }
}

/// Log resource changes after operation (stateless)
pub fn log_resources_after_operation(operation: &str, duration_ms: u64) {
    let resources = get_current_resources();
    
    log::info!("💻 {}_AFTER | CPU: {:.1}% | Memory: {}MB | Disk: {:.1}GB", 
        operation, resources.cpu_usage, resources.memory_usage_mb, resources.disk_available_gb);
    
    log::info!("⏱️ Operation completed in {}ms", duration_ms);
}

/// Check for thermal throttling
#[allow(dead_code)] // Available for performance diagnostics
pub fn check_thermal_state() -> bool {
    // Platform-specific thermal checks
    #[cfg(target_os = "macos")]
    {
        // Check if CPU frequency is reduced
        let system = match SYSTEM.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                log::error!("System monitor lock poisoned during thermal check");
                poisoned.into_inner()
            }
        };
        if let Some(cpu) = system.cpus().first() {
            let cpu_freq = cpu.frequency();
            
            // If frequency drops below threshold, likely throttling
            if cpu_freq < thresholds::cpu_throttle_mhz() {
                log::warn!(
                    "⚠️ THERMAL_THROTTLING_DETECTED - CPU frequency: {}MHz (threshold: {}MHz)", 
                    cpu_freq,
                    thresholds::cpu_throttle_mhz()
                );
                return true;
            }
        }
    }
    
    false
}

/// Monitor GPU memory (if available)
#[allow(dead_code)] // Available for GPU diagnostics when needed
pub fn log_gpu_memory() {
    #[cfg(target_os = "macos")]
    {
        // Use Metal performance shaders info if available
        if let Ok(output) = std::process::Command::new("system_profiler")
            .args(&["SPDisplaysDataType", "-json"])
            .output() 
        {
            if let Ok(_json) = String::from_utf8(output.stdout) {
                // Parse and log GPU memory
                log::info!("🎮 GPU_MEMORY_CHECK completed");
            }
        }
    }
    
    #[cfg(windows)]
    {
        // Use DXGI or WMI for GPU info
        log::info!("🎮 GPU_MEMORY_CHECK - Windows GPU monitoring");
    }
}

/// Detect if running in virtual machine
#[allow(dead_code)] // Useful for environment-specific debugging
pub fn detect_virtual_environment() -> Option<String> {
    // Check for VM indicators
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