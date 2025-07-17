use sha2::{Digest, Sha256};
use std::process::Command;

/// Generate a unique device hash based on the machine's hardware ID
pub fn get_device_hash() -> Result<String, String> {
    let machine_id = get_machine_uuid()?;

    // Hash the machine ID for privacy
    let mut hasher = Sha256::new();
    hasher.update(machine_id.as_bytes());
    let result = hasher.finalize();

    Ok(format!("{:x}", result))
}

/// Get the machine's unique identifier based on the platform
fn get_machine_uuid() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        get_macos_uuid()
    }

    #[cfg(target_os = "windows")]
    {
        get_windows_uuid()
    }

    #[cfg(target_os = "linux")]
    {
        get_linux_uuid()
    }
}

#[cfg(target_os = "macos")]
fn get_macos_uuid() -> Result<String, String> {
    // Get hardware UUID on macOS
    let output = Command::new("ioreg")
        .args(&["-d2", "-c", "IOPlatformExpertDevice"])
        .output()
        .map_err(|e| format!("Failed to execute ioreg: {}", e))?;

    if !output.status.success() {
        return Err("Failed to get hardware UUID".to_string());
    }

    let output_str = String::from_utf8_lossy(&output.stdout);

    // Parse the UUID from the output
    for line in output_str.lines() {
        if line.contains("IOPlatformUUID") {
            if let Some(uuid_part) = line.split("\"").nth(3) {
                return Ok(uuid_part.to_string());
            }
        }
    }

    Err("Could not find hardware UUID".to_string())
}

#[cfg(target_os = "windows")]
fn get_windows_uuid() -> Result<String, String> {
    // Get machine GUID on Windows
    let output = Command::new("wmic")
        .args(&["csproduct", "get", "UUID"])
        .output()
        .map_err(|e| format!("Failed to execute wmic: {}", e))?;

    if !output.status.success() {
        return Err("Failed to get machine GUID".to_string());
    }

    let output_str = String::from_utf8_lossy(&output.stdout);

    // Parse the UUID from the output (skip header line)
    for line in output_str.lines().skip(1) {
        let trimmed = line.trim();
        if !trimmed.is_empty() && trimmed != "UUID" {
            return Ok(trimmed.to_string());
        }
    }

    Err("Could not find machine GUID".to_string())
}

#[cfg(target_os = "linux")]
fn get_linux_uuid() -> Result<String, String> {
    // Try to read machine-id on Linux
    use std::fs;

    // Try systemd machine-id first
    if let Ok(machine_id) = fs::read_to_string("/etc/machine-id") {
        return Ok(machine_id.trim().to_string());
    }

    // Try dbus machine-id as fallback
    if let Ok(machine_id) = fs::read_to_string("/var/lib/dbus/machine-id") {
        return Ok(machine_id.trim().to_string());
    }

    // As a last resort, try to get the first MAC address
    get_linux_mac_address()
}

#[cfg(target_os = "linux")]
fn get_linux_mac_address() -> Result<String, String> {
    let output = Command::new("ip")
        .args(&["link", "show"])
        .output()
        .map_err(|e| format!("Failed to execute ip command: {}", e))?;

    if !output.status.success() {
        return Err("Failed to get network interfaces".to_string());
    }

    let output_str = String::from_utf8_lossy(&output.stdout);

    // Find the first non-loopback MAC address
    for line in output_str.lines() {
        if line.contains("link/ether") {
            if let Some(mac) = line.split_whitespace().nth(1) {
                // Skip loopback addresses
                if mac != "00:00:00:00:00:00" {
                    return Ok(mac.to_string());
                }
            }
        }
    }

    Err("Could not find a valid MAC address".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_hash_consistency() {
        // Test that device hash is consistent across calls
        let hash1 = get_device_hash().expect("Should get device hash");
        let hash2 = get_device_hash().expect("Should get device hash");

        assert_eq!(hash1, hash2, "Device hash should be consistent");
        assert_eq!(hash1.len(), 64, "SHA256 hash should be 64 characters");
    }

    #[test]
    fn test_device_hash_format() {
        let hash = get_device_hash().expect("Should get device hash");

        // Check that it's a valid hex string
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(hash.len(), 64);
    }
}
