#![allow(dead_code)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tauri::Manager;
use tokio::process::Command;

// On Windows ensure spawned console apps (ffmpeg/ffprobe) don't flash a console window
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt as _;
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[cfg(target_os = "windows")]
const FFMPEG_CANDIDATES: &[&str] = &["ffmpeg.exe", "ffmpeg-x86_64-pc-windows-msvc.exe"];
#[cfg(not(target_os = "windows"))]
const FFMPEG_CANDIDATES: &[&str] = &["ffmpeg", "ffmpeg-aarch64-apple-darwin"];

#[cfg(target_os = "windows")]
const FFPROBE_CANDIDATES: &[&str] = &["ffprobe.exe", "ffprobe-x86_64-pc-windows-msvc.exe"];
#[cfg(not(target_os = "windows"))]
const FFPROBE_CANDIDATES: &[&str] = &["ffprobe", "ffprobe-aarch64-apple-darwin"];

fn resolve_binary(app: &AppHandle, names: &[&str], label: &str) -> Result<PathBuf, String> {
    let is_store_install = crate::commands::distribution::is_store_install();
    let search_dirs = collect_search_dirs(
        app.path().resource_dir().ok(),
        std::env::current_exe().ok(),
        std::env::current_dir().ok(),
        is_store_install,
    );
    let mut tried = Vec::new();

    if is_store_install {
        log::debug!(
            "Skipping development and PATH {} search paths for Microsoft Store install",
            label
        );
    }

    log::debug!("Searching for {} in directories: {:?}", label, search_dirs);

    for dir in &search_dirs {
        for name in names {
            let candidate = dir.join(name);
            if candidate.exists() {
                return Ok(candidate);
            }
            tried.push(candidate);
        }
    }

    if !is_store_install {
        if let Some(path_env) = std::env::var_os("PATH") {
            log::debug!("{} not found in sidecar directories, scanning PATH", label);
            for dir in std::env::split_paths(&path_env) {
                for name in names {
                    let candidate = dir.join(name);
                    if candidate.exists() {
                        return Ok(candidate);
                    }
                    tried.push(candidate);
                }
            }
        }
    }

    let searched: Vec<String> = tried.iter().map(|p| p.display().to_string()).collect();
    Err(format!(
        "{} binary not found. Searched: {}",
        label,
        searched.join(", ")
    ))
}

fn collect_search_dirs(
    resource_dir: Option<PathBuf>,
    exe_path: Option<PathBuf>,
    cwd: Option<PathBuf>,
    is_store_install: bool,
) -> Vec<PathBuf> {
    let mut seen_dirs = HashSet::new();
    let mut search_dirs = Vec::new();

    if let Some(resource_dir) = resource_dir {
        push_search_dir(&mut search_dirs, &mut seen_dirs, resource_dir.clone());
        push_search_dir(
            &mut search_dirs,
            &mut seen_dirs,
            resource_dir.join("sidecar").join("ffmpeg").join("dist"),
        );
        // On macOS, externalBin are placed under Contents/MacOS; include that sibling of Resources.
        #[cfg(target_os = "macos")]
        if let Some(contents_dir) = resource_dir.parent() {
            push_search_dir(&mut search_dirs, &mut seen_dirs, contents_dir.join("MacOS"));
        }
    }

    if let Some(exe_dir) = exe_path.and_then(|path| path.parent().map(Path::to_path_buf)) {
        // Search the executable directory itself, where MSIX-packaged sidecars are staged.
        push_search_dir(&mut search_dirs, &mut seen_dirs, exe_dir.clone());
        push_search_dir(
            &mut search_dirs,
            &mut seen_dirs,
            exe_dir.join("sidecar").join("ffmpeg").join("dist"),
        );
        push_search_dir(
            &mut search_dirs,
            &mut seen_dirs,
            exe_dir
                .join("Resources")
                .join("sidecar")
                .join("ffmpeg")
                .join("dist"),
        );

        if !is_store_install {
            let mut dir_opt = exe_dir.parent();
            while let Some(dir) = dir_opt {
                push_search_dir(&mut search_dirs, &mut seen_dirs, dir.to_path_buf());
                push_search_dir(
                    &mut search_dirs,
                    &mut seen_dirs,
                    dir.join("sidecar").join("ffmpeg").join("dist"),
                );
                push_search_dir(
                    &mut search_dirs,
                    &mut seen_dirs,
                    dir.join("Resources")
                        .join("sidecar")
                        .join("ffmpeg")
                        .join("dist"),
                );
                dir_opt = dir.parent();
            }
        }
    }

    if !is_store_install {
        if let Some(cwd) = cwd {
            push_search_dir(
                &mut search_dirs,
                &mut seen_dirs,
                cwd.join("sidecar").join("ffmpeg").join("dist"),
            );
            push_search_dir(
                &mut search_dirs,
                &mut seen_dirs,
                cwd.join("..").join("sidecar").join("ffmpeg").join("dist"),
            );
        }
    }

    search_dirs
}

fn push_search_dir(search_dirs: &mut Vec<PathBuf>, seen_dirs: &mut HashSet<PathBuf>, dir: PathBuf) {
    if seen_dirs.insert(dir.clone()) {
        search_dirs.push(dir);
    }
}

