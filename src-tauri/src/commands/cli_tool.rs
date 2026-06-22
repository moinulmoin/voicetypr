//! Install/uninstall/status for the `voicetypr` command-line launcher.
//!
//! macOS: writes a tiny shell shim to `/usr/local/bin/voicetypr` that `exec`s the in-bundle,
//! notarized binary by its absolute path (the VS Code `code` pattern). We never copy or
//! hardlink the signed Mach-O out of the bundle — Gatekeeper kills a relocated signed binary.
//! The shim survives in-place app updates because the bundle path is stable.
//!
//! Windows: the executable is already named `voicetypr.exe`, so exposing the command is just
//! a matter of putting its install directory on PATH. We do this from the app (no installer
//! changes): add/remove the directory in `HKCU\Environment\Path`, which is user-writable (no
//! elevation) and avoids the classic NSIS PATH-truncation data-loss bug, then broadcast
//! `WM_SETTINGCHANGE` so newly-opened terminals pick it up.

use serde::Serialize;

#[cfg(target_os = "macos")]
const MACOS_SHIM_PATH: &str = "/usr/local/bin/voicetypr";

#[derive(Debug, Clone, Serialize)]
pub struct CliToolStatus {
    /// `voicetypr` is reachable from a terminal (shim installed / install dir on PATH).
    pub installed: bool,
    /// Install/uninstall can be managed from inside the app. False only on unsupported
    /// platforms, in which case the UI should stay informational.
    pub manageable: bool,
    /// Where the command lives / how to invoke it, when known.
    pub path: Option<String>,
}

#[tauri::command]
pub fn cli_tool_status() -> Result<CliToolStatus, String> {
    #[cfg(target_os = "macos")]
    {
        Ok(macos::status())
    }
    #[cfg(target_os = "windows")]
    {
        Ok(windows_path::status())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Ok(CliToolStatus {
            installed: false,
            manageable: false,
            path: None,
        })
    }
}

#[tauri::command]
pub async fn install_cli_tool() -> Result<CliToolStatus, String> {
    #[cfg(target_os = "macos")]
    {
        tokio::task::spawn_blocking(macos::install)
            .await
            .map_err(|e| format!("Install task failed: {e}"))??;
        Ok(macos::status())
    }
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(windows_path::install)
            .await
            .map_err(|e| format!("Install task failed: {e}"))??;
        Ok(windows_path::status())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err("Installing the voicetypr command is not supported on this platform.".to_string())
    }
}

