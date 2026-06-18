#![allow(dead_code)]

use super::error::ParakeetError;
use super::messages::{ParakeetCommand, ParakeetResponse};
use log::{error, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::async_runtime::{Receiver, RwLock};
use tauri::AppHandle;
use tauri_plugin_shell::{
    process::{CommandChild, CommandEvent},
    ShellExt,
};
use tokio::sync::RwLockWriteGuard;

fn extract_json_payload(raw: &str) -> Option<&str> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    (start < end).then_some(&raw[start..=end])
}

fn parse_response_line(raw: &str) -> Result<ParakeetResponse, ParakeetError> {
    match serde_json::from_str::<ParakeetResponse>(raw) {
        Ok(response) => Ok(response),
        Err(primary_error) => {
            let Some(payload) = extract_json_payload(raw) else {
                error!("Failed to parse sidecar response: {primary_error}. raw={raw}");
                return Err(ParakeetError::InvalidResponse);
            };
            match serde_json::from_str::<ParakeetResponse>(payload) {
                Ok(response) => {
                    warn!("Recovered Parakeet response from noisy stdout: {raw}");
                    Ok(response)
                }
                Err(recovery_error) => {
                    error!(
                        "Failed to parse sidecar response: {primary_error}; recovery failed: {recovery_error}. raw={raw}"
                    );
                    Err(ParakeetError::InvalidResponse)
                }
            }
        }
    }
}

async fn request_with_timeout<F>(
    operation: String,
    timeout_secs: u64,
    timeout: Duration,
    request: F,
) -> Result<ParakeetResponse, ParakeetError>
where
    F: std::future::Future<Output = Result<ParakeetResponse, ParakeetError>>,
{
    match tokio::time::timeout(timeout, request).await {
        Ok(result) => result,
        Err(_) => Err(ParakeetError::Timeout {
            operation,
            timeout_secs,
        }),
    }
}

async fn timed_request<F>(
    command: &ParakeetCommand,
    request: F,
) -> Result<ParakeetResponse, ParakeetError>
where
    F: std::future::Future<Output = Result<ParakeetResponse, ParakeetError>>,
{
    let operation = command.operation_name().to_string();
    let timeout_secs = command.request_timeout_secs();
    request_with_timeout(
        operation,
        timeout_secs,
        Duration::from_secs(timeout_secs),
        request,
    )
    .await
}

pub struct ParakeetSidecar {
    rx: Receiver<CommandEvent>,
    child: Option<CommandChild>,
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