async fn run_ffmpeg_command(
    app: &AppHandle,
    candidates: &[&str],
    args: &[String],
    label: &str,
) -> Result<(), String> {
    let bin = resolve_binary(app, candidates, label)?;
    log::debug!(
        "Running {} from {} with args {:?}",
        label,
        bin.display(),
        args
    );
    let mut cmd = Command::new(&bin);
    cmd.args(args);
    // Hide console window on Windows
    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let status = cmd
        .status()
        .await
        .map_err(|e| format!("Failed to spawn '{}': {}", bin.display(), e))?;
    if !status.success() {
        return Err(format!("{} exited with status {:?}", label, status.code()));
    }
    Ok(())
}

async fn run_ffprobe_capture(app: &AppHandle, args: &[String]) -> Result<Vec<u8>, String> {
    let bin = resolve_binary(app, FFPROBE_CANDIDATES, "ffprobe")?;
    log::debug!(
        "Running ffprobe from {} with args {:?}",
        bin.display(),
        args
    );
    let mut cmd = Command::new(&bin);
    cmd.args(args);
    // Hide console window on Windows
    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to spawn '{}': {}", bin.display(), e))?;
    if !output.status.success() {
        return Err(format!(
            "ffprobe exited with status {:?}, stderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output.stdout)
}

pub async fn probe_json(app: &AppHandle, input: &Path) -> Result<serde_json::Value, String> {
    let args: Vec<String> = vec![
        "-v".into(),
        "quiet".into(),
        "-print_format".into(),
        "json".into(),
        "-show_format".into(),
        "-show_streams".into(),
        input.to_string_lossy().to_string(),
    ];
    let out = run_ffprobe_capture(app, &args).await?;
    serde_json::from_slice(&out).map_err(|e| format!("Failed to parse ffprobe json: {}", e))
}

pub async fn to_wav_streaming(app: &AppHandle, input: &Path, output: &Path) -> Result<(), String> {
    // ffmpeg -y -loglevel error -vn -sn -i input -ac 1 -ar 16000 -sample_fmt s16 output
    let args: Vec<String> = vec![
        "-y".into(),
        "-loglevel".into(),
        "error".into(),
        "-hide_banner".into(),
        "-vn".into(),
        "-sn".into(),
        "-i".into(),
        input.to_string_lossy().to_string(),
        "-ac".into(),
        "1".into(),
        "-ar".into(),
        "16000".into(),
        "-sample_fmt".into(),
        "s16".into(),
        output.to_string_lossy().to_string(),
    ];
    run_ffmpeg_command(app, FFMPEG_CANDIDATES, &args, "ffmpeg").await
}

pub async fn normalize_streaming(
    app: &AppHandle,
    input: &Path,
    output: &Path,
) -> Result<(), String> {
    // For now, same as to_wav_streaming. Two-pass loudness can be added later.
    to_wav_streaming(app, input, output).await
}

pub async fn segment(
    app: &AppHandle,
    input: &Path,
    out_pattern: &Path,
    seconds: u32,
) -> Result<(), String> {
    // ffmpeg -y -loglevel error -i input -f segment -segment_time <seconds> -reset_timestamps 1 out%03d.wav
    let seg = seconds.to_string();
    let args: Vec<String> = vec![
        "-y".into(),
        "-loglevel".into(),
        "error".into(),
        "-hide_banner".into(),
        "-i".into(),
        input.to_string_lossy().to_string(),
        "-f".into(),
        "segment".into(),
        "-segment_time".into(),
        seg,
        "-reset_timestamps".into(),
        "1".into(),
        out_pattern.to_string_lossy().to_string(),
    ];
    run_ffmpeg_command(app, FFMPEG_CANDIDATES, &args, "ffmpeg").await
}

#[cfg(test)]
mod tests {
    use super::collect_search_dirs;
    use std::path::PathBuf;

    // Portable fixtures only: these tests compare PathBuf values and never touch the filesystem.
    fn path(value: &str) -> PathBuf {
        PathBuf::from(value)
    }

    #[test]
    fn store_search_dirs_exclude_development_and_parent_fallbacks() {
        let dirs = collect_search_dirs(
            Some(path("/package/Resources")),
            Some(path("/package/voicetypr.exe")),
            Some(path("/repo")),
            true,
        );

        assert!(dirs.contains(&path("/package/Resources")));
        assert!(dirs.contains(&path("/package/sidecar/ffmpeg/dist")));
        assert!(!dirs.contains(&path("/repo/sidecar/ffmpeg/dist")));
        assert!(!dirs.contains(&path("/sidecar/ffmpeg/dist")));
    }

    #[test]
    fn direct_search_dirs_include_development_fallbacks() {
        let dirs = collect_search_dirs(
            Some(path("/package/Resources")),
            Some(path("/package/voicetypr.exe")),
            Some(path("/repo")),
            false,
        );

        assert!(dirs.contains(&path("/repo/sidecar/ffmpeg/dist")));
        assert!(dirs.contains(&path("/sidecar/ffmpeg/dist")));
    }
}
