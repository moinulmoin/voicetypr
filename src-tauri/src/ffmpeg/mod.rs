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
    let mut tried = Vec::new();
    let mut seen_dirs = HashSet::new();
    let mut search_dirs = Vec::new();

    let mut push_dir = |dir: PathBuf| {
        if seen_dirs.insert(dir.clone()) {
            search_dirs.push(dir);
        }
    };

    if let Ok(resource_dir) = app.path().resource_dir() {
        push_dir(resource_dir.clone());
        push_dir(resource_dir.join("sidecar").join("ffmpeg").join("dist"));
        // On macOS, externalBin are placed under Contents/MacOS; include that sibling of Resources
        #[cfg(target_os = "macos")]
        if let Some(contents_dir) = resource_dir.parent() {
            let macos_dir = contents_dir.join("MacOS");
            push_dir(macos_dir);
        }
    }

    if let Ok(exe_path) = std::env::current_exe() {
        let mut dir_opt = exe_path.parent();
        while let Some(dir) = dir_opt {
            // Search the executable directory itself (e.g., .../Contents/MacOS)
            push_dir(dir.to_path_buf());
            push_dir(dir.join("sidecar").join("ffmpeg").join("dist"));
            push_dir(
                dir.join("Resources")
                    .join("sidecar")
                    .join("ffmpeg")
                    .join("dist"),
            );
            dir_opt = dir.parent();
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        push_dir(cwd.join("sidecar").join("ffmpeg").join("dist"));
        push_dir(cwd.join("..").join("sidecar").join("ffmpeg").join("dist"));
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

    let searched: Vec<String> = tried.iter().map(|p| p.display().to_string()).collect();
    Err(format!(
        "{} binary not found. Searched: {}",
        label,
        searched.join(", ")
    ))
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
