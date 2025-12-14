use sha2::{Digest, Sha256};
use std::process::Command;

#[cfg(target_os = "windows")]
use std::process::Output;

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

#[cfg(target_os = "windows")]
trait WindowsCommandRunner {
    fn run(&self, program: &str, args: &[&str], timeout_ms: u64) -> Result<Output, String>;
}

#[cfg(target_os = "windows")]
struct RealWindowsCommandRunner;

#[cfg(target_os = "windows")]
impl WindowsCommandRunner for RealWindowsCommandRunner {
    fn run(&self, program: &str, args: &[&str], timeout_ms: u64) -> Result<Output, String> {
        use std::io::Read;
        use std::os::windows::process::CommandExt;
        use std::process::Stdio;
        use std::thread;
        use std::time::{Duration, Instant};

        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut child = Command::new(program)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to execute {}: {}", program, e))?;

        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| format!("Failed to capture {} stdout", program))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| format!("Failed to capture {} stderr", program))?;

        let stdout_handle = thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stdout.read_to_end(&mut buf);
            buf
        });
        let stderr_handle = thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stderr.read_to_end(&mut buf);
            buf
        });

        let timeout = Duration::from_millis(timeout_ms);
        let start = Instant::now();

        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if start.elapsed() >= timeout {
                        let _ = child.kill();
                        break child
                            .wait()
                            .map_err(|e| format!("Failed to wait for {} after kill: {}", program, e))?;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    let _ = child.kill();
                    return Err(format!("Failed to wait for {}: {}", program, e));
                }
            }
        };

        let stdout_buf = stdout_handle.join().unwrap_or_default();
        let stderr_buf = stderr_handle.join().unwrap_or_default();

        Ok(Output {
            status,
            stdout: stdout_buf,
            stderr: stderr_buf,
        })
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
    get_windows_uuid_with_runner(&RealWindowsCommandRunner)
}

#[cfg(target_os = "windows")]
fn get_windows_uuid_with_runner(runner: &dyn WindowsCommandRunner) -> Result<String, String> {
    fn normalize_uuid(value: &str) -> Option<String> {
        let trimmed = value
            .trim()
            .trim_matches(&['{', '}', '"', '\''][..]);
        if trimmed.is_empty() {
            return None;
        }

        let lower = trimmed.to_ascii_lowercase();
        if lower == "uuid" || lower.contains("to be filled") {
            return None;
        }

        Some(trimmed.to_ascii_uppercase())
    }

    // Keep the existing path first for backward compatibility.
    // If WMIC is missing/hanging (newer Windows builds), fall back quickly.
    const WMIC_TIMEOUT_MS: u64 = 4_000;
    const PS_TIMEOUT_MS: u64 = 4_000;
    const REG_TIMEOUT_MS: u64 = 2_500;

    if let Ok(output) = runner.run("wmic", &["csproduct", "get", "UUID"], WMIC_TIMEOUT_MS) {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines().skip(1) {
                if let Some(uuid) = normalize_uuid(line) {
                    log::info!("[DeviceID] Source: wmic (csproduct UUID)");
                    return Ok(uuid);
                }
            }
        }
    }

    // Modern Windows: query CIM via PowerShell.
    log::debug!("[DeviceID] wmic failed or unavailable, trying PowerShell CIM...");
    if let Ok(output) = runner.run(
        "powershell",
        &[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-CimInstance -ClassName Win32_ComputerSystemProduct).UUID",
        ],
        PS_TIMEOUT_MS,
    ) {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines() {
                if let Some(uuid) = normalize_uuid(line) {
                    log::info!("[DeviceID] Source: PowerShell (Get-CimInstance)");
                    return Ok(uuid);
                }
            }
        }
    }

    // Final fallback: MachineGuid from registry.
    log::debug!("[DeviceID] PowerShell failed or unavailable, trying registry MachineGuid...");
    if let Ok(output) = runner.run(
        "reg",
        &[
            "query",
            "HKLM\\SOFTWARE\\Microsoft\\Cryptography",
            "/v",
            "MachineGuid",
        ],
        REG_TIMEOUT_MS,
    ) {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines() {
                if !line.contains("MachineGuid") {
                    continue;
                }

                if let Some(rest) = line.split("REG_SZ").nth(1) {
                    if let Some(uuid) = normalize_uuid(rest) {
                        log::info!("[DeviceID] Source: Registry (MachineGuid)");
                        return Ok(uuid);
                    }
                }
            }
        }
    }

    log::error!("[DeviceID] All sources failed: wmic, PowerShell, and registry");
    Err("Could not determine machine UUID".to_string())
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
    #[cfg(not(target_os = "windows"))]
    fn test_device_hash_consistency() {
        // Test that device hash is consistent across calls
        let hash1 = get_device_hash().expect("Should get device hash");
        let hash2 = get_device_hash().expect("Should get device hash");

        assert_eq!(hash1, hash2, "Device hash should be consistent");
        assert_eq!(hash1.len(), 64, "SHA256 hash should be 64 characters");
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_device_hash_format() {
        let hash = get_device_hash().expect("Should get device hash");

        // Check that it's a valid hex string
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(hash.len(), 64);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn windows_uuid_falls_back_when_wmic_unavailable() {
        use std::cell::RefCell;
        use std::os::windows::process::ExitStatusExt;
        use std::process::ExitStatus;

        struct StubRunner {
            responses: RefCell<Vec<Result<Output, String>>>,
        }

        impl WindowsCommandRunner for StubRunner {
            fn run(&self, _program: &str, _args: &[&str], _timeout_ms: u64) -> Result<Output, String> {
                self.responses
                    .borrow_mut()
                    .remove(0)
            }
        }

        let ok = |stdout: &str| Output {
            status: ExitStatus::from_raw(0),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        };

        let runner = StubRunner {
            responses: RefCell::new(vec![
                Err("wmic not found".to_string()),
                Ok(ok("{550E8400-E29B-41D4-A716-446655440000}\r\n")),
            ]),
        };

        let uuid = get_windows_uuid_with_runner(&runner).expect("should fall back to PowerShell");
        assert_eq!(uuid, "550E8400-E29B-41D4-A716-446655440000");
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn windows_uuid_falls_back_to_registry_when_powershell_fails() {
        use std::cell::RefCell;
        use std::os::windows::process::ExitStatusExt;
        use std::process::ExitStatus;

        struct StubRunner {
            responses: RefCell<Vec<Result<Output, String>>>,
        }

        impl WindowsCommandRunner for StubRunner {
            fn run(&self, _program: &str, _args: &[&str], _timeout_ms: u64) -> Result<Output, String> {
                self.responses
                    .borrow_mut()
                    .remove(0)
            }
        }

        let ok = |stdout: &str| Output {
            status: ExitStatus::from_raw(0),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        };

        let fail = || Output {
            status: ExitStatus::from_raw(1),
            stdout: Vec::new(),
            stderr: b"error".to_vec(),
        };

        let runner = StubRunner {
            responses: RefCell::new(vec![
                Ok(fail()),
                Ok(fail()),
                Ok(ok(
                    "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Cryptography\r\n    MachineGuid    REG_SZ    123e4567-e89b-12d3-a456-426614174000\r\n",
                )),
            ]),
        };

        let uuid = get_windows_uuid_with_runner(&runner).expect("should fall back to registry");
        assert_eq!(uuid, "123E4567-E89B-12D3-A456-426614174000");
    }
}