        log::info!(
            "Spawned Parakeet sidecar pid={} name={}",
            child.pid(),
            binary_name
        );
        Ok(Self {
            rx,
            child: Some(child),
        })
    }

    pub async fn request(
        &mut self,
        command: &ParakeetCommand,
    ) -> Result<ParakeetResponse, ParakeetError> {
        self.request_with_progress_and_cancel(command, None::<&mut fn(f32, Option<&str>)>, None)
            .await
    }

    pub async fn request_with_progress_and_cancel<F>(
        &mut self,
        command: &ParakeetCommand,
        mut progress_callback: Option<&mut F>,
        cancel_flag: Option<Arc<AtomicBool>>,
    ) -> Result<ParakeetResponse, ParakeetError>
    where
        F: FnMut(f32, Option<&str>),
    {
        let mut payload = serde_json::to_string(command)?;
        payload.push('\n');
        self.child
            .as_mut()
            .ok_or(ParakeetError::Terminated)?
            .write(payload.as_bytes())
            .map_err(|e| ParakeetError::SpawnError(e.to_string()))?;

        loop {
            if cancel_flag
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::Relaxed))
            {
                if let Some(child) = self.child.take() {
                    if let Err(err) = child.kill() {
                        warn!("Failed to kill Parakeet sidecar during cancellation: {err:?}");
                    }
                }
                return Err(ParakeetError::SidecarError {
                    code: "cancelled".to_string(),
                    message: "Cancelled by user".to_string(),
                });
            }

            let event = if cancel_flag.is_some() {
                tokio::select! {
                    event = self.rx.recv() => event,
                    _ = tokio::time::sleep(Duration::from_millis(100)) => continue,
                }
            } else {
                self.rx.recv().await
            };

            let Some(event) = event else {
                break;
            };

            let (line_bytes, from_stdout) = match event {
                CommandEvent::Stdout(line) => (line, true),
                CommandEvent::Stderr(line) => (line, false),
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
                _ => continue,
            };

            let text = String::from_utf8_lossy(&line_bytes);
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }

            // The sidecar redirects stdout->stderr around native CoreML calls, so a
            // protocol response (load/transcribe result or status) can surface on
            // stderr instead of stdout. Parse responses from EITHER stream; a
            // non-protocol stderr line is only a diagnostic log.
            match parse_response_line(trimmed) {
                Ok(ParakeetResponse::Error { code, message, .. }) => {
                    return Err(ParakeetError::SidecarError { code, message });
                }
                Ok(ParakeetResponse::Progress { progress, phase }) => {
                    if let Some(callback) = progress_callback.as_deref_mut() {
                        callback(progress, phase.as_deref());
                    }
                }
                Ok(response) => return Ok(response),
                Err(err) => {
                    if from_stdout {
                        return Err(err);
                    }
                    warn!("Parakeet sidecar stderr: {}", trimmed);
                }
            }
        }

        Err(ParakeetError::Terminated)
    }

    pub fn kill(self) {
        if let Some(child) = self.child {
            if let Err(err) = child.kill() {
                warn!("Failed to kill Parakeet sidecar: {err:?}");
            }
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

    fn clear_sidecar(guard: &mut RwLockWriteGuard<'_, Option<ParakeetSidecar>>) {
        if let Some(sidecar) = guard.take() {
            sidecar.kill();
        }
    }

    pub async fn send(
        &self,
        app: &AppHandle,
        command: &ParakeetCommand,
    ) -> Result<ParakeetResponse, ParakeetError> {
        let mut guard = self.ensure(app).await?;
        let response = match guard.as_mut() {
            Some(sidecar) => timed_request(command, sidecar.request(command)).await,
            None => return Err(ParakeetError::Terminated),
        };

        match response {
            Err(ParakeetError::Timeout { .. }) => {
                Self::clear_sidecar(&mut guard);
                response
            }
            Err(ParakeetError::Terminated) => {
                Self::clear_sidecar(&mut guard);
                drop(guard);
                let mut guard = self.ensure(app).await?;
                let response = match guard.as_mut() {
                    Some(sidecar) => timed_request(command, sidecar.request(command)).await,
                    None => Err(ParakeetError::Terminated),
                };
                if matches!(response, Err(ParakeetError::Timeout { .. })) {
                    Self::clear_sidecar(&mut guard);
                }
                response
            }
            other => other,
        }
    }

    pub async fn send_with_progress_and_cancel<F>(
        &self,
        app: &AppHandle,
        command: &ParakeetCommand,
        cancel_flag: Option<Arc<AtomicBool>>,
        mut progress_callback: F,
    ) -> Result<ParakeetResponse, ParakeetError>
    where
        F: FnMut(f32, Option<&str>),
    {
        let mut guard = self.ensure(app).await?;
        let response = match guard.as_mut() {
            Some(sidecar) if cancel_flag.is_some() => {
                sidecar
                    .request_with_progress_and_cancel(
                        command,
                        Some(&mut progress_callback),
                        cancel_flag.clone(),
                    )
                    .await
            }
            Some(sidecar) => {
                timed_request(
                    command,
                    sidecar.request_with_progress_and_cancel(
                        command,
                        Some(&mut progress_callback),
                        None,
                    ),
                )
                .await
            }
            None => return Err(ParakeetError::Terminated),
        };

        match response {
            Err(ParakeetError::Timeout { .. }) => {
                Self::clear_sidecar(&mut guard);
                response
            }
            Err(ParakeetError::Terminated)
                if !cancel_flag
                    .as_ref()
                    .is_some_and(|flag| flag.load(Ordering::Relaxed)) =>
            {
                Self::clear_sidecar(&mut guard);
                drop(guard);
                let mut guard = self.ensure(app).await?;
                let response = match guard.as_mut() {
                    Some(sidecar) if cancel_flag.is_some() => {
                        sidecar
                            .request_with_progress_and_cancel(
                                command,
                                Some(&mut progress_callback),
                                cancel_flag,
                            )
                            .await
                    }
                    Some(sidecar) => {
                        timed_request(
                            command,
                            sidecar.request_with_progress_and_cancel(
                                command,
                                Some(&mut progress_callback),
                                None,
                            ),
                        )
                        .await
                    }
                    None => Err(ParakeetError::Terminated),
                };
                if matches!(response, Err(ParakeetError::Timeout { .. })) {
                    Self::clear_sidecar(&mut guard);
                }
                response
            }
            Err(ParakeetError::SidecarError { code, message }) if code == "cancelled" => {
                Self::clear_sidecar(&mut guard);
                Err(ParakeetError::SidecarError { code, message })
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

#[cfg(test)]
mod tests {
    use super::{extract_json_payload, parse_response_line, request_with_timeout};
    use crate::parakeet::error::ParakeetError;
    use crate::parakeet::messages::{ParakeetResponse, SHORT_REQUEST_TIMEOUT_SECS};
    use std::time::Duration;

    #[test]
    fn extract_json_payload_returns_object_slice() {
        let raw = r#"noise before {"type":"status","loadedModel":"parakeet-tdt-0.6b-v2","modelVersion":"v2"}"#;
        assert_eq!(
            extract_json_payload(raw),
            Some(r#"{"type":"status","loadedModel":"parakeet-tdt-0.6b-v2","modelVersion":"v2"}"#)
        );
    }

    #[test]
    fn parse_response_line_accepts_clean_json() {
        let raw = r#"{"type":"status","loadedModel":"parakeet-tdt-0.6b-v2","modelVersion":"v2"}"#;
        let response = parse_response_line(raw).expect("expected valid response");

        match response {
            ParakeetResponse::Status {
                loaded_model,
                model_version,
                ..
            } => {
                assert_eq!(loaded_model.as_deref(), Some("parakeet-tdt-0.6b-v2"));
                assert_eq!(model_version.as_deref(), Some("v2"));
            }
            other => panic!("unexpected response: {:?}", other),
        }
    }

    #[test]
    fn parse_response_line_recovers_json_after_noisy_prefix() {
        let raw = r#"E5RT encountered an STL exception. {"type":"status","loadedModel":"parakeet-tdt-0.6b-v2","modelVersion":"v2"}"#;
        let response = parse_response_line(raw).expect("expected recovered response");

        match response {
            ParakeetResponse::Status {
                loaded_model,
                model_version,
                ..
            } => {
                assert_eq!(loaded_model.as_deref(), Some("parakeet-tdt-0.6b-v2"));
                assert_eq!(model_version.as_deref(), Some("v2"));
            }
            other => panic!("unexpected response: {:?}", other),
        }
    }

    #[test]
    fn parse_response_line_accepts_progress_events() {
        let raw = r#"{"type":"progress","progress":0.42,"phase":"downloading 1/3"}"#;
        let response = parse_response_line(raw).expect("expected valid response");

        match response {
            ParakeetResponse::Progress { progress, phase } => {
                assert!((progress - 0.42).abs() < f32::EPSILON);
                assert_eq!(phase.as_deref(), Some("downloading 1/3"));
            }
            other => panic!("unexpected response: {:?}", other),
        }
    }

    #[tokio::test]
    async fn request_with_timeout_returns_typed_timeout_for_pending_receive() {
        let started = tokio::time::Instant::now();
        let err = request_with_timeout(
            "status".to_string(),
            SHORT_REQUEST_TIMEOUT_SECS,
            Duration::from_millis(10),
            std::future::pending::<Result<ParakeetResponse, ParakeetError>>(),
        )
        .await
        .expect_err("expected timeout");

        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(matches!(
            err,
            ParakeetError::Timeout {
                operation,
                timeout_secs: SHORT_REQUEST_TIMEOUT_SECS,
            } if operation == "status"
        ));
    }

    #[test]
    fn parse_response_line_accepts_diarization_events() {
        let raw = r#"{"type":"diarization","segments":[{"speakerId":"speaker_1","start":0.5,"end":2.25}]}"#;
        let response = parse_response_line(raw).expect("expected valid response");

        match response {
            ParakeetResponse::Diarization { segments } => {
                assert_eq!(segments.len(), 1);
                assert_eq!(segments[0].speaker_id, "speaker_1");
                assert!((segments[0].start - 0.5).abs() < f32::EPSILON);
                assert!((segments[0].end - 2.25).abs() < f32::EPSILON);
            }
            other => panic!("unexpected response: {:?}", other),
        }
    }

    #[test]
    fn parse_response_line_rejects_non_json_output() {
        let err = parse_response_line("definitely not json").expect_err("expected parse failure");
        assert!(matches!(err, ParakeetError::InvalidResponse));
    }
}