#[tauri::command]
pub async fn uninstall_cli_tool() -> Result<CliToolStatus, String> {
    #[cfg(target_os = "macos")]
    {
        tokio::task::spawn_blocking(macos::uninstall)
            .await
            .map_err(|e| format!("Uninstall task failed: {e}"))??;
        Ok(macos::status())
    }
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(windows_path::uninstall)
            .await
            .map_err(|e| format!("Uninstall task failed: {e}"))??;
        Ok(windows_path::status())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err("Uninstalling the voicetypr command is not supported on this platform.".to_string())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{CliToolStatus, MACOS_SHIM_PATH};
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::process::Command;

    pub fn status() -> CliToolStatus {
        let installed = Path::new(MACOS_SHIM_PATH).exists();
        CliToolStatus {
            installed,
            manageable: true,
            path: installed.then(|| MACOS_SHIM_PATH.to_string()),
        }
    }

    pub fn install() -> Result<(), String> {
        let exe =
            std::env::current_exe().map_err(|e| format!("Cannot resolve app executable: {e}"))?;
        let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
        let exe_str = exe
            .to_str()
            .ok_or("App executable path is not valid UTF-8")?;

        // App Translocation gives a quarantined app a random, read-only path that vanishes;
        // a shim pointing there would break. Tell the user to install to /Applications first.
        if exe_str.contains("/AppTranslocation/") {
            return Err(
                "Move VoiceTypr to your Applications folder before installing the command."
                    .to_string(),
            );
        }

        let shim = build_shim(exe_str);

        // Write the shim to a temp file with std I/O so the exe path is never parsed by a
        // shell, then place it. The temp/destination paths are fixed and contain no user
        // input, keeping the privileged step injection-free.
        let tmp = std::env::temp_dir().join("voicetypr-cli-shim");
        {
            let mut f =
                std::fs::File::create(&tmp).map_err(|e| format!("Cannot stage command: {e}"))?;
            f.write_all(shim.as_bytes())
                .map_err(|e| format!("Cannot stage command: {e}"))?;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("Cannot stage command: {e}"))?;
        }
        let tmp_str = tmp.to_str().ok_or("Temp path is not valid UTF-8")?;

        // Fast path: /usr/local/bin already writable (typical on Homebrew machines).
        let result = if try_install_direct(tmp_str).is_ok() {
            Ok(())
        } else {
            // Otherwise escalate with a single administrator prompt.
            install_with_admin(tmp_str)
        };
        let _ = std::fs::remove_file(&tmp);
        result
    }

    pub fn uninstall() -> Result<(), String> {
        if !Path::new(MACOS_SHIM_PATH).exists() {
            return Ok(());
        }
        if std::fs::remove_file(MACOS_SHIM_PATH).is_ok() {
            return Ok(());
        }
        run_osascript(&format!("rm -f '{MACOS_SHIM_PATH}'"))
    }

    fn try_install_direct(tmp: &str) -> Result<(), String> {
        std::fs::create_dir_all("/usr/local/bin").map_err(|e| e.to_string())?;
        std::fs::copy(tmp, MACOS_SHIM_PATH).map_err(|e| e.to_string())?;
        std::fs::set_permissions(MACOS_SHIM_PATH, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn install_with_admin(tmp: &str) -> Result<(), String> {
        run_osascript(&format!(
            "mkdir -p /usr/local/bin && cp '{tmp}' '{MACOS_SHIM_PATH}' && chmod 755 '{MACOS_SHIM_PATH}'"
        ))
    }

    /// Run a /bin/sh command with administrator privileges via osascript (shows the standard
    /// macOS authentication dialog). `shell_script` must use only fixed/controlled paths.
    fn run_osascript(shell_script: &str) -> Result<(), String> {
        let apple = format!(
            "do shell script \"{}\" with administrator privileges",
            applescript_escape(shell_script)
        );
        let out = Command::new("osascript")
            .arg("-e")
            .arg(&apple)
            .output()
            .map_err(|e| format!("Failed to run osascript: {e}"))?;
        if out.status.success() {
            return Ok(());
        }
        let err = String::from_utf8_lossy(&out.stderr);
        if err.contains("-128") || err.contains("User canceled") {
            Err("Authorization was cancelled.".to_string())
        } else {
            Err(format!("Failed to install command: {}", err.trim()))
        }
    }

    fn build_shim(exe: &str) -> String {
        format!(
            "#!/bin/sh\n# VoiceTypr CLI launcher (managed by VoiceTypr; reinstall from Settings). Do not edit.\nexec \"{}\" \"$@\"\n",
            sh_double_quote_escape(exe)
        )
    }

    /// Escape a string for inclusion inside an AppleScript double-quoted literal.
    fn applescript_escape(s: &str) -> String {
        s.replace('\\', "\\\\").replace('"', "\\\"")
    }

    /// Escape a path for inclusion inside a `"..."` literal in /bin/sh.
    fn sh_double_quote_escape(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('`', "\\`")
    }
}

#[cfg(target_os = "windows")]
mod windows_path {
    use super::CliToolStatus;
    use winreg::enums::{RegType, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::{RegKey, RegValue};

    pub fn status() -> CliToolStatus {
        CliToolStatus {
            installed: check_installed().unwrap_or(false),
            manageable: true,
            path: Some("voicetypr".to_string()),
        }
    }

    fn check_installed() -> Option<bool> {
        let dir = install_dir().ok()?;
        let env = open_env(false).ok()?;
        let (current, _) = read_path(&env)?;
        Some(path_contains_dir(&current, &dir))
    }

    pub fn install() -> Result<(), String> {
        let dir = install_dir()?;
        let env = open_env(true)?;
        let (current, vtype) = read_path(&env).unwrap_or((String::new(), RegType::REG_EXPAND_SZ));
        if path_contains_dir(&current, &dir) {
            return Ok(());
        }
        let new = if current.is_empty() {
            dir
        } else {
            format!("{};{}", current.trim_end_matches(';'), dir)
        };
        env.set_raw_value("Path", &encode_reg_sz(&new, vtype))
            .map_err(|e| format!("Cannot update PATH: {e}"))?;
        broadcast_env_change();
        Ok(())
    }

    pub fn uninstall() -> Result<(), String> {
        let dir = install_dir()?;
        let env = open_env(true)?;
        let Some((current, vtype)) = read_path(&env) else {
            return Ok(());
        };
        if !path_contains_dir(&current, &dir) {
            return Ok(());
        }
        let kept: Vec<&str> = current
            .split(';')
            .filter(|entry| !entry_matches_dir(entry, &dir))
            .collect();
        env.set_raw_value("Path", &encode_reg_sz(&kept.join(";"), vtype))
            .map_err(|e| format!("Cannot update PATH: {e}"))?;
        broadcast_env_change();
        Ok(())
    }

    /// Directory holding `voicetypr.exe` — the entry we put on PATH.
    fn install_dir() -> Result<String, String> {
        let exe =
            std::env::current_exe().map_err(|e| format!("Cannot resolve app executable: {e}"))?;
        exe.parent()
            .ok_or("App executable has no parent directory")?
            .to_str()
            .map(str::to_string)
            .ok_or_else(|| "Install path is not valid UTF-8".to_string())
    }

    fn open_env(write: bool) -> Result<RegKey, String> {
        let flags = if write {
            KEY_READ | KEY_WRITE
        } else {
            KEY_READ
        };
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags("Environment", flags)
            .map_err(|e| format!("Cannot open user environment: {e}"))
    }

    /// Read `Path` as a string, preserving its registry type (usually REG_EXPAND_SZ). Returns
    /// None when the value is absent so callers can decide the default behaviour.
    fn read_path(env: &RegKey) -> Option<(String, RegType)> {
        let raw = env.get_raw_value("Path").ok()?;
        Some((decode_reg_sz(&raw), raw.vtype))
    }

    fn decode_reg_sz(value: &RegValue) -> String {
        let units: Vec<u16> = value
            .bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
            .trim_end_matches('\u{0}')
            .to_string()
    }

    fn encode_reg_sz(value: &str, vtype: RegType) -> RegValue {
        let mut units: Vec<u16> = value.encode_utf16().collect();
        units.push(0);
        RegValue {
            bytes: units.iter().flat_map(|u| u.to_le_bytes()).collect(),
            vtype,
        }
    }

    fn path_contains_dir(path: &str, dir: &str) -> bool {
        path.split(';').any(|entry| entry_matches_dir(entry, dir))
    }

    /// Case-insensitive, trailing-slash-insensitive comparison of a single PATH entry.
    fn entry_matches_dir(entry: &str, dir: &str) -> bool {
        let normalize = |s: &str| s.trim().trim_end_matches('\\').to_string();
        let entry = normalize(entry);
        !entry.is_empty() && entry.eq_ignore_ascii_case(&normalize(dir))
    }

    fn broadcast_env_change() {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{
            SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
        };
        // UTF-16 "Environment" payload; must outlive the synchronous SendMessageTimeoutW call.
        let target: Vec<u16> = "Environment\0".encode_utf16().collect();
        // SAFETY: user32 FFI; target buffer is valid for the duration of this blocking call.
        unsafe {
            let _ = SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(target.as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                5000,
                None,
            );
        }
    }
}
