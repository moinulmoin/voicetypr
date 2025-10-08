use log::{error, warn};
use tauri::async_runtime::{Receiver, RwLock};
use tauri::AppHandle;
use tauri_plugin_shell::{process::{CommandChild, CommandEvent}, ShellExt};
use tokio::sync::RwLockWriteGuard;
use super::error::ParakeetError;
use super::messages::{ParakeetCommand, ParakeetResponse};

pub struct ParakeetSidecar {
    rx: Receiver<CommandEvent>,
    child: CommandChild,
}

impl ParakeetSidecar {
    pub async fn spawn(app: &AppHandle, binary_name: &str) -> Result<Self, ParakeetError> {
        // In Tauri v2, use the shell plugin and pass just the filename.
        // The externalBin entry in tauri.conf.json must include this binary.
        let (rx, child) = app
            .shell()
            .sidecar(binary_name)
            .map_err(|e| ParakeetError::SpawnError(e.to_string()))?
            .spawn()
            .map_err(|e| ParakeetError::SpawnError(e.to_string()))?;

        log::info!("Spawned Parakeet sidecar pid={} name={}", child.pid(), binary_name);
        Ok(Self { rx, child })
    }

    pub async fn request(&mut self, command: &ParakeetCommand) -> Result<ParakeetResponse, ParakeetError> {
        let mut payload = serde_json::to_string(command)?;
        payload.push('\n');
        self.child
            .write(payload.as_bytes())
            .map_err(|e| ParakeetError::SpawnError(e.to_string()))?;

        while let Some(event) = self.rx.recv().await {
            match event {
                CommandEvent::Stdout(line) => {
                    let text = String::from_utf8_lossy(&line);
                    let trimmed = text.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<ParakeetResponse>(trimmed) {
                        Ok(response) => {
                            if let ParakeetResponse::Error { code, message, .. } = &response {
                                return Err(ParakeetError::SidecarError {
                                    code: code.clone(),
                                    message: message.clone(),
                                });
                            }
                            return Ok(response);
                        }
                        Err(err) => {
                            error!("Failed to parse sidecar response: {err}. raw={trimmed}");
                            return Err(ParakeetError::InvalidResponse);
                        }
                    }
                }
                CommandEvent::Stderr(line) => {
                    warn!("Parakeet sidecar stderr: {}", String::from_utf8_lossy(&line));
                }
                CommandEvent::Terminated(payload) => {
                    error!(
                        "Parakeet sidecar terminated unexpectedly code={:?}",
                        payload.code
                    );
                    return Err(ParakeetError::Terminated);
                }
                CommandEvent::Error(err) => {
                    error!("Error from Parakeet sidecar pipe: {err}");
                    return Err(ParakeetError::SpawnError(err));
                }
                _ => {}
            }
        }

        Err(ParakeetError::Terminated)
    }

    pub fn kill(self) {
        if let Err(err) = self.child.kill() {
            warn!("Failed to kill Parakeet sidecar: {err:?}");
        }
    }
}

pub struct ParakeetClient {
    binary_name: String,
    inner: RwLock<Option<ParakeetSidecar>>,
}

impl ParakeetClient {
    pub fn new(binary_name: impl Into<String>) -> Self {
        Self {
            binary_name: binary_name.into(),
            inner: RwLock::new(None),
        }
    }

    async fn ensure(
        &self,
        app: &AppHandle,
    ) -> Result<RwLockWriteGuard<'_, Option<ParakeetSidecar>>, ParakeetError> {
        let mut guard = self.inner.write().await;
        if guard.is_none() {
            let sidecar = ParakeetSidecar::spawn(app, &self.binary_name).await?;
            guard.replace(sidecar);
        }
        Ok(guard)
    }

    pub async fn send(&self, app: &AppHandle, command: &ParakeetCommand) -> Result<ParakeetResponse, ParakeetError> {
        let mut guard = self.ensure(app).await?;
        let response = match guard.as_mut() {
            Some(sidecar) => sidecar.request(command).await,
            None => return Err(ParakeetError::Terminated),
        };

        match response {
            Err(ParakeetError::Terminated) => {
                let old = guard.take();
                drop(guard);
                if let Some(sidecar) = old {
                    sidecar.kill();
                }
                let mut guard = self.ensure(app).await?;
                if let Some(sidecar) = guard.as_mut() {
                    sidecar.request(command).await
                } else {
                    Err(ParakeetError::Terminated)
                }
            }
            other => other,
        }
    }

    pub async fn shutdown(&self) {
        if let Some(sidecar) = self.inner.write().await.take() {
            sidecar.kill();
        }
    }
}
