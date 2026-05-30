#![cfg_attr(not(target_os = "windows"), allow(dead_code, unused_imports))]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::async_runtime::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt as _;
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);

#[cfg(target_os = "windows")]
const SIDECAR_CANDIDATES: &[&str] = &[
    "whisper-vulkan-sidecar.exe",
    "whisper-vulkan-sidecar-x86_64-pc-windows-msvc.exe",
];

#[cfg(not(target_os = "windows"))]
const SIDECAR_CANDIDATES: &[&str] = &["whisper-vulkan-sidecar"];

#[derive(Clone, Debug, Serialize)]
pub struct AccelerationRuntimeStatus {
    pub mode: String,
    pub effective_backend: String,
    pub gpu_available: Option<bool>,
    pub message: String,
    pub last_error: Option<String>,
}

impl Default for AccelerationRuntimeStatus {
    fn default() -> Self {
        Self {
            mode: "auto".to_string(),
            effective_backend: "unknown".to_string(),
            gpu_available: None,
            message: "GPU acceleration has not been tested yet.".to_string(),
            last_error: None,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SidecarRequest<'a> {
    Probe {
        id: u64,
        model_path: &'a str,
    },
    Transcribe {
        id: u64,
        model_path: &'a str,
        audio_path: &'a str,
        language: Option<&'a str>,
        translate: bool,
    },
}

impl SidecarRequest<'_> {
    fn id(&self) -> u64 {
        match self {
            Self::Probe { id, .. } | Self::Transcribe { id, .. } => *id,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SidecarResponse {
    Health {
        id: u64,
        ok: bool,
        backend: String,
    },
    Probe {
        id: u64,
        ok: bool,
        backend: String,
        load_time_ms: u128,
    },
    Transcription {
        id: u64,
        ok: bool,
        backend: String,
        text: String,
        inference_time_ms: u128,
    },
    Shutdown {
        id: u64,
        ok: bool,
        backend: String,
    },
    Error {
        id: u64,
        ok: bool,
        code: String,
        message: String,
    },
}

impl SidecarResponse {
    fn id(&self) -> u64 {
        match self {
            Self::Health { id, .. }
            | Self::Probe { id, .. }
            | Self::Transcription { id, .. }
            | Self::Shutdown { id, .. }
            | Self::Error { id, .. } => *id,
        }
    }

    fn ok(&self) -> bool {
        match self {
            Self::Health { ok, .. }
            | Self::Probe { ok, .. }
            | Self::Transcription { ok, .. }
            | Self::Shutdown { ok, .. }
            | Self::Error { ok, .. } => *ok,
        }
    }
}

struct GpuSidecarProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
}

impl GpuSidecarProcess {
    async fn spawn(app: &AppHandle) -> Result<Self, String> {
        let path = resolve_sidecar_binary(app)?;
        log::info!("Spawning Whisper Vulkan sidecar: {}", path.display());

        let mut command = Command::new(&path);
        command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        #[cfg(target_os = "windows")]
        {
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command
            .spawn()
            .map_err(|err| format!("failed to spawn Vulkan sidecar: {err}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Vulkan sidecar stdin was not captured".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Vulkan sidecar stdout was not captured".to_string())?;

        if let Some(stderr) = child.stderr.take() {
            tauri::async_runtime::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    log::warn!("Whisper Vulkan sidecar stderr: {}", line);
                }
            });
        }

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout).lines(),
        })
    }

