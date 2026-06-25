#![allow(dead_code)]

use super::error::ParakeetError;
use super::messages::{ParakeetCommand, ParakeetResponse};
use log::{debug, error, trace, warn};
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
        Err(_) => {
            let Some(payload) = extract_json_payload(raw) else {
                return Err(ParakeetError::InvalidResponse);
            };
            match serde_json::from_str::<ParakeetResponse>(payload) {
                Ok(response) => {
                    debug!("Recovered Parakeet response from a noisy sidecar line");
                    Ok(response)
                }
                Err(_) => Err(ParakeetError::InvalidResponse),
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

/// Cancellable command dispatch shared by
/// [`ParakeetClient::send_with_progress_and_cancel`]: runs the first attempt,
/// and on `Terminated` (when the user did NOT cancel) respawns and retries once.
/// EVERY attempt is wrapped in [`timed_request`] — including the cancel-flag
/// path — so a sidecar that stays alive but stops responding still hits the
/// command deadline instead of hanging forever.
///
/// `make_request(is_retry, cancel)` produces the deadline-bounded sidecar
/// future (or a stand-in under test); the caller owns sidecar lifecycle around
/// it. Extracted so the deadline-on-cancel-path invariant is unit-testable
/// without spawning a real process.
async fn dispatch_cancellable<F, Fut>(
    command: &ParakeetCommand,
    cancel_flag: Option<Arc<AtomicBool>>,
    mut make_request: F,
) -> Result<ParakeetResponse, ParakeetError>
where
    F: FnMut(bool, Option<Arc<AtomicBool>>) -> Fut,
    Fut: std::future::Future<Output = Result<ParakeetResponse, ParakeetError>>,
{
    // First attempt — always deadline-bounded, even with a cancel flag. This was
    // the bug: the cancel path used to call the sidecar directly, so a live but
    // unresponsive sidecar could hang indefinitely while still polling cancel.
    let response = timed_request(command, make_request(false, cancel_flag.clone())).await;
    match response {
        Err(ParakeetError::Terminated)
            if !cancel_flag
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::Relaxed)) =>
        {
            // The sidecar died but the user did not cancel: respawn once and
            // retry. The retry shared the same bug class, so it is bounded too.
            timed_request(command, make_request(true, cancel_flag.clone())).await
        }
        other => other,
    }
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
                        error!(
                            "Failed to parse Parakeet sidecar stdout protocol line ({} bytes)",
                            trimmed.len()
                        );
                        return Err(err);
                    }
                    trace!("Parakeet sidecar: {}", trimmed);
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
        progress_callback: F,
    ) -> Result<ParakeetResponse, ParakeetError>
    where
        F: FnMut(f32, Option<&str>),
    {
        // The progress callback is shared across the (at most two) sequential
        // attempts. Wrap it so each attempt future can borrow it mutably without
        // holding one long-lived `&mut` across both `make_request` invocations —
        // the two attempts never overlap, so the lock is uncontended.
        let progress = Arc::new(tokio::sync::Mutex::new(progress_callback));
        let make_request = move |is_retry: bool, cancel: Option<Arc<AtomicBool>>| {
            let progress = progress.clone();
            async move {
                let mut guard = self.ensure(app).await?;
                if is_retry {
                    // Previous attempt's sidecar died (Terminated): kill it so
                    // `ensure` respawns a fresh process.
                    Self::clear_sidecar(&mut guard);
                    drop(guard);
                    guard = self.ensure(app).await?;
                }
                let response = match guard.as_mut() {
                    Some(sidecar) => {
                        let mut cb = progress.lock().await;
                        sidecar
                            .request_with_progress_and_cancel(command, Some(&mut *cb), cancel)
                            .await
                    }
                    None => Err(ParakeetError::Terminated),
                };
                response
            }
        };

        // Delegate the dispatch: it wraps BOTH attempts in `timed_request`
        // (deadline enforced even on the cancel path) and retries once on
        // Terminated-while-not-cancelled. This centralises the deadline-on-cancel
        // invariant so it is unit-testable without spawning a real process.
        let response = dispatch_cancellable(command, cancel_flag, make_request).await;

        // Lifecycle: kill the sidecar when it is left unusable — deadline
        // exceeded (the process may be wedged) or the user cancelled (the
        // sidecar's cancel loop already tried to kill it; clear for a clean slate
        // so the next command spawns fresh).
        let clear_after = match &response {
            Err(ParakeetError::Timeout { .. }) => true,
            Err(ParakeetError::SidecarError { code, .. }) => code.as_str() == "cancelled",
            _ => false,
        };
        if clear_after {
            let mut guard = self.inner.write().await;
            Self::clear_sidecar(&mut guard);
        }

        response
    }

    pub async fn shutdown(&self) {
        if let Some(sidecar) = self.inner.write().await.take() {
            sidecar.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        dispatch_cancellable, extract_json_payload, parse_response_line, request_with_timeout,
    };
    use crate::parakeet::error::ParakeetError;
    use crate::parakeet::messages::{
        ParakeetCommand, ParakeetResponse, SHORT_REQUEST_TIMEOUT_SECS,
    };
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
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
    #[tokio::test(start_paused = true)]
    async fn cancellable_dispatch_enforces_deadline_on_first_attempt_with_cancel_flag() {
        // Reproduces the bug: send_with_progress_and_cancel used to call the
        // sidecar directly (no timed_request) when a cancel_flag was present, so a
        // sidecar that stayed alive but never responded hung forever while still
        // polling cancel. The dispatch must wrap the FIRST attempt in a deadline
        // even with a cancel flag.
        //
        // Fails on the pre-fix code: without the wrap, `make_request`'s pending
        // future never resolves, the outer guard elapses, and `.expect` panics.
        let command = ParakeetCommand::Status {};
        let cancel = Some(Arc::new(AtomicBool::new(false)));
        let make_request = |_is_retry: bool, _cancel: Option<Arc<AtomicBool>>| async move {
            // A sidecar that stays alive but never sends a protocol line.
            std::future::pending::<Result<ParakeetResponse, ParakeetError>>().await
        };

        let result = tokio::time::timeout(
            Duration::from_secs(command.request_timeout_secs() * 2),
            dispatch_cancellable(&command, cancel, make_request),
        )
        .await;

        let err = result
            .expect("dispatch hung — the cancel path's first attempt is not deadline-bounded")
            .expect_err("expected a typed Timeout, not a response");
        let expected = command.request_timeout_secs();
        assert!(matches!(
            err,
            ParakeetError::Timeout { operation, timeout_secs }
                if operation == "status" && timeout_secs == expected
        ));
        assert_eq!(expected, SHORT_REQUEST_TIMEOUT_SECS);
    }

    #[tokio::test(start_paused = true)]
    async fn cancellable_dispatch_enforces_deadline_on_retry_with_cancel_flag() {
        // The retry branch shared the same bug: when the first attempt returned
        // Terminated (sidecar died) the retry also skipped timed_request. With a
        // cancel flag present, a retry that never responds must still hit the
        // deadline rather than hang. Fails on the pre-fix code via the same
        // outer-guard panic as the first-attempt case.
        let command = ParakeetCommand::Status {};
        let cancel = Some(Arc::new(AtomicBool::new(false)));
        let make_request = |is_retry: bool, _cancel: Option<Arc<AtomicBool>>| async move {
            if is_retry {
                std::future::pending::<Result<ParakeetResponse, ParakeetError>>().await
            } else {
                Err(ParakeetError::Terminated)
            }
        };

        let result = tokio::time::timeout(
            Duration::from_secs(command.request_timeout_secs() * 2),
            dispatch_cancellable(&command, cancel, make_request),
        )
        .await;

        let err = result
            .expect("dispatch hung on retry — the cancel path's retry is not deadline-bounded")
            .expect_err("expected a typed Timeout, not a response");
        assert!(matches!(
            err,
            ParakeetError::Timeout { operation, .. } if operation == "status"
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn cancellable_dispatch_preserves_cancel_polling() {
        // The added deadline must NOT mask cancellation: when the (simulated)
        // sidecar honors the cancel flag and aborts, the dispatch surfaces the
        // cancellation promptly instead of waiting the full deadline. Proves the
        // wrap preserves cancel polling rather than always timing out.
        let command = ParakeetCommand::Status {};
        let cancel = Some(Arc::new(AtomicBool::new(true))); // already cancelled
        let make_request = |_is_retry: bool, cancel: Option<Arc<AtomicBool>>| async move {
            if cancel.as_ref().is_some_and(|f| f.load(Ordering::Relaxed)) {
                Err(ParakeetError::SidecarError {
                    code: "cancelled".to_string(),
                    message: "Cancelled by user".to_string(),
                })
            } else {
                std::future::pending::<Result<ParakeetResponse, ParakeetError>>().await
            }
        };

        let started = tokio::time::Instant::now();
        let err = dispatch_cancellable(&command, cancel, make_request)
            .await
            .expect_err("expected cancellation, not a response");
        // Returned promptly (well before the deadline) — cancel polling wins.
        assert!(started.elapsed() < Duration::from_secs(command.request_timeout_secs()));
        assert!(matches!(
            err,
            ParakeetError::SidecarError { code, .. } if code == "cancelled"
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

    #[test]
    fn parse_response_line_accepts_transcription() {
        let raw = r#"{"type":"transcription","text":"Hello world","segments":[],"language":"en","duration":1.25}"#;
        let response = parse_response_line(raw).expect("expected valid transcription response");
        match response {
            ParakeetResponse::Transcription { text, duration, .. } => {
                assert_eq!(text, "Hello world");
                assert!((duration.unwrap() - 1.25_f32).abs() < 1e-4);
            }
            other => panic!("unexpected response: {:?}", other),
        }
    }

    #[test]
    fn parse_response_line_rejects_banner_line() {
        let err = parse_response_line("🔄 LOAD MODEL REQUEST")
            .expect_err("expected parse failure for banner");
        assert!(matches!(err, ParakeetError::InvalidResponse));
    }

    #[test]
    fn parse_response_line_recovers_transcription_from_noisy_line() {
        let raw = r#"🔄 LOAD MODEL REQUEST {"type":"transcription","text":"Noisy","segments":[]}"#;
        let response =
            parse_response_line(raw).expect("expected recovery via extract_json_payload");
        match response {
            ParakeetResponse::Transcription { text, .. } => {
                assert_eq!(text, "Noisy");
            }
            other => panic!("unexpected response: {:?}", other),
        }
    }
}
