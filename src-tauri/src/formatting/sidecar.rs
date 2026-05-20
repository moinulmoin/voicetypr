#![allow(dead_code)]

use super::messages::{FormattingCommand, FormattingResponse, PROTOCOL_VERSION};
use log::{error, warn};
use std::time::Duration;
use tauri::async_runtime::{Receiver, RwLock};
use tauri::AppHandle;
use tauri_plugin_shell::{
    process::{CommandChild, CommandEvent},
    ShellExt,
};
use thiserror::Error;
use tokio::sync::RwLockWriteGuard;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(35);

#[derive(Debug, Error)]
pub enum FormattingSidecarError {
    #[error("failed to spawn formatting sidecar: {0}")]
    Spawn(String),
    #[error("formatting sidecar terminated")]
    Terminated,
    #[error("invalid formatting sidecar response")]
    InvalidResponse,
    #[error("formatting sidecar request timed out")]
    Timeout,
    #[error("formatting sidecar returned mismatched response id")]
    MismatchedResponse,
    #[error("formatting sidecar error ({code}): {message}")]
    Sidecar {
        code: String,
        message: String,
        retryable: bool,
    },
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub struct FormattingSidecar {
    rx: Receiver<CommandEvent>,
    child: CommandChild,
    stdout_buffer: Vec<u8>,
}

impl FormattingSidecar {
    pub async fn spawn(app: &AppHandle, binary_name: &str) -> Result<Self, FormattingSidecarError> {
        let (rx, child) = app
            .shell()
            .sidecar(binary_name)
            .map_err(|e| FormattingSidecarError::Spawn(e.to_string()))?
            .spawn()
            .map_err(|e| FormattingSidecarError::Spawn(e.to_string()))?;

        log::info!(
            "Spawned formatting sidecar pid={} name={}",
            child.pid(),
            binary_name
        );
        Ok(Self {
            rx,
            child,
            stdout_buffer: Vec::new(),
        })
    }

    async fn request_inner(
        &mut self,
        command: &FormattingCommand,
    ) -> Result<FormattingResponse, FormattingSidecarError> {
        let mut payload = serde_json::to_string(command)?;
        payload.push('\n');
        self.child.write(payload.as_bytes()).map_err(|e| {
            warn!(
                "Formatting sidecar pipe write failed; treating cached child as terminated: {}",
                sanitize_log_value(&e.to_string())
            );
            FormattingSidecarError::Terminated
        })?;

        while let Some(event) = self.rx.recv().await {
            match event {
                CommandEvent::Stdout(chunk) => {
                    let responses = parse_stdout_responses(&mut self.stdout_buffer, &chunk)?;
                    for response in responses {
                        match response_matches_command(&response, command.id()) {
                            Ok(true) => {}
                            Ok(false) => {
                                let response_id = response.id().unwrap_or("<missing>");
                                warn!(
                                    "Ignoring stale formatting sidecar response id={} expected={}",
                                    response_id,
                                    command.id()
                                );
                                continue;
                            }
                            Err(err) => {
                                warn!(
                                    "Rejecting invalid formatting sidecar response; expected id={}",
                                    command.id()
                                );
                                return Err(err);
                            }
                        }

                        if let FormattingResponse::Error {
                            code,
                            message,
                            retryable,
                            ..
                        } = &response
                        {
                            return Err(FormattingSidecarError::Sidecar {
                                code: sanitize_log_value(code),
                                message: sanitize_log_value(message),
                                retryable: *retryable,
                            });
                        }

                        return Ok(response);
                    }
                }
                CommandEvent::Stderr(line) => {
                    warn!(
                        "Formatting sidecar stderr: {}",
                        sanitize_log_value(&String::from_utf8_lossy(&line))
                    );
                }
                CommandEvent::Terminated(payload) => {
                    error!(
                        "Formatting sidecar terminated unexpectedly code={:?}",
                        payload.code
                    );
                    return Err(FormattingSidecarError::Terminated);
                }
                CommandEvent::Error(err) => {
                    error!(
                        "Formatting sidecar pipe error: {}",
                        sanitize_log_value(&err)
                    );
                    return Err(FormattingSidecarError::Spawn(err));
                }
                _ => {}
            }
        }

        Err(FormattingSidecarError::Terminated)
    }