    async fn request(&mut self, request: &SidecarRequest<'_>) -> Result<SidecarResponse, String> {
        let mut payload = serde_json::to_vec(request)
            .map_err(|err| format!("failed to encode Vulkan sidecar request: {err}"))?;
        payload.push(b'\n');

        self.stdin
            .write_all(&payload)
            .await
            .map_err(|err| format!("failed to write Vulkan sidecar request: {err}"))?;
        self.stdin
            .flush()
            .await
            .map_err(|err| format!("failed to flush Vulkan sidecar request: {err}"))?;

        let line = tokio::time::timeout(REQUEST_TIMEOUT, self.stdout.next_line())
            .await
            .map_err(|_| "Vulkan sidecar request timed out".to_string())?
            .map_err(|err| format!("failed to read Vulkan sidecar response: {err}"))?
            .ok_or_else(|| "Vulkan sidecar exited before responding".to_string())?;

        serde_json::from_str::<SidecarResponse>(&line).map_err(|err| {
            format!(
                "failed to parse Vulkan sidecar response: {err}; response_bytes={}",
                line.len()
            )
        })
    }

    async fn kill_and_wait(&mut self) {
        if let Err(err) = self.child.start_kill() {
            log::debug!("Failed to kill Whisper Vulkan sidecar: {err}");
            return;
        }

        match tokio::time::timeout(Duration::from_secs(5), self.child.wait()).await {
            Ok(Ok(status)) => log::debug!("Whisper Vulkan sidecar exited after kill: {status}"),
            Ok(Err(err)) => log::debug!("Failed waiting for Whisper Vulkan sidecar exit: {err}"),
            Err(_) => log::warn!("Timed out waiting for Whisper Vulkan sidecar to exit after kill"),
        }
    }
}

pub struct GpuSidecarClient {
    next_id: AtomicU64,
    process: AsyncMutex<Option<GpuSidecarProcess>>,
    status: AsyncRwLock<AccelerationRuntimeStatus>,
}

impl Default for GpuSidecarClient {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuSidecarClient {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            process: AsyncMutex::new(None),
            status: AsyncRwLock::new(AccelerationRuntimeStatus::default()),
        }
    }

    pub async fn status(&self) -> AccelerationRuntimeStatus {
        self.status.read().await.clone()
    }

    pub async fn set_cpu_status(&self, mode: &str, message: impl Into<String>) {
        *self.status.write().await = AccelerationRuntimeStatus {
            mode: mode.to_string(),
            effective_backend: "cpu".to_string(),
            gpu_available: None,
            message: message.into(),
            last_error: None,
        };
    }

    pub async fn probe(
        &self,
        app: &AppHandle,
        model_path: &Path,
        mode: &str,
    ) -> Result<(), String> {
        let model_path = model_path
            .to_str()
            .ok_or_else(|| format!("Model path contains invalid UTF-8: {:?}", model_path))?;
        let id = self.next_request_id();
        match self
            .send(app, &SidecarRequest::Probe { id, model_path })
            .await
        {
            Ok(SidecarResponse::Probe {
                ok: true,
                backend,
                load_time_ms,
                ..
            }) => {
                *self.status.write().await = AccelerationRuntimeStatus {
                    mode: mode.to_string(),
                    effective_backend: backend,
                    gpu_available: Some(true),
                    message: format!(
                        "GPU acceleration is available (model loaded in {load_time_ms}ms)."
                    ),
                    last_error: None,
                };
                Ok(())
            }
            Ok(SidecarResponse::Error { code, message, .. }) => {
                let error = format!("{code}: {message}");
                self.record_gpu_error(mode, &error).await;
                Err(error)
            }
            Ok(other) => {
                let error = format!("unexpected Vulkan probe response: {other:?}");
                self.record_gpu_error(mode, &error).await;
                Err(error)
            }
            Err(error) => {
                self.record_gpu_error(mode, &error).await;
                Err(error)
            }
        }
    }

    pub async fn transcribe(
        &self,
        app: &AppHandle,
        model_path: &Path,
        audio_path: &Path,
        language: Option<&str>,
        translate: bool,
        mode: &str,
    ) -> Result<String, String> {
        let model_path = model_path
            .to_str()
            .ok_or_else(|| format!("Model path contains invalid UTF-8: {:?}", model_path))?;
        let audio_path = audio_path
            .to_str()
            .ok_or_else(|| format!("Audio path contains invalid UTF-8: {:?}", audio_path))?;
        let id = self.next_request_id();

        match self
            .send(
                app,
                &SidecarRequest::Transcribe {
                    id,
                    model_path,
                    audio_path,
                    language,
                    translate,
                },
            )
            .await
        {
            Ok(SidecarResponse::Transcription {
                ok: true,
                backend,
                text,
                inference_time_ms,
                ..
            }) => {
                *self.status.write().await = AccelerationRuntimeStatus {
                    mode: mode.to_string(),
                    effective_backend: backend,
                    gpu_available: Some(true),
                    message: format!(
                        "Last transcription used GPU acceleration ({inference_time_ms}ms)."
                    ),
                    last_error: None,
                };
                Ok(text)
            }
            Ok(SidecarResponse::Error { code, message, .. }) => {
                let error = format!("{code}: {message}");
                if code == "context_failed" {
                    self.record_gpu_error(mode, &error).await;
                }
                Err(error)
            }
            Ok(other) => {
                let error = format!("unexpected Vulkan transcription response: {other:?}");
                self.record_gpu_error(mode, &error).await;
                Err(error)
            }
            Err(error) => {
                self.record_gpu_error(mode, &error).await;
                Err(error)
            }
        }
    }

    async fn send(
        &self,
        app: &AppHandle,
        request: &SidecarRequest<'_>,
    ) -> Result<SidecarResponse, String> {
        let mut guard = self.process.lock().await;
        if guard.is_none() {
            guard.replace(GpuSidecarProcess::spawn(app).await?);
        }

        let result = match guard.as_mut() {
            Some(process) => process.request(request).await,
            None => Err("Vulkan sidecar was not started".to_string()),
        };

        let response = match result {
            Ok(response) => response,
            Err(error) => {
                if let Some(process) = guard.as_mut() {
                    process.kill_and_wait().await;
                }
                guard.take();
                return Err(error);
            }
        };

        if response.id() != request.id() {
            if let Some(process) = guard.as_mut() {
                process.kill_and_wait().await;
            }
            guard.take();
            return Err(format!(
                "Vulkan sidecar response id mismatch: expected {}, got {}",
                request.id(),
                response.id()
            ));
        }

        if !response.ok() {
            return Ok(response);
        }
        Ok(response)
    }

    async fn record_gpu_error(&self, mode: &str, error: &str) {
        *self.status.write().await = AccelerationRuntimeStatus {
            mode: mode.to_string(),
            effective_backend: "cpu".to_string(),
            gpu_available: Some(false),
            message: "GPU acceleration is unavailable; VoiceTypr is using CPU mode.".to_string(),
            last_error: Some(error.to_string()),
        };
    }

    fn next_request_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
}

fn resolve_sidecar_binary(app: &AppHandle) -> Result<PathBuf, String> {
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
        push_dir(
            resource_dir
                .join("sidecar")
                .join("whisper-vulkan")
                .join("dist"),
        );
    }

    if let Ok(exe_path) = std::env::current_exe() {
        let mut dir_opt = exe_path.parent();
        while let Some(dir) = dir_opt {
            push_dir(dir.to_path_buf());
            push_dir(dir.join("sidecar").join("whisper-vulkan").join("dist"));
            push_dir(
                dir.join("Resources")
                    .join("sidecar")
                    .join("whisper-vulkan")
                    .join("dist"),
            );
            dir_opt = dir.parent();
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        push_dir(cwd.join("sidecar").join("whisper-vulkan").join("dist"));
        push_dir(
            cwd.join("..")
                .join("sidecar")
                .join("whisper-vulkan")
                .join("dist"),
        );
    }

    for dir in &search_dirs {
        for name in SIDECAR_CANDIDATES {
            let candidate = dir.join(name);
            if candidate.exists() {
                return Ok(candidate);
            }
            tried.push(candidate);
        }
    }

    let searched = tried
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "Whisper Vulkan sidecar not found. Searched: {searched}"
    ))
}