    pub async fn request(
        &mut self,
        command: &FormattingCommand,
    ) -> Result<FormattingResponse, FormattingSidecarError> {
        tokio::time::timeout(REQUEST_TIMEOUT, self.request_inner(command))
            .await
            .map_err(|_| FormattingSidecarError::Timeout)?
    }

    pub fn kill(self) {
        if let Err(err) = self.child.kill() {
            warn!("Failed to kill formatting sidecar: {err:?}");
        }
    }
}

pub struct FormattingClient {
    binary_name: String,
    inner: RwLock<Option<FormattingSidecar>>,
}

impl FormattingClient {
    pub fn new(binary_name: impl Into<String>) -> Self {
        Self {
            binary_name: binary_name.into(),
            inner: RwLock::new(None),
        }
    }

    async fn ensure(
        &self,
        app: &AppHandle,
    ) -> Result<RwLockWriteGuard<'_, Option<FormattingSidecar>>, FormattingSidecarError> {
        let mut guard = self.inner.write().await;
        if guard.is_none() {
            let sidecar = FormattingSidecar::spawn(app, &self.binary_name).await?;
            guard.replace(sidecar);
        }
        Ok(guard)
    }

    pub async fn send(
        &self,
        app: &AppHandle,
        command: &FormattingCommand,
    ) -> Result<FormattingResponse, FormattingSidecarError> {
        let mut guard = self.ensure(app).await?;
        let response = match guard.as_mut() {
            Some(sidecar) => sidecar.request(command).await,
            None => return Err(FormattingSidecarError::Terminated),
        };

        match response {
            Err(
                error @ (FormattingSidecarError::Terminated
                | FormattingSidecarError::Timeout
                | FormattingSidecarError::InvalidResponse),
            ) => {
                let old = guard.take();
                drop(guard);
                if let Some(sidecar) = old {
                    sidecar.kill();
                }

                if matches!(command, FormattingCommand::Format { .. }) {
                    return Err(error);
                }

                let mut guard = self.ensure(app).await?;
                if let Some(sidecar) = guard.as_mut() {
                    sidecar.request(command).await
                } else {
                    Err(FormattingSidecarError::Terminated)
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

fn parse_stdout_responses(
    buffer: &mut Vec<u8>,
    chunk: &[u8],
) -> Result<Vec<FormattingResponse>, FormattingSidecarError> {
    buffer.extend_from_slice(chunk);

    let mut responses = Vec::new();
    while let Some(newline_index) = buffer.iter().position(|byte| *byte == b'\n') {
        let mut line: Vec<u8> = buffer.drain(..=newline_index).collect();
        if line.last() == Some(&b'\n') {
            line.pop();
        }
        if line.last() == Some(&b'\r') {
            line.pop();
        }

        let start = line
            .iter()
            .position(|byte| !byte.is_ascii_whitespace())
            .unwrap_or(line.len());
        let end = line
            .iter()
            .rposition(|byte| !byte.is_ascii_whitespace())
            .map(|index| index + 1)
            .unwrap_or(start);
        let trimmed = &line[start..end];

        if trimmed.is_empty() {
            continue;
        }

        let response = serde_json::from_slice::<FormattingResponse>(trimmed).map_err(|err| {
            error!("Failed to parse formatting sidecar response: {err}");
            FormattingSidecarError::InvalidResponse
        })?;
        responses.push(response);
    }

    Ok(responses)
}

fn response_matches_command(
    response: &FormattingResponse,
    expected_id: &str,
) -> Result<bool, FormattingSidecarError> {
    let Some(response_id) = response.id() else {
        return Err(FormattingSidecarError::InvalidResponse);
    };

    if response_id != expected_id {
        return Ok(false);
    }

    if response.protocol_version() != Some(PROTOCOL_VERSION) {
        return Err(FormattingSidecarError::InvalidResponse);
    }

    Ok(true)
}

fn sanitize_log_value(value: &str) -> String {
    let mut out = value.to_string();
    if let Ok(re) = regex::Regex::new(r"sk-ant-[A-Za-z0-9_-]+") {
        out = re.replace_all(&out, "sk-ant-[REDACTED]").to_string();
    }
    if let Ok(re) = regex::Regex::new(r"sk-[A-Za-z0-9_-]+") {
        out = re.replace_all(&out, "sk-[REDACTED]").to_string();
    }
    if let Ok(re) = regex::Regex::new(r"(?i)bearer\s+[A-Za-z0-9._~+/=-]+") {
        out = re.replace_all(&out, "Bearer [REDACTED]").to_string();
    }
    out.chars().take(500).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_split_stdout_frame_without_emitting_partial_response() {
        let mut buffer = Vec::new();

        let first = parse_stdout_responses(
            &mut buffer,
            br#"{"type":"formatted","id":"format-1","protocolVersion":1,"ok":true,"text":"hello"#,
        )
        .expect("partial frame should not fail");
        assert!(first.is_empty());

        let second = parse_stdout_responses(
            &mut buffer,
            br#" world","provider":"openai","model":"gpt-5-nano","latencyMs":12,"usage":null}"#
                .as_ref(),
        )
        .expect("frame without newline should remain buffered");
        assert!(second.is_empty());

        let third =
            parse_stdout_responses(&mut buffer, b"\n").expect("complete frame should parse");
        assert_eq!(third.len(), 1);
        match &third[0] {
            FormattingResponse::Formatted { text, .. } => assert_eq!(text, "hello world"),
            other => panic!("expected formatted response, got {other:?}"),
        }
        assert!(buffer.is_empty());
    }

    #[test]
    fn parses_split_multibyte_utf8_frame() {
        let mut buffer = Vec::new();
        let frame = "{\"type\":\"formatted\",\"id\":\"format-utf8\",\"protocolVersion\":1,\"ok\":true,\"text\":\"hello 🌍\",\"provider\":\"openai\",\"model\":\"gpt-5-nano\",\"latencyMs\":12,\"usage\":null}\n";
        let split_at = frame
            .as_bytes()
            .windows("🌍".len())
            .position(|window| window == "🌍".as_bytes())
            .expect("test frame should contain emoji")
            + 1;

        let first = parse_stdout_responses(&mut buffer, &frame.as_bytes()[..split_at])
            .expect("partial multibyte frame should not fail");
        assert!(first.is_empty());

        let second = parse_stdout_responses(&mut buffer, &frame.as_bytes()[split_at..])
            .expect("completed multibyte frame should parse");
        assert_eq!(second.len(), 1);
        match &second[0] {
            FormattingResponse::Formatted { text, .. } => assert_eq!(text, "hello 🌍"),
            other => panic!("expected formatted response, got {other:?}"),
        }
        assert!(buffer.is_empty());
    }

    #[test]
    fn parses_multiple_stdout_frames_from_one_chunk() {
        let mut buffer = Vec::new();
        let responses = parse_stdout_responses(
            &mut buffer,
            b"{\"type\":\"ready\",\"id\":\"health-1\",\"protocolVersion\":1,\"ok\":true}\n{\"type\":\"shutdown\",\"id\":\"shutdown-1\",\"protocolVersion\":1,\"ok\":true}\n",
        )
        .expect("complete frames should parse");

        assert_eq!(responses.len(), 2);
        assert!(matches!(responses[0], FormattingResponse::Ready { .. }));
        assert!(matches!(responses[1], FormattingResponse::Shutdown { .. }));
        assert!(buffer.is_empty());
    }

    #[test]
    fn rejects_response_without_id_for_command_matching() {
        let response = FormattingResponse::Ready {
            id: None,
            protocol_version: 1,
            ok: true,
        };

        assert!(matches!(
            response_matches_command(&response, "health-1"),
            Err(FormattingSidecarError::InvalidResponse)
        ));
    }

    #[test]
    fn distinguishes_matching_and_stale_response_ids() {
        let response = FormattingResponse::Ready {
            id: Some("health-1".to_string()),
            protocol_version: 1,
            ok: true,
        };

        assert!(response_matches_command(&response, "health-1").unwrap());
        assert!(!response_matches_command(&response, "health-2").unwrap());
    }

    #[test]
    fn rejects_mismatched_protocol_version_for_command_matching() {
        let response = FormattingResponse::Ready {
            id: Some("health-1".to_string()),
            protocol_version: PROTOCOL_VERSION + 1,
            ok: true,
        };

        assert!(matches!(
            response_matches_command(&response, "health-1"),
            Err(FormattingSidecarError::InvalidResponse)
        ));
    }
}
