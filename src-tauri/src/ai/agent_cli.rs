//! Cold-spawn runtime for agent-CLI polish providers.
//!
//! Agent CLIs are deliberately launched as isolated, one-shot subprocesses.
//! There is no warm-session manager: every request gets a fresh process and a
//! fresh temporary working directory.
//!
//! # Spawning model
//!
//! The user's installed coding CLI is spawned headless via raw
//! `tokio::process::Command` (NOT `tauri-plugin-shell` — the shell scope governs
//! the untrusted webview; trusted Rust native code is not subject to it, and we
//! avoid a wildcard `shell:allow-execute`). Text to polish travels via **stdin**
//! where the provider supports it; the polished text is parsed from the CLI's
//! JSON output. Every child and its descendants run in a dedicated process
//! group/job, with bounded stdout/stderr drains, an isolated temporary cwd,
//! `kill_on_drop(true)`, and an explicit group kill/reap path on timeout.
//!
//! # PATH resolution
//!
//! A Finder-launched macOS GUI app inherits only `/usr/bin:/bin:/usr/sbin:/sbin`,
//! NOT the user's shell PATH — so `~/.local/bin/claude`, `~/.bun/bin/...` are
//! invisible to a naive spawn. We resolve the user's real login-shell PATH once
//! (cached via `OnceLock`, with a timeout + minimal-PATH fallback), then spawn
//! the CLI by ABSOLUTE path. `cli_tool.rs` only installs the voicetypr shim and
//! does App-Translocation handling — it has no login-shell PATH resolver to
//! reuse, so one lives here.
//!
//! # Security
//!
//! Dictated text is fed via discrete stdin/argv values — NEVER a shell string,
//! NEVER `sh -c`. Isolation flags are provider policy in `AgentCliSpec`; the
//! child also runs from an EMPTY temp cwd so it discovers no project config.

use super::contract::AiPolishRequest;
use super::error::{AiProviderError, MappedAiProviderError};
use super::providers::{PROVIDER_CLAUDE_CODE, PROVIDER_OMP, PROVIDER_PI};
use command_group::{AsyncCommandGroup, AsyncGroupChild};
use serde_json::Value;
use std::ffi::{OsStr, OsString};
#[cfg(test)]
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::OnceLock;
use std::time::Duration;
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Hard wall-clock cap for a single cold-spawn polish. The executor's own
/// `polish()` budget also bounds this (per-runtime `timeout_ms`), but a CLI can
/// wedge independently of HTTP semantics, so we hard-kill at this deadline
/// regardless and surface `Err(Timeout)` → raw-transcript fallback.
const COLD_SPAWN_TIMEOUT: Duration = Duration::from_secs(9);

/// Maximum bytes retained from either child stream. Readers continue draining
/// after this limit so a chatty child cannot deadlock on a full pipe.
const MAX_CAPTURE_BYTES: usize = 128 * 1024;

/// Maximum user-visible text copied from an untrusted CLI error.
const MAX_CLI_ERROR_CHARS: usize = 200;

/// Per-provider cold-spawn spec. Provider policy lives here rather than in
/// provider-specific branches in the process runtime.
struct AgentCliSpec {
    #[allow(dead_code)]
    provider_id: &'static str,
    binary: &'static str,
    /// Fixed argv before policy args and the `--system-prompt` value.
    cold_argv_prefix: &'static [&'static str],
    /// Fixed argv after the `--system-prompt` value.
    cold_argv_suffix: &'static [&'static str],
    /// Required isolation args, in provider CLI order.
    required_isolation_args: &'static [&'static str],
    /// Required capability flags that must be advertised by bounded `--help`.
    required_capability_flags: &'static [&'static str],
    /// Provider default when the selected model is empty.
    default_model: Option<&'static str>,
    /// Provider reasoning policy.
    reasoning: ReasoningPolicy,
    /// How dictated text reaches the child process.
    input_mode: InputMode,
    /// How the polished result is parsed from stdout.
    output: OutputParser,
    /// How `probe()` determines the authed state.
    auth: AuthMode,
}

/// How a provider's reasoning/thinking mode is selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReasoningPolicy {
    /// Always disable model thinking for deterministic, bounded polishing.
    AlwaysOff,
    /// Use the low Claude effort level only when the CLI advertises it.
    ClaudeEffortLowIfSupported,
}

/// How dictated text reaches the cold-spawn child.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputMode {
    /// Dictation written to child stdin, then the pipe is closed (EOF signals
    /// the child to flush). The argv holds NO dictated text. claude-code + pi.
    Stdin,
    /// Dictation appended as the LAST argv element — a discrete
    /// `Command::arg` value handed to the OS exec, NEVER a shell string, NEVER
    /// `sh -c` (so it is injection-safe regardless of content). stdin is null.
    /// omp does not read stdin in `--mode json`; positional is its input
    /// channel.
    PositionalArg,
}

/// Which stdout shape a CLI emits, selecting the parser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputParser {
    ClaudeJson,
    PiJsonl,
}

/// How `probe()` decides the `authed` flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AuthMode {
    /// Run `<bin> auth status` and parse its JSON for a logged-in account.
    RealAuthStatus,
    /// Installed => authed (optimistic). pi/omp have no clean auth command.
    Optimistic,
}

/// Capabilities discovered from Claude's bounded help output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ClaudeCapabilities {
    safe_mode: bool,
    effort: bool,
}

/// Claude Code policy. `--safe-mode` is preferred when advertised; older
/// versions fall back to `--setting-sources ""`. Credentials remain available
/// in both modes, while tools/settings are explicitly disabled.
const CLAUDE_CODE_SPEC: AgentCliSpec = AgentCliSpec {
    provider_id: PROVIDER_CLAUDE_CODE,
    binary: "claude",
    cold_argv_prefix: &["-p"],
    cold_argv_suffix: &["--output-format", "json"],
    required_isolation_args: &["--tools", "", "--strict-mcp-config", "--no-chrome"],
    required_capability_flags: &[],
    default_model: Some("haiku"),
    reasoning: ReasoningPolicy::ClaudeEffortLowIfSupported,
    input_mode: InputMode::Stdin,
    output: OutputParser::ClaudeJson,
    auth: AuthMode::RealAuthStatus,
};

/// pi policy. Required isolation flags are mandatory; if an installed pi does
/// not advertise one of them, launch is rejected rather than degraded.
const PI_SPEC: AgentCliSpec = AgentCliSpec {
    provider_id: PROVIDER_PI,
    binary: "pi",
    cold_argv_prefix: &["-p"],
    cold_argv_suffix: &[],
    required_isolation_args: &[
        "--no-tools",
        "--no-session",
        "--no-extensions",
        "--no-skills",
        "--no-prompt-templates",
        "--no-context-files",
    ],
    required_capability_flags: &[
        "--no-tools",
        "--no-session",
        "--no-extensions",
        "--no-skills",
        "--no-prompt-templates",
        "--no-context-files",
        "--thinking",
    ],
    default_model: None,
    reasoning: ReasoningPolicy::AlwaysOff,
    input_mode: InputMode::Stdin,
    output: OutputParser::PiJsonl,
    auth: AuthMode::Optimistic,
};

/// omp policy. Required isolation flags are mandatory; if an installed omp
/// does not advertise one of them, launch is rejected rather than degraded.
const OMP_SPEC: AgentCliSpec = AgentCliSpec {
    provider_id: PROVIDER_OMP,
    binary: "omp",
    cold_argv_prefix: &["-p"],
    cold_argv_suffix: &[],
    required_isolation_args: &[
        "--no-tools",
        "--no-session",
        "--no-skills",
        "--no-rules",
        "--no-extensions",
        "--no-lsp",
        "--no-title",
    ],
    required_capability_flags: &[
        "--no-tools",
        "--no-session",
        "--no-skills",
        "--no-rules",
        "--no-extensions",
        "--no-lsp",
        "--no-title",
        "--thinking",
    ],
    default_model: None,
    reasoning: ReasoningPolicy::AlwaysOff,
    input_mode: InputMode::PositionalArg,
    output: OutputParser::PiJsonl,
    auth: AuthMode::Optimistic,
};

fn spec_for(provider_id: &str) -> Option<&'static AgentCliSpec> {
    match provider_id {
        PROVIDER_CLAUDE_CODE => Some(&CLAUDE_CODE_SPEC),
        PROVIDER_PI => Some(&PI_SPEC),
        PROVIDER_OMP => Some(&OMP_SPEC),
        _ => None,
    }
}

/// Build provider argv with a selected model and bounded help capabilities.
/// Every interpolated value remains one discrete argv element.
fn cold_argv_for_model(
    spec: &AgentCliSpec,
    prompt: &str,
    selected_model: &str,
    capabilities: ClaudeCapabilities,
) -> Vec<String> {
    let mut argv: Vec<String> = spec
        .cold_argv_prefix
        .iter()
        .map(|s| (*s).to_string())
        .collect();

    if spec.provider_id == PROVIDER_CLAUDE_CODE {
        if capabilities.safe_mode {
            argv.push("--safe-mode".to_string());
        } else {
            argv.extend(["--setting-sources", ""].into_iter().map(str::to_string));
        }
    } else if !selected_model.trim().is_empty() {
        argv.extend(["--model", selected_model].into_iter().map(str::to_string));
    }

    argv.extend(
        spec.required_isolation_args
            .iter()
            .map(|s| (*s).to_string()),
    );

    if let Some(default_model) = spec.default_model {
        let model = if selected_model.trim().is_empty() {
            default_model
        } else {
            selected_model
        };
        argv.extend(["--model", model].into_iter().map(str::to_string));
    }

    match spec.reasoning {
        ReasoningPolicy::AlwaysOff => {
            argv.extend(["--thinking", "off"].into_iter().map(str::to_string));
        }
        ReasoningPolicy::ClaudeEffortLowIfSupported if capabilities.effort => {
            argv.extend(["--effort", "low"].into_iter().map(str::to_string));
        }
        ReasoningPolicy::ClaudeEffortLowIfSupported => {}
    }

    if spec.provider_id != PROVIDER_CLAUDE_CODE {
        argv.extend(["--mode", "json"].into_iter().map(str::to_string));
    }

    argv.push("--system-prompt".to_string());
    argv.push(prompt.to_string());
    argv.extend(spec.cold_argv_suffix.iter().map(|s| (*s).to_string()));
    argv
}

/// Backwards-compatible default argv helper used by pure tests and callers
/// that intentionally request the provider default model.
#[cfg(test)]
fn cold_argv(spec: &AgentCliSpec, prompt: &str) -> Vec<String> {
    cold_argv_for_model(spec, prompt, "", ClaudeCapabilities::default())
}

/// Cold-spawn runtime. Each `polish` call spawns a fresh child in a unique
/// temporary cwd; no warm session is retained.
#[derive(Clone, Default)]
pub struct AgentCliRuntime;

impl AgentCliRuntime {
    pub fn new() -> Self {
        Self
    }

    /// Polish `request.input_text` under `request.prompt`, returning the RAW
    /// model string. The executor outer loop (executor.rs) sanitizes, validates,
    /// and retries — this method does NOT duplicate that. Any spawn/timeout/parse
    /// failure returns `Err`, which the executor surfaces as `ai_error` → the
    /// existing raw-transcript fallback delivers deterministic text + toast.
    pub async fn polish(&self, request: &AiPolishRequest) -> Result<String, MappedAiProviderError> {
        let spec = spec_for(&request.provider_id)
            .ok_or_else(|| MappedAiProviderError::new(AiProviderError::UnsupportedProvider))?;

        let binary_path = resolve_binary(spec.binary)
            .await
            .ok_or_else(|| MappedAiProviderError::new(AiProviderError::UnsupportedProvider))?;
        let (claude_capabilities, help) =
            discover_capabilities_for_polish(&binary_path, spec).await;
        if !required_capabilities_present(spec, &help) {
            return Err(MappedAiProviderError::new(AiProviderError::AgentCli(
                "The installed CLI does not support the required isolation flags.".to_string(),
            )));
        }

        let prompt = request.prompt.clone();
        let input_text = request.input_text.clone();
        let input_mode = spec.input_mode;
        let output = spec.output;
        let argv = cold_argv_for_model(spec, &prompt, &request.model_id, claude_capabilities);
        cold_spawn_and_collect(&binary_path, &argv, input_mode, &input_text, output).await
    }
}

/// Owns a process group/job until it has been explicitly killed and reaped.
/// Dropping an in-flight command (executor cancellation or an outer deadline)
/// synchronously kills the whole group, then detaches a reaper task.
struct ProcessGroupGuard {
    child: Option<AsyncGroupChild>,
}

impl ProcessGroupGuard {
    fn new(child: AsyncGroupChild) -> Self {
        Self { child: Some(child) }
    }

    fn inner(&mut self) -> &mut tokio::process::Child {
        self.child
            .as_mut()
            .expect("process group must exist while command is running")
            .inner()
    }

    fn start_kill(&mut self) -> std::io::Result<()> {
        self.child
            .as_mut()
            .expect("process group must exist while command is running")
            .start_kill()
    }

    async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        self.child
            .as_mut()
            .expect("process group must exist while command is running")
            .wait()
            .await
    }

    fn disarm(&mut self) {
        let _ = self.child.take();
    }
}

impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let _ = child.start_kill();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            std::mem::drop(handle.spawn(async move {
                let _ = child.wait().await;
            }));
        }
    }
}

fn group_kill_is_settled(result: &std::io::Result<()>) -> bool {
    match result {
        Ok(()) => true,
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::InvalidInput | std::io::ErrorKind::NotFound
            ) =>
        {
            true
        }
        #[cfg(unix)]
        Err(error) if error.raw_os_error() == Some(libc::ESRCH) => true,
        Err(_) => false,
    }
}

async fn terminate_process_group(child: &mut ProcessGroupGuard) -> std::io::Result<()> {
    let kill_result = child.start_kill();
    let wait_result = child.wait().await.map(|_| ());
    if group_kill_is_settled(&kill_result) && wait_result.is_ok() {
        child.disarm();
    }
    wait_result
}

/// Capture result from an isolated child. Both streams are always drained
/// concurrently, even when only stdout is needed.
struct ProcessCapture {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[derive(Debug)]
enum ProcessFailure {
    Spawn(std::io::Error),
    Io(std::io::Error),
    Timeout,
}

/// Run a configured command from a unique temporary cwd. The `TempDir` stays
/// alive until the child has been reaped and both drain tasks have completed.
async fn run_isolated_command(
    mut command: Command,
    stdin_bytes: Option<&[u8]>,
    timeout: Duration,
) -> Result<ProcessCapture, ProcessFailure> {
    let deadline = tokio::time::Instant::now() + timeout;
    let temp_dir = TempDir::new().map_err(ProcessFailure::Io)?;
    command.current_dir(temp_dir.path());
    command.stdin(if stdin_bytes.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.kill_on_drop(true);

    let mut child = ProcessGroupGuard::new(command.group_spawn().map_err(ProcessFailure::Spawn)?);
    let stdout = match child.inner().stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = terminate_process_group(&mut child).await;
            return Err(ProcessFailure::Io(std::io::Error::other(
                "CLI stdout pipe was unavailable",
            )));
        }
    };
    let stderr = match child.inner().stderr.take() {
        Some(stderr) => stderr,
        None => {
            let _ = terminate_process_group(&mut child).await;
            return Err(ProcessFailure::Io(std::io::Error::other(
                "CLI stderr pipe was unavailable",
            )));
        }
    };
    let mut stdout_task = Some(tokio::spawn(drain_bounded(stdout)));
    let mut stderr_task = Some(tokio::spawn(drain_bounded(stderr)));

    if let Some(input) = stdin_bytes {
        let mut stdin = match child.inner().stdin.take() {
            Some(stdin) => stdin,
            None => {
                kill_and_reap(&mut child, stdout_task.take(), stderr_task.take()).await;
                return Err(ProcessFailure::Io(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "CLI stdin pipe was unavailable",
                )));
            }
        };
        match tokio::time::timeout_at(deadline, stdin.write_all(input)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                drop(stdin);
                kill_and_reap(&mut child, stdout_task.take(), stderr_task.take()).await;
                return Err(ProcessFailure::Io(error));
            }
            Err(_) => {
                drop(stdin);
                kill_and_reap(&mut child, stdout_task.take(), stderr_task.take()).await;
                return Err(ProcessFailure::Timeout);
            }
        }
        // Explicitly close stdin so providers waiting for EOF flush promptly.
        drop(stdin);
    }

    let status = match tokio::time::timeout_at(deadline, child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(error)) => {
            kill_and_reap(&mut child, stdout_task.take(), stderr_task.take()).await;
            return Err(ProcessFailure::Io(error));
        }
        Err(_) => {
            // `kill_on_drop` is defense in depth; explicitly kill and await the
            // child here so timeout never leaves a process or pipe behind.
            kill_and_reap(&mut child, stdout_task.take(), stderr_task.take()).await;
            return Err(ProcessFailure::Timeout);
        }
    };

    let stdout_result = {
        let task = stdout_task
            .as_mut()
            .expect("stdout drain task must be present before joining");
        tokio::time::timeout_at(deadline, task).await
    };
    let stdout = match stdout_result {
        Err(_) => {
            kill_and_reap(&mut child, stdout_task.take(), stderr_task.take()).await;
            return Err(ProcessFailure::Timeout);
        }
        Ok(joined) => {
            // The task's result has been consumed; remove its handle before
            // handling the result so cleanup cannot await it a second time.
            let _ = stdout_task.take();
            match joined {
                Ok(Ok(stdout)) => stdout,
                Ok(Err(error)) => {
                    kill_and_reap(&mut child, stdout_task.take(), stderr_task.take()).await;
                    return Err(ProcessFailure::Io(error));
                }
                Err(_) => {
                    kill_and_reap(&mut child, stdout_task.take(), stderr_task.take()).await;
                    return Err(ProcessFailure::Io(std::io::Error::other(
                        "CLI stdout drain task failed",
                    )));
                }
            }
        }
    };

    let stderr_result = {
        let task = stderr_task
            .as_mut()
            .expect("stderr drain task must be present before joining");
        tokio::time::timeout_at(deadline, task).await
    };
    let stderr = match stderr_result {
        Err(_) => {
            kill_and_reap(&mut child, stdout_task.take(), stderr_task.take()).await;
            return Err(ProcessFailure::Timeout);
        }
        Ok(joined) => {
            // As above, this completed handle must not be handed to cleanup.
            let _ = stderr_task.take();
            match joined {
                Ok(Ok(stderr)) => stderr,
                Ok(Err(error)) => return Err(ProcessFailure::Io(error)),
                Err(_) => {
                    return Err(ProcessFailure::Io(std::io::Error::other(
                        "CLI stderr drain task failed",
                    )));
                }
            }
        }
    };
    let _ = terminate_process_group(&mut child).await;

    drop(temp_dir);
    Ok(ProcessCapture {
        status,
        stdout,
        stderr,
    })
}

/// Drain an entire pipe while retaining only a bounded prefix.
async fn drain_bounded<R>(mut reader: R) -> std::io::Result<Vec<u8>>
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;

    let mut retained = Vec::with_capacity(MAX_CAPTURE_BYTES.min(8192));
    let mut chunk = [0_u8; 8192];
    loop {
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        if retained.len() < MAX_CAPTURE_BYTES {
            let take = (MAX_CAPTURE_BYTES - retained.len()).min(read);
            retained.extend_from_slice(&chunk[..take]);
        }
    }
    Ok(retained)
}

async fn kill_and_reap(
    child: &mut ProcessGroupGuard,
    stdout_task: Option<tokio::task::JoinHandle<std::io::Result<Vec<u8>>>>,
    stderr_task: Option<tokio::task::JoinHandle<std::io::Result<Vec<u8>>>>,
) {
    let _ = terminate_process_group(child).await;
    for task in [stdout_task, stderr_task].into_iter().flatten() {
        task.abort();
        let _ = task.await;
    }
}

/// Run a bounded `--help` capability probe. Claude falls back conservatively
/// when this optional probe fails; pi/omp then fail mandatory-capability checks.
async fn discover_capabilities_for_polish(
    binary_path: &Path,
    spec: &AgentCliSpec,
) -> (ClaudeCapabilities, Vec<u8>) {
    let mut command = Command::new(binary_path);
    command.arg("--help");
    command.env("PATH", resolved_path().await);
    command.env("ZSH_TMUX_AUTOSTART", "false");
    apply_no_window(&mut command);
    match run_isolated_command(command, None, Duration::from_secs(3)).await {
        Ok(capture) if capture.status.success() => {
            let mut help = capture.stdout;
            if !capture.stderr.is_empty() {
                help.extend_from_slice(&capture.stderr);
            }
            let claude = claude_capabilities_from_help(&help);
            (claude, help)
        }
        _ if spec.provider_id == PROVIDER_CLAUDE_CODE => {
            (ClaudeCapabilities::default(), Vec::new())
        }
        _ => (ClaudeCapabilities::default(), Vec::new()),
    }
}

fn required_capabilities_present(spec: &AgentCliSpec, help: &[u8]) -> bool {
    spec.required_capability_flags
        .iter()
        .all(|flag| help_advertises_flag(help, flag))
}

fn help_advertises_flag(help: &[u8], flag: &str) -> bool {
    let text = String::from_utf8_lossy(help);
    text.split_whitespace().any(|token| {
        let token =
            token.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_');
        token == flag
            || token
                .strip_prefix(flag)
                .is_some_and(|rest| rest.starts_with('='))
    })
}

fn claude_capabilities_from_help(help: &[u8]) -> ClaudeCapabilities {
    ClaudeCapabilities {
        safe_mode: help_advertises_flag(help, "--safe-mode"),
        effort: help_advertises_flag(help, "--effort"),
    }
}

fn apply_no_window(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = command;
    }
}

/// Spawn the binary, deliver `input_text` per `input_mode`, wait for exit, and
/// parse the polish result from stdout. A non-zero exit is always an error,
/// even when stdout looks like a successful JSON response.
async fn cold_spawn_and_collect(
    binary_path: &Path,
    argv: &[String],
    input_mode: InputMode,
    input_text: &str,
    output: OutputParser,
) -> Result<String, MappedAiProviderError> {
    let mut command = Command::new(binary_path);
    command.args(argv);
    if input_mode == InputMode::PositionalArg {
        // Positional input is one discrete OS argument, never a shell string.
        command.arg(input_text);
    }
    command.env("PATH", resolved_path().await);
    command.env("ZSH_TMUX_AUTOSTART", "false");
    apply_no_window(&mut command);

    let capture = run_isolated_command(
        command,
        (input_mode == InputMode::Stdin).then_some(input_text.as_bytes()),
        COLD_SPAWN_TIMEOUT,
    )
    .await
    .map_err(map_process_failure)?;

    if !capture.status.success() {
        let detail = extract_process_error(output, &capture.stdout, &capture.stderr);
        log::warn!(
            "agent-cli polish exited non-zero ({}); returning bounded CLI error",
            capture.status
        );
        return Err(MappedAiProviderError::new(AiProviderError::AgentCli(
            detail,
        )));
    }

    parse_cli_output(output, &capture.stdout)
}

fn map_process_failure(failure: ProcessFailure) -> MappedAiProviderError {
    match failure {
        ProcessFailure::Spawn(error) => MappedAiProviderError::new(map_spawn_error(&error)),
        ProcessFailure::Io(error) => MappedAiProviderError::new(AiProviderError::AgentCli(
            sanitize_cli_error(&error.to_string()),
        )),
        ProcessFailure::Timeout => MappedAiProviderError::new(AiProviderError::Timeout),
    }
}

fn extract_process_error(output: OutputParser, stdout: &[u8], stderr: &[u8]) -> String {
    if let Ok(stderr_text) = std::str::from_utf8(stderr) {
        if !stderr_text.trim().is_empty() {
            let detail = sanitize_cli_error(stderr_text);
            if !detail.is_empty() {
                return detail;
            }
        }
    }
    match parse_cli_output(output, stdout) {
        Ok(message) => {
            let detail = sanitize_cli_error(&message);
            if !detail.is_empty() {
                return detail;
            }
        }
        Err(error) => {
            if let AiProviderError::AgentCli(message) = error.error {
                let detail = sanitize_cli_error(&message);
                if !detail.is_empty() {
                    return detail;
                }
            }
        }
    }
    let stdout_text = String::from_utf8_lossy(stdout);
    let stdout_text = stdout_text.trim();
    if !stdout_text.is_empty() {
        let detail = sanitize_cli_error(stdout_text);
        if !detail.is_empty() {
            return detail;
        }
    }
    "The CLI could not complete this request.".to_string()
}

/// Remove control/ANSI bytes and cap untrusted CLI text at 200 characters.
fn sanitize_cli_error(raw: &str) -> String {
    let mut clean = String::new();
    let mut in_ansi = false;
    for ch in raw.chars() {
        if ch == '\u{1b}' {
            in_ansi = true;
            continue;
        }
        if in_ansi {
            if ch.is_ascii_alphabetic() {
                in_ansi = false;
            }
            continue;
        }
        if ch.is_control() {
            if matches!(ch, '\n' | '\r' | '\t') {
                clean.push(' ');
            }
        } else {
            clean.push(ch);
        }
        if clean.chars().count() >= MAX_CLI_ERROR_CHARS {
            break;
        }
    }
    let normalized = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    normalized
        .chars()
        .take(MAX_CLI_ERROR_CHARS)
        .collect::<String>()
}

fn redact_probe_paths(detail: &str) -> String {
    detail
        .split_whitespace()
        .filter(|token| {
            let slash_count = token.chars().filter(|ch| *ch == '/').count();
            let windows_drive_path = token.len() >= 3
                && token.as_bytes().get(1) == Some(&b':')
                && token
                    .as_bytes()
                    .get(2)
                    .is_some_and(|byte| *byte == b'\\' || *byte == b'/');
            let unc_path = token.starts_with("\\\\");
            !(token.starts_with("~/") || slash_count >= 2 || windows_drive_path || unc_path)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Deadline-bound a generic operation. Process helpers perform their own
/// explicit child kill/reap; this utility remains useful for non-process
/// callers and pure timeout tests.
#[cfg(test)]
async fn deadline_bounded<F, Fut>(
    timeout: Duration,
    make_operation: F,
) -> Result<String, MappedAiProviderError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<String, MappedAiProviderError>>,
{
    match tokio::time::timeout(timeout, make_operation()).await {
        Ok(result) => result,
        Err(_) => Err(MappedAiProviderError::new(AiProviderError::Timeout)),
    }
}
/// Select the parser for a CLI's stdout shape. claude emits a single JSON
/// object; pi/omp emit a JSONL event stream.
fn parse_cli_output(output: OutputParser, stdout: &[u8]) -> Result<String, MappedAiProviderError> {
    match output {
        OutputParser::ClaudeJson => parse_claude_json(stdout),
        OutputParser::PiJsonl => parse_pi_jsonl(stdout),
    }
}

/// Parse claude's `--output-format json` result. It emits an object with
/// `.result` (string) or `.content` (string / array of
/// `{type:"text", text:"..."}` blocks). A noisy prefix line (update notice,
/// shell banner) before the JSON is tolerated by slicing between the first `{`
/// and the last `}` — mirrors parakeet's `extract_json_payload`.
fn parse_claude_json(stdout: &[u8]) -> Result<String, MappedAiProviderError> {
    let text = String::from_utf8_lossy(stdout);
    let payload = extract_json_payload(&text).unwrap_or(text.as_ref());
    let value: Value = serde_json::from_str(payload)
        .map_err(|_| MappedAiProviderError::new(AiProviderError::BadResponse))?;

    // The CLI signals a fatal (non-result) outcome with `is_error: true` and/or
    // a `subtype`/`type` of `error`. We must NOT return `result` as polished
    // text — that would type the CLI's OWN message (e.g. "Not logged in ·
    // Please run /login") at the user's cursor. Surface the CLI's words as Err
    // (AgentCli) so the executor falls back to the raw transcript and the UI
    // toasts the cause the CLI itself printed.
    if is_cli_error_marker(&value) {
        return Err(MappedAiProviderError::new(AiProviderError::AgentCli(
            cli_error_message(&value),
        )));
    }

    if let Some(result) = value.get("result").and_then(Value::as_str) {
        return Ok(result.to_string());
    }
    if let Some(content) = value.get("content") {
        if let Some(s) = content.as_str() {
            return Ok(s.to_string());
        }
        if let Some(arr) = content.as_array() {
            let mut parts = Vec::new();
            for item in arr {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    parts.push(text.to_string());
                }
            }
            if !parts.is_empty() {
                return Ok(parts.join("\n"));
            }
        }
    }
    Err(MappedAiProviderError::new(AiProviderError::BadResponse))
}

/// True when the CLI's JSON marks a fatal (non-result) outcome. Claude emits
/// `is_error: true`; some CLIs use a `subtype`/`type` of `error` instead.
fn is_cli_error_marker(value: &Value) -> bool {
    if value
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return true;
    }
    let marker = value
        .get("subtype")
        .or_else(|| value.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("");
    marker.eq_ignore_ascii_case("error")
}

/// Extract the CLI's own error text from its JSON. Prefers `result` (Claude's
/// fatal-result message), then `error`/`message` fields, else a short generic.
fn cli_error_message(value: &Value) -> String {
    for field in ["result", "error", "message"] {
        if let Some(s) = value.get(field).and_then(Value::as_str) {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return sanitize_cli_error(trimmed);
            }
        }
    }
    "The CLI could not complete this request.".to_string()
}

fn extract_json_payload(raw: &str) -> Option<&str> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    // `.then_some` is eager — it would compute the slice before the guard
    // runs, panicking when `}` precedes `{`. The closure form `.then` is lazy.
    (start < end).then(|| &raw[start..=end])
}
/// Parse pi/omp's `--mode json` result. It emits a JSONL EVENT STREAM — one
/// JSON object per line — NOT claude's single `{"result":...}` object. Event
/// types include `session`, `agent_start`, `turn_start`, `message_start`,
/// `message_update`, `message_end` (user + assistant), `turn_end`, `agent_end`.
/// The polished text is the ASSISTANT message's text: we scan every line, keep
/// the LAST non-empty `message.content[].text` (type=="text") of any event
/// whose `message.role == "assistant"`. (`message_end`, `message_update`, and
/// `turn_end` all carry the full final assistant text; taking the last non-empty
/// one yields the completed polished output.) Unparseable lines are skipped. If
/// no assistant text is found → `Err(BadResponse)`. Best-effort: a line carrying
/// an error event/message surfaces `Err(AgentCli(message))` (its own words).
fn parse_pi_jsonl(stdout: &[u8]) -> Result<String, MappedAiProviderError> {
    let text = String::from_utf8_lossy(stdout);
    let mut last_assistant_text: Option<String> = None;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        // Cheap skip: only JSON object lines start with `{`. Blank lines, shell
        // banners, or an update notice before the stream are ignored.
        if line.is_empty() || !line.starts_with('{') {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue; // tolerate stray non-JSON lines without failing the parse
        };
        // Best-effort error detection BEFORE extracting text, so an error event
        // is never mistaken for a (possibly-present) assistant message.
        if let Some(message) = pi_error_message(&value) {
            return Err(MappedAiProviderError::new(AiProviderError::AgentCli(
                message,
            )));
        }
        if let Some(message_text) = assistant_message_text(&value) {
            if !message_text.trim().is_empty() {
                last_assistant_text = Some(message_text);
            }
        }
    }
    last_assistant_text.ok_or_else(|| MappedAiProviderError::new(AiProviderError::BadResponse))
}

/// Extract the joined text of an assistant message from a JSONL event line.
/// Returns `Some` only when `message.role == "assistant"` and `message.content`
/// is an array of `{type:"text", text:"..."}` blocks with at least one text
/// block; the blocks are joined with `\n` (mirrors parse_claude_json's content
/// handling). Non-assistant events (user, session, agent_end) yield `None`.
fn assistant_message_text(value: &Value) -> Option<String> {
    let message = value.get("message")?;
    if message.get("role").and_then(Value::as_str) != Some("assistant") {
        return None;
    }
    let content = message.get("content")?;
    let arr = content.as_array()?;
    let mut parts = Vec::new();
    for item in arr {
        if item.get("type").and_then(Value::as_str) == Some("text") {
            if let Some(text) = item.get("text").and_then(Value::as_str) {
                parts.push(text.to_string());
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

/// Best-effort pi/omp error extraction from a JSONL event line. Triggered when
/// a line has top-level `type == "error"` OR an `error` field; the message is
/// mined from `error`/`message`/`result` string fields, else a short generic.
/// pi/omp error event shapes are not formally documented, so this only
/// short-circuits clearly-error lines — a malformed assistant reply still falls
/// through to `Err(BadResponse)` via the caller.
fn pi_error_message(value: &Value) -> Option<String> {
    let is_error_type = value
        .get("type")
        .and_then(Value::as_str)
        .map(|t| t.eq_ignore_ascii_case("error"))
        .unwrap_or(false);
    if !is_error_type && value.get("error").is_none() {
        return None;
    }
    for field in ["error", "message", "result"] {
        if let Some(s) = value.get(field).and_then(Value::as_str) {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Some(sanitize_cli_error(trimmed));
            }
        }
    }
    Some("The CLI could not complete this request.".to_string())
}

/// Map a process spawn failure to a user-facing provider error. "Not found"
/// (CLI not installed, or PATH miss) is `UnsupportedProvider` so the executor
/// surfaces the raw transcript; permission failures map to `Internal`.
fn map_spawn_error(error: &std::io::Error) -> AiProviderError {
    if error.kind() == std::io::ErrorKind::NotFound {
        AiProviderError::UnsupportedProvider
    } else {
        AiProviderError::Internal
    }
}

// ─── login-shell PATH resolution ────────────────────────────────────────────

/// Cached resolved PATH, carried as an `OsString` so the raw OS path survives
/// without UTF-8 conversion (Windows paths may carry non-UTF-8 bytes). A
/// Finder-launched macOS GUI app gets only `/usr/bin:/bin:/usr/sbin:/sbin`; we
/// replace it with the user's real login-shell PATH (or a minimal Unix
/// fallback). Resolved once, reused for every cold spawn + probe.
static RESOLVED_PATH: OnceLock<OsString> = OnceLock::new();

/// Resolution outcome used by probe state mapping. Public callers retain the
/// historical `Option<PathBuf>` wrapper, while detection can distinguish a
/// missing binary from a deliberately rejected launcher.
#[derive(Debug, Clone, PartialEq, Eq)]
enum BinaryResolution {
    Found(PathBuf),
    Missing,
    UnsafeLauncher,
}

/// Resolve all PATH candidates and continue after unsafe launchers. A rejected
/// first match must never hide a later native executable.
fn select_safe_candidate(candidates: impl IntoIterator<Item = PathBuf>) -> BinaryResolution {
    let mut saw_unsafe = false;
    for candidate in candidates {
        if is_allowed_binary(&candidate) {
            return BinaryResolution::Found(candidate);
        }
        saw_unsafe = true;
    }
    if saw_unsafe {
        BinaryResolution::UnsafeLauncher
    } else {
        BinaryResolution::Missing
    }
}

async fn resolve_binary_state(binary: &str) -> BinaryResolution {
    let path = resolved_path().await;
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir());
    let candidates = which::which_in_all(binary, Some(path), cwd)
        .map(|paths| paths.collect::<Vec<_>>())
        .unwrap_or_default();
    select_safe_candidate(candidates)
}

/// Resolve a CLI binary name to an absolute safe path against the resolved
/// PATH. Unsafe candidates are skipped rather than stopping at the first match.
pub async fn resolve_binary(binary: &str) -> Option<PathBuf> {
    match resolve_binary_state(binary).await {
        BinaryResolution::Found(path) => Some(path),
        BinaryResolution::Missing | BinaryResolution::UnsafeLauncher => None,
    }
}

/// Windows allows only native `.exe` launchers. `.com` is intentionally not
/// allowed: unlike `.exe`, it is ambiguous across PATH/PATHEXT environments.
fn is_windows_native_executable(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
}

/// Compatibility helper retained for callers/tests that only need to identify
/// the two classic Windows batch suffixes. Full resolution uses
/// `is_script_launcher`, which also rejects PowerShell, shell, and WSH forms.
#[cfg(test)]
fn is_windows_batch_shim(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    matches!(ext.to_ascii_lowercase().as_str(), "cmd" | "bat")
}

/// Script and shell launcher suffixes are never accepted as executable
/// candidates. Extensionless Unix shebang binaries remain supported.
fn is_script_launcher(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "cmd"
            | "bat"
            | "ps1"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
            | "js"
            | "jse"
            | "vbs"
            | "vbe"
            | "wsf"
            | "wsc"
            | "wsh"
            | "hta"
    )
}

fn is_allowed_binary(path: &Path) -> bool {
    if cfg!(target_os = "windows") {
        is_windows_native_executable(path)
    } else {
        !is_script_launcher(path)
    }
}

async fn resolved_path() -> OsString {
    if let Some(cached) = RESOLVED_PATH.get() {
        return cached.clone();
    }
    let resolved = resolve_resolved_path().await;
    // Race-tolerant set: whichever task won the race produced an equivalent PATH.
    let _ = RESOLVED_PATH.set(resolved.clone());
    RESOLVED_PATH.get().cloned().unwrap_or(resolved)
}

/// Compute the resolved PATH to use for cold spawns + probes.
///
/// Unix (macOS/Linux): a Finder/GUI-launched app inherits only the minimal
/// system PATH, so we probe the user's real login-shell PATH (cached), with a
/// minimal Unix fallback covering common user-bin locations when the probe
/// fails or times out.
///
/// Non-Unix (Windows): the registry PATH set by installers is already present
/// in the inherited process environment, so we take it directly — NEVER the
/// Unix login-shell fallback, whose colon-joined entries are invalid here.
#[cfg(unix)]
async fn resolve_resolved_path() -> OsString {
    resolve_login_shell_path()
        .await
        .map_or_else(|| OsString::from(fallback_path()), OsString::from)
}

#[cfg(not(unix))]
async fn resolve_resolved_path() -> OsString {
    std::env::var_os("PATH").unwrap_or_default()
}

#[cfg(unix)]
async fn resolve_login_shell_path() -> Option<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    const MARKER: &str = "VOICETYPR_PATH_PROBE_BOUNDARY";
    let script = format!("printf '%s\\n' '{MARKER}'; env; printf '%s\\n' '{MARKER}'");

    let mut command = Command::new(&shell);
    command.args(["-ilc", &script]);
    command.env("ZSH_TMUX_AUTOSTART", "false");
    apply_no_window(&mut command);

    let capture = run_isolated_command(command, None, Duration::from_secs(8))
        .await
        .ok()?;
    if !capture.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&capture.stdout);
    extract_path_between_markers(&stdout, MARKER)
}

/// Extract the `PATH=...` line from the env block emitted between the two
/// marker lines. Handles optional double-quote wrapping some shells add.
#[cfg(unix)]
fn extract_path_between_markers(stdout: &str, marker: &str) -> Option<String> {
    let mut in_block = false;
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed == marker {
            in_block = !in_block;
            continue;
        }
        if in_block {
            if let Some(rest) = line.strip_prefix("PATH=") {
                return Some(rest.trim_matches('"').to_string());
            }
        }
    }
    None
}

/// Minimal fallback PATH when the login-shell probe fails or times out.
/// Covers the common user-bin locations so a typical install is still found.
fn fallback_path() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut entries: Vec<String> = vec![
        "/usr/bin".to_string(),
        "/bin".to_string(),
        "/usr/sbin".to_string(),
        "/sbin".to_string(),
        "/opt/homebrew/bin".to_string(),
        "/usr/local/bin".to_string(),
    ];
    if !home.is_empty() {
        entries.push(format!("{home}/.local/bin"));
        entries.push(format!("{home}/.bun/bin"));
    }
    entries.join(":")
}

// ─── probe (detection) ──────────────────────────────────────────────────────

/// Probe state sent over the wire. `installed`, `authed`, and `detail` remain
/// for compatibility with existing settings clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentCliProbeState {
    Ready,
    NotAuthenticated,
    Missing,
    UnsafeLauncher,
    Incompatible,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentCliProbe {
    pub state: AgentCliProbeState,
    pub installed: bool,
    pub authed: bool,
    /// A bounded CLI-owned auth/capability detail, never a binary path.
    pub detail: String,
}

impl AgentCliProbe {
    fn unavailable(state: AgentCliProbeState) -> Self {
        Self {
            state,
            installed: false,
            authed: false,
            detail: String::new(),
        }
    }

    fn state(state: AgentCliProbeState, installed: bool, authed: bool, detail: String) -> Self {
        Self {
            state,
            installed,
            authed,
            detail: redact_probe_paths(&sanitize_cli_error(&detail)),
        }
    }
}

fn state_for_probe(
    resolution: &BinaryResolution,
    executable_ok: bool,
    compatible: bool,
    authed: bool,
) -> AgentCliProbeState {
    match resolution {
        BinaryResolution::Missing => AgentCliProbeState::Missing,
        BinaryResolution::UnsafeLauncher => AgentCliProbeState::UnsafeLauncher,
        BinaryResolution::Found(_) if !executable_ok || !compatible => {
            AgentCliProbeState::Incompatible
        }
        BinaryResolution::Found(_) if !authed => AgentCliProbeState::NotAuthenticated,
        BinaryResolution::Found(_) => AgentCliProbeState::Ready,
    }
}

/// Probe an agent-CLI provider without reading credentials or performing a
/// model completion. `--version` and bounded `--help` are local capability
/// checks; only Claude's own `auth status` is used for authentication.
pub async fn probe(provider: &str) -> AgentCliProbe {
    let Some(spec) = spec_for(provider) else {
        return AgentCliProbe::unavailable(AgentCliProbeState::Missing);
    };
    let resolution = resolve_binary_state(spec.binary).await;
    let binary_path = match &resolution {
        BinaryResolution::Found(path) => path,
        BinaryResolution::Missing => {
            return AgentCliProbe::unavailable(AgentCliProbeState::Missing)
        }
        BinaryResolution::UnsafeLauncher => {
            return AgentCliProbe::unavailable(AgentCliProbeState::UnsafeLauncher)
        }
    };

    let mut version_command = Command::new(binary_path);
    version_command.arg("--version");
    version_command.env("PATH", resolved_path().await);
    version_command.env("ZSH_TMUX_AUTOSTART", "false");
    apply_no_window(&mut version_command);
    let version = run_isolated_command(version_command, None, Duration::from_secs(5)).await;
    let version_ok = matches!(
        version.as_ref(),
        Ok(capture) if capture.status.success()
    );
    if !version_ok {
        let detail = version
            .ok()
            .map(|capture| {
                extract_process_error(OutputParser::PiJsonl, &capture.stdout, &capture.stderr)
            })
            .unwrap_or_else(|| "The installed CLI could not be executed.".to_string());
        return AgentCliProbe::state(
            state_for_probe(&resolution, false, false, false),
            true,
            false,
            detail,
        );
    }

    let (_, help) = discover_capabilities_for_polish(binary_path, spec).await;
    let compatible = required_capabilities_present(spec, &help);
    if !compatible {
        return AgentCliProbe::state(
            state_for_probe(&resolution, true, false, false),
            true,
            false,
            "The installed CLI does not support the required isolation flags.".to_string(),
        );
    }

    match spec.auth {
        AuthMode::RealAuthStatus => {
            let auth_status_raw = check_auth_status(binary_path).await;
            let authed = parse_auth_status(auth_status_raw.as_bytes());
            let detail = extract_auth_detail(&auth_status_raw);
            let state = state_for_probe(&resolution, true, true, authed);
            AgentCliProbe::state(state, true, authed, detail)
        }
        AuthMode::Optimistic => {
            let state = state_for_probe(&resolution, true, true, true);
            AgentCliProbe::state(state, true, true, String::new())
        }
    }
}

/// Run `<binary> auth status` with a short timeout and return the raw stdout.
/// Empty on non-zero exit, spawn failure, or timeout. Credential files are
/// never read.
async fn check_auth_status(binary_path: &Path) -> String {
    const AUTH_STATUS_TIMEOUT: Duration = Duration::from_secs(3);
    let path = resolved_path().await;
    match run_auth_status(binary_path, &["auth", "status"], &path, AUTH_STATUS_TIMEOUT).await {
        Some(stdout) => String::from_utf8_lossy(&stdout).trim().to_string(),
        None => String::new(),
    }
}

/// Spawn `<binary> auth status <extra>` in a unique temp cwd, bounded by
/// `timeout`, with concurrent bounded stdout/stderr drains.
async fn run_auth_status(
    binary_path: &Path,
    extra_argv: &[&str],
    path: &OsStr,
    timeout: Duration,
) -> Option<Vec<u8>> {
    let mut command = Command::new(binary_path);
    command.args(extra_argv);
    command.env("ZSH_TMUX_AUTOSTART", "false");
    command.env("PATH", path);
    apply_no_window(&mut command);
    let capture = run_isolated_command(command, None, timeout).await.ok()?;
    capture.status.success().then_some(capture.stdout)
}

/// Parse `<bin> auth status` output for a logged-in account. Returns `true`
/// ONLY when the output is JSON with `"loggedIn": true`. Logged-out payloads
/// (`"loggedIn": false`, `"is_error": true`, or plain "Not logged in" text) and
/// any unparseable output all return `false` — the safe default so a broken
/// probe never overstates auth.
fn parse_auth_status(output: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(output) else {
        return false;
    };
    // A notice line may precede the JSON; slice to the payload (parakeet
    // precedent). Plain-text output ("Not logged in") has no braces → None.
    let candidate = extract_json_payload(text).unwrap_or("");
    let Ok(value) = serde_json::from_str::<Value>(candidate) else {
        return false;
    };
    let Some(obj) = value.as_object() else {
        return false;
    };
    if obj
        .get("is_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return false;
    }
    obj.get("loggedIn")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Reduce the raw `<bin> auth status` stdout to a short, human-readable line
/// for the sign-in badge — the CLI's OWN words, not a canned string. JSON
/// payloads are mined for a message-like string field
/// (`result`/`message`/`content`/`error`); plain-text output (pi/omp-style
/// "run <bin> login") is kept verbatim. Returns empty when nothing useful is
/// present so the badge falls back to its static hint.
fn extract_auth_detail(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // JSON: pull the first non-empty message-like string field. A bare
    // `{"loggedIn":false}` with no message yields nothing useful → "".
    let candidate = extract_json_payload(trimmed).unwrap_or(trimmed);
    if let Ok(value) = serde_json::from_str::<Value>(candidate) {
        for field in ["result", "message", "error"] {
            if let Some(s) = value.get(field).and_then(Value::as_str) {
                let s = s.trim();
                if !s.is_empty() {
                    return truncate_detail(s);
                }
            }
        }
        if let Some(s) = value.get("content").and_then(Value::as_str) {
            let s = s.trim();
            if !s.is_empty() {
                return truncate_detail(s);
            }
        }
        return String::new();
    }
    // Plain text (pi/omp login guidance): keep the CLI's own line(s).
    truncate_detail(trimmed)
}

/// Cap a badge detail line so a chatty CLI can't blow up the settings card.
fn truncate_detail(s: &str) -> String {
    const MAX_LEN: usize = 200;
    if s.chars().count() <= MAX_LEN {
        return s.to_string();
    }
    let truncated: String = s.chars().take(MAX_LEN).collect();
    format!("{truncated}…")
}
/// A model exposed by an agent CLI.
///
/// Unlike catalog models, agent-CLI entries intentionally carry no token-cost
/// fields: pi and omp use the user's own subscriptions/configuration. The
/// camelCase serde names are part of the Tauri wire contract.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentCliModel {
    pub id: String,
    pub name: String,
    pub recommended: bool,
    pub reasoning: bool,
    #[serde(rename = "contextWindow")]
    pub context_window: Option<u64>,
    #[serde(rename = "sourceProvider")]
    pub source_provider: Option<String>,
    #[serde(rename = "cliDefault")]
    pub cli_default: bool,
}

const PI_MODELS_RESPONSE_ID: &str = "voicetypr-models";

fn cli_default_model() -> AgentCliModel {
    AgentCliModel {
        id: String::new(),
        name: "CLI default".to_string(),
        recommended: true,
        reasoning: false,
        context_window: None,
        source_provider: None,
        cli_default: true,
    }
}

fn curated_claude_models() -> Vec<AgentCliModel> {
    [
        ("haiku", "Haiku", true),
        ("sonnet", "Sonnet", false),
        ("opus", "Opus", false),
    ]
    .into_iter()
    .map(|(id, name, recommended)| AgentCliModel {
        id: id.to_string(),
        name: name.to_string(),
        recommended,
        reasoning: false,
        context_window: None,
        source_provider: None,
        cli_default: false,
    })
    .collect()
}

/// Parse the matching pi RPC response.
///
/// pi speaks JSONL. A response with another id is deliberately ignored rather
/// than treated as model data; callers must not accidentally display an
/// unrelated RPC response. Invalid lines are tolerated as stream noise, but a
/// payload with no matching response is a malformed response.
fn parse_pi_models(stdout: &[u8]) -> Result<Vec<AgentCliModel>, MappedAiProviderError> {
    let text = String::from_utf8_lossy(stdout);
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("id").and_then(Value::as_str) != Some(PI_MODELS_RESPONSE_ID) {
            continue;
        }
        return parse_pi_models_response(&value);
    }

    // Keep the pure parser useful for a single pretty-printed JSON object in
    // addition to pi's normal JSONL form.
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if value.get("id").and_then(Value::as_str) == Some(PI_MODELS_RESPONSE_ID) {
            return parse_pi_models_response(&value);
        }
    }
    Err(MappedAiProviderError::new(AiProviderError::BadResponse))
}

fn parse_pi_models_response(value: &Value) -> Result<Vec<AgentCliModel>, MappedAiProviderError> {
    let models = value
        .get("data")
        .and_then(|data| data.get("models"))
        .and_then(Value::as_array)
        .ok_or_else(|| MappedAiProviderError::new(AiProviderError::BadResponse))?;
    models
        .iter()
        .map(parse_pi_model)
        .collect::<Result<Vec<_>, _>>()
}

fn parse_pi_model(value: &Value) -> Result<AgentCliModel, MappedAiProviderError> {
    let provider = value
        .get("provider")
        .and_then(Value::as_str)
        .filter(|provider| !provider.is_empty())
        .ok_or_else(|| MappedAiProviderError::new(AiProviderError::BadResponse))?;
    let model_id = value
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| MappedAiProviderError::new(AiProviderError::BadResponse))?;
    let name = value
        .get("name")
        .or_else(|| value.get("label"))
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| humanize_cli_model_id(model_id));

    Ok(AgentCliModel {
        id: format!("{provider}/{model_id}"),
        name,
        recommended: false,
        reasoning: model_reasoning(value),
        context_window: model_context_window(value),
        source_provider: Some(provider.to_string()),
        cli_default: false,
    })
}

/// Parse `omp models --json` output. Omp's selector is the exact value it
/// accepts for `--model`; it must never be reconstructed from provider/id.
fn parse_omp_models(stdout: &[u8]) -> Result<Vec<AgentCliModel>, MappedAiProviderError> {
    let text = String::from_utf8_lossy(stdout);
    let payload = extract_json_payload(&text).unwrap_or(text.trim());
    let value: Value = serde_json::from_str(payload)
        .map_err(|_| MappedAiProviderError::new(AiProviderError::BadResponse))?;
    let models = value
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| MappedAiProviderError::new(AiProviderError::BadResponse))?;
    models
        .iter()
        .map(parse_omp_model)
        .collect::<Result<Vec<_>, _>>()
}

fn parse_omp_model(value: &Value) -> Result<AgentCliModel, MappedAiProviderError> {
    let selector = value
        .get("selector")
        .and_then(Value::as_str)
        .filter(|selector| !selector.is_empty())
        .ok_or_else(|| MappedAiProviderError::new(AiProviderError::BadResponse))?;
    let name = value
        .get("name")
        .or_else(|| value.get("label"))
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| humanize_cli_model_id(selector));
    let source_provider = value
        .get("provider")
        .and_then(Value::as_str)
        .filter(|provider| !provider.is_empty())
        .map(str::to_string);

    Ok(AgentCliModel {
        id: selector.to_string(),
        name,
        recommended: false,
        reasoning: model_reasoning(value),
        context_window: model_context_window(value),
        source_provider,
        cli_default: false,
    })
}

fn model_reasoning(value: &Value) -> bool {
    value
        .get("reasoning")
        .and_then(Value::as_bool)
        .or_else(|| value.get("thinking").and_then(Value::as_bool))
        .unwrap_or(false)
}

fn model_context_window(value: &Value) -> Option<u64> {
    ["contextWindow", "context_window", "context"]
        .into_iter()
        .find_map(|key| value.get(key).and_then(Value::as_u64))
}

fn humanize_cli_model_id(id: &str) -> String {
    let mut result = String::new();
    for (index, word) in id
        .split(['/', '-', '_', '.'])
        .filter(|word| !word.is_empty())
        .enumerate()
    {
        if index > 0 {
            result.push(' ');
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            result.extend(first.to_uppercase());
            result.extend(chars);
        }
    }
    if result.is_empty() {
        id.to_string()
    } else {
        result
    }
}

const MODEL_LIST_TIMEOUT: Duration = Duration::from_secs(3);
const MODEL_LIST_MAX_OUTPUT: usize = 512 * 1024;
const PI_MODEL_LIST_ARGV: &[&str] = &[
    "--mode",
    "rpc",
    "--no-tools",
    "--no-session",
    "--no-extensions",
    "--no-skills",
    "--no-prompt-templates",
    "--no-context-files",
];
const OMP_MODEL_LIST_ARGV: &[&str] = &["models", "--json", "--no-extensions"];
const PI_MODEL_LIST_REQUEST: &[u8] =
    b"{\"id\":\"voicetypr-models\",\"type\":\"get_available_models\"}\n";

/// List models without making a completion request. Claude has no machine
/// readable model-list command, so its list is deliberately curated. pi and
/// omp prepend an explicit empty-id entry that means "use the CLI default".
pub async fn list_models(provider: &str) -> Result<Vec<AgentCliModel>, MappedAiProviderError> {
    match provider {
        PROVIDER_CLAUDE_CODE => Ok(curated_claude_models()),
        PROVIDER_PI => {
            let binary = resolve_binary(PI_SPEC.binary)
                .await
                .ok_or_else(|| MappedAiProviderError::new(AiProviderError::UnsupportedProvider))?;
            let payload = run_pi_model_listing(&binary).await?;
            let mut models = vec![cli_default_model()];

            models.extend(parse_pi_models(&payload)?);
            Ok(models)
        }
        PROVIDER_OMP => {
            let binary = resolve_binary(OMP_SPEC.binary)
                .await
                .ok_or_else(|| MappedAiProviderError::new(AiProviderError::UnsupportedProvider))?;
            let payload = run_omp_model_listing(&binary).await?;
            let mut models = vec![cli_default_model()];
            models.extend(parse_omp_models(&payload)?);
            Ok(models)
        }
        _ => Err(MappedAiProviderError::new(
            AiProviderError::UnsupportedProvider,
        )),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModelListCleanupStatus {
    Complete,
    Failed,
    TimedOut,
}

async fn terminate_model_list_child(
    child: &mut ProcessGroupGuard,
    deadline: tokio::time::Instant,
) -> ModelListCleanupStatus {
    let kill_result = child.start_kill();
    match tokio::time::timeout_at(deadline, child.wait()).await {
        Ok(Ok(_)) if group_kill_is_settled(&kill_result) => {
            child.disarm();
            ModelListCleanupStatus::Complete
        }
        Ok(Ok(_)) | Ok(Err(_)) => ModelListCleanupStatus::Failed,
        Err(_) => ModelListCleanupStatus::TimedOut,
    }
}

async fn finish_model_list_task<T>(
    mut task: tokio::task::JoinHandle<T>,
    deadline: tokio::time::Instant,
) -> (ModelListCleanupStatus, Option<T>) {
    match tokio::time::timeout_at(deadline, &mut task).await {
        Ok(Ok(value)) => (ModelListCleanupStatus::Complete, Some(value)),
        Ok(Err(_)) => (ModelListCleanupStatus::Failed, None),
        Err(_) => {
            task.abort();
            let _ = tokio::time::timeout_at(deadline, task).await;
            (ModelListCleanupStatus::TimedOut, None)
        }
    }
}

async fn run_pi_model_listing(binary_path: &Path) -> Result<Vec<u8>, MappedAiProviderError> {
    let deadline = tokio::time::Instant::now() + MODEL_LIST_TIMEOUT;
    let temp_dir = tempfile::TempDir::new()
        .map_err(|_| MappedAiProviderError::new(AiProviderError::Internal))?;
    let mut command = Command::new(binary_path);
    command.args(PI_MODEL_LIST_ARGV);
    command.current_dir(temp_dir.path());
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.env("PATH", resolved_path().await);
    command.env("ZSH_TMUX_AUTOSTART", "false");
    command.kill_on_drop(true);
    apply_no_window(&mut command);

    let mut child = ProcessGroupGuard::new(
        command
            .group_spawn()
            .map_err(|error| MappedAiProviderError::new(map_spawn_error(&error)))?,
    );
    let mut stderr = match child.inner().stderr.take() {
        Some(stderr) => stderr,
        None => {
            let cleanup = terminate_model_list_child(&mut child, deadline).await;
            return Err(MappedAiProviderError::new(
                if matches!(cleanup, ModelListCleanupStatus::TimedOut) {
                    AiProviderError::Timeout
                } else {
                    AiProviderError::Internal
                },
            ));
        }
    };
    let mut stdout = match child.inner().stdout.take() {
        Some(stdout) => stdout,
        None => {
            let cleanup = terminate_model_list_child(&mut child, deadline).await;
            return Err(MappedAiProviderError::new(
                if matches!(cleanup, ModelListCleanupStatus::TimedOut) {
                    AiProviderError::Timeout
                } else {
                    AiProviderError::Internal
                },
            ));
        }
    };
    let stderr_task = tokio::spawn(async move { drain_bounded(&mut stderr).await });
    let mut stdout_task =
        tokio::spawn(async move { read_until_pi_model_response(&mut stdout).await });
    let mut stdin = match child.inner().stdin.take() {
        Some(stdin) => stdin,
        None => {
            let child_cleanup = terminate_model_list_child(&mut child, deadline).await;
            let (stdout_cleanup, _) = finish_model_list_task(stdout_task, deadline).await;
            let (stderr_cleanup, _) = finish_model_list_task(stderr_task, deadline).await;
            let timed_out = matches!(child_cleanup, ModelListCleanupStatus::TimedOut)
                || matches!(stdout_cleanup, ModelListCleanupStatus::TimedOut)
                || matches!(stderr_cleanup, ModelListCleanupStatus::TimedOut);
            return Err(MappedAiProviderError::new(if timed_out {
                AiProviderError::Timeout
            } else {
                AiProviderError::Internal
            }));
        }
    };
    let mut stdout_consumed = false;
    let response = tokio::time::timeout_at(deadline, async {
        stdin
            .write_all(PI_MODEL_LIST_REQUEST)
            .await
            .map_err(|_| MappedAiProviderError::new(AiProviderError::Internal))?;
        // Keep stdin open until the matching response arrives. pi's stdin EOF
        // handler otherwise exits before the asynchronous RPC command finishes.
        let joined = (&mut stdout_task).await;
        stdout_consumed = true;
        joined.map_err(|_| MappedAiProviderError::new(AiProviderError::Internal))?
    })
    .await;

    // RPC mode is intentionally persistent until its matching response (or
    // timeout). Close stdin only during teardown, then explicitly terminate and
    // reap the child; it is never retained as a session.
    drop(stdin);
    let child_cleanup = terminate_model_list_child(&mut child, deadline).await;
    let (stdout_cleanup, _) = if stdout_consumed {
        (ModelListCleanupStatus::Complete, None)
    } else {
        finish_model_list_task(stdout_task, deadline).await
    };
    let (stderr_cleanup, _) = finish_model_list_task(stderr_task, deadline).await;
    let cleanup_timed_out = matches!(child_cleanup, ModelListCleanupStatus::TimedOut)
        || matches!(stdout_cleanup, ModelListCleanupStatus::TimedOut)
        || matches!(stderr_cleanup, ModelListCleanupStatus::TimedOut);
    let cleanup_failed = matches!(child_cleanup, ModelListCleanupStatus::Failed)
        || matches!(stdout_cleanup, ModelListCleanupStatus::Failed)
        || matches!(stderr_cleanup, ModelListCleanupStatus::Failed);

    match response {
        Err(_) => Err(MappedAiProviderError::new(AiProviderError::Timeout)),
        Ok(Err(error)) => {
            if cleanup_timed_out {
                Err(MappedAiProviderError::new(AiProviderError::Timeout))
            } else if cleanup_failed {
                Err(MappedAiProviderError::new(AiProviderError::Internal))
            } else {
                Err(error)
            }
        }
        Ok(Ok(payload)) => {
            if cleanup_timed_out {
                Err(MappedAiProviderError::new(AiProviderError::Timeout))
            } else if cleanup_failed {
                Err(MappedAiProviderError::new(AiProviderError::Internal))
            } else {
                Ok(payload)
            }
        }
    }
}

async fn read_until_pi_model_response(
    stdout: &mut tokio::process::ChildStdout,
) -> Result<Vec<u8>, MappedAiProviderError> {
    use tokio::io::AsyncReadExt as _;

    let mut retained = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let read = stdout
            .read(&mut chunk)
            .await
            .map_err(|_| MappedAiProviderError::new(AiProviderError::Internal))?;
        if read == 0 {
            return Err(MappedAiProviderError::new(AiProviderError::BadResponse));
        }
        if retained.len().saturating_add(read) > MODEL_LIST_MAX_OUTPUT {
            return Err(MappedAiProviderError::new(AiProviderError::BadResponse));
        }
        retained.extend_from_slice(&chunk[..read]);
        if pi_response_is_present(&retained) {
            return Ok(retained);
        }
    }
}

fn pi_response_is_present(output: &[u8]) -> bool {
    String::from_utf8_lossy(output).lines().any(|line| {
        serde_json::from_str::<Value>(line.trim())
            .ok()
            .and_then(|value| value.get("id").and_then(Value::as_str).map(str::to_string))
            .as_deref()
            == Some(PI_MODELS_RESPONSE_ID)
    })
}
async fn run_omp_model_listing(binary_path: &Path) -> Result<Vec<u8>, MappedAiProviderError> {
    let deadline = tokio::time::Instant::now() + MODEL_LIST_TIMEOUT;
    let temp_dir = tempfile::TempDir::new()
        .map_err(|_| MappedAiProviderError::new(AiProviderError::Internal))?;
    let mut command = Command::new(binary_path);
    command.args(OMP_MODEL_LIST_ARGV);
    command.current_dir(temp_dir.path());
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.env("PATH", resolved_path().await);
    command.env("ZSH_TMUX_AUTOSTART", "false");
    command.kill_on_drop(true);
    apply_no_window(&mut command);

    let mut child = ProcessGroupGuard::new(
        command
            .group_spawn()
            .map_err(|error| MappedAiProviderError::new(map_spawn_error(&error)))?,
    );
    let mut stdout = match child.inner().stdout.take() {
        Some(stdout) => stdout,
        None => {
            let cleanup = terminate_model_list_child(&mut child, deadline).await;
            return Err(MappedAiProviderError::new(
                if matches!(cleanup, ModelListCleanupStatus::TimedOut) {
                    AiProviderError::Timeout
                } else {
                    AiProviderError::Internal
                },
            ));
        }
    };
    let mut stderr = match child.inner().stderr.take() {
        Some(stderr) => stderr,
        None => {
            let cleanup = terminate_model_list_child(&mut child, deadline).await;
            return Err(MappedAiProviderError::new(
                if matches!(cleanup, ModelListCleanupStatus::TimedOut) {
                    AiProviderError::Timeout
                } else {
                    AiProviderError::Internal
                },
            ));
        }
    };
    let stdout_task = tokio::spawn(async move { read_bounded_output(&mut stdout).await });
    let stderr_task = tokio::spawn(async move { drain_bounded(&mut stderr).await });

    let status = tokio::time::timeout_at(deadline, child.wait()).await;
    match status {
        Ok(Ok(status)) => {
            // Both drains must be settled (or explicitly aborted at the same
            // absolute deadline) before the payload is handed to the parser.
            let (stdout_cleanup, stdout_result) =
                finish_model_list_task(stdout_task, deadline).await;
            let (stderr_cleanup, _) = finish_model_list_task(stderr_task, deadline).await;
            if matches!(stdout_cleanup, ModelListCleanupStatus::TimedOut)
                || matches!(stderr_cleanup, ModelListCleanupStatus::TimedOut)
            {
                return Err(MappedAiProviderError::new(AiProviderError::Timeout));
            }
            if matches!(stdout_cleanup, ModelListCleanupStatus::Failed) {
                return Err(MappedAiProviderError::new(AiProviderError::Internal));
            }
            let output = stdout_result
                .ok_or_else(|| MappedAiProviderError::new(AiProviderError::Internal))??;
            if !status.success() {
                return Err(MappedAiProviderError::new(AiProviderError::AgentCli(
                    "The CLI could not list models.".to_string(),
                )));
            }
            Ok(output)
        }
        Ok(Err(_)) => {
            // A wait error is terminal for this child. Kill it before settling
            // the readers, but never join a descendant-held pipe indefinitely.
            let _ = child.start_kill();
            let (stdout_cleanup, _) = finish_model_list_task(stdout_task, deadline).await;
            let (stderr_cleanup, _) = finish_model_list_task(stderr_task, deadline).await;
            if matches!(stdout_cleanup, ModelListCleanupStatus::TimedOut)
                || matches!(stderr_cleanup, ModelListCleanupStatus::TimedOut)
            {
                Err(MappedAiProviderError::new(AiProviderError::Timeout))
            } else {
                Err(MappedAiProviderError::new(AiProviderError::Internal))
            }
        }
        Err(_) => {
            let _ = terminate_model_list_child(&mut child, deadline).await;
            let _ = finish_model_list_task(stdout_task, deadline).await;
            let _ = finish_model_list_task(stderr_task, deadline).await;
            Err(MappedAiProviderError::new(AiProviderError::Timeout))
        }
    }
}

async fn read_bounded_output(
    stdout: &mut tokio::process::ChildStdout,
) -> Result<Vec<u8>, MappedAiProviderError> {
    use tokio::io::AsyncReadExt as _;

    let mut retained = Vec::new();
    let mut too_large = false;
    let mut chunk = [0_u8; 8192];
    loop {
        let read = stdout
            .read(&mut chunk)
            .await
            .map_err(|_| MappedAiProviderError::new(AiProviderError::Internal))?;
        if read == 0 {
            break;
        }
        let retained_before = retained.len();
        let remaining = MODEL_LIST_MAX_OUTPUT.saturating_sub(retained_before);
        if remaining > 0 {
            retained.extend_from_slice(&chunk[..read.min(remaining)]);
        }
        if read > remaining {
            too_large = true;
        }
    }
    if too_large {
        Err(MappedAiProviderError::new(AiProviderError::BadResponse))
    } else {
        Ok(retained)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curated_claude_models_are_ordered_with_haiku_default() {
        let models = curated_claude_models();
        assert_eq!(
            models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["haiku", "sonnet", "opus"]
        );
        assert!(models[0].recommended);
        assert!(!models[0].cli_default);
        assert!(models.iter().skip(1).all(|model| !model.recommended));
    }

    #[test]
    fn cli_default_model_is_explicit_empty_selection() {
        let model = cli_default_model();
        assert_eq!(model.id, "");
        assert_eq!(model.name, "CLI default");
        assert!(model.recommended);
        assert!(model.cli_default);
        assert_eq!(model.source_provider, None);
    }

    #[test]
    fn agent_cli_model_serializes_camel_case_metadata() {
        let model = AgentCliModel {
            id: "provider/model".to_string(),
            name: "Model".to_string(),
            recommended: false,
            reasoning: true,
            context_window: Some(12_345),
            source_provider: Some("provider".to_string()),
            cli_default: false,
        };
        let value = serde_json::to_value(model).unwrap();
        assert_eq!(value["contextWindow"], 12_345);
        assert_eq!(value["sourceProvider"], "provider");
        assert_eq!(value["cliDefault"], false);
        assert!(value.get("context_window").is_none());
        assert!(value.get("source_provider").is_none());
    }

    #[test]
    fn model_discovery_uses_only_isolated_non_completion_commands() {
        assert_eq!(
            PI_MODEL_LIST_ARGV,
            &[
                "--mode",
                "rpc",
                "--no-tools",
                "--no-session",
                "--no-extensions",
                "--no-skills",
                "--no-prompt-templates",
                "--no-context-files",
            ][..]
        );
        assert_eq!(
            OMP_MODEL_LIST_ARGV,
            &["models", "--json", "--no-extensions"][..]
        );
        assert_eq!(
            PI_MODEL_LIST_REQUEST,
            b"{\"id\":\"voicetypr-models\",\"type\":\"get_available_models\"}\n"
        );
        assert!(!PI_MODEL_LIST_ARGV.contains(&"-p"));
        assert!(!OMP_MODEL_LIST_ARGV.contains(&"-p"));
        assert!(!String::from_utf8_lossy(PI_MODEL_LIST_REQUEST).contains("transcript"));
    }

    #[test]
    fn parse_pi_models_matches_response_id_and_preserves_metadata() {
        let payload = br#"{"id":"other","data":{"models":[{"provider":"wrong","id":"wrong"}]}}
{"id":"voicetypr-models","type":"response","data":{"models":[{"provider":"anthropic","id":"claude-sonnet","name":"Sonnet","contextWindow":200000,"reasoning":true}]}}"#;
        let models = parse_pi_models(payload).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "anthropic/claude-sonnet");
        assert_eq!(models[0].name, "Sonnet");
        assert_eq!(models[0].source_provider.as_deref(), Some("anthropic"));
        assert_eq!(models[0].context_window, Some(200_000));
        assert!(models[0].reasoning);
        assert!(!models[0].cli_default);
    }

    #[test]
    fn parse_pi_models_rejects_nonmatching_malformed_and_empty_payloads() {
        for payload in [
            br#"{"id":"not-voicetypr-models","data":{"models":[]}}"# as &[u8],
            b"not json",
            b"",
            br#"{"id":"voicetypr-models","data":{}}"#,
        ] {
            assert!(
                parse_pi_models(payload).is_err(),
                "payload should be rejected: {:?}",
                String::from_utf8_lossy(payload)
            );
        }
        let empty_models = br#"{"id":"voicetypr-models","data":{"models":[]}}"#;
        assert!(parse_pi_models(empty_models).unwrap().is_empty());
    }

    #[test]
    fn parse_omp_models_preserves_exact_selector_and_metadata() {
        let payload = br#"{"models":[{"provider":"openai","id":"gpt-4o","selector":"openai/gpt-4o@2024-11-20","name":"GPT-4o","context":128000,"reasoning":false}]}"#;
        let models = parse_omp_models(payload).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "openai/gpt-4o@2024-11-20");
        assert_eq!(models[0].source_provider.as_deref(), Some("openai"));
        assert_eq!(models[0].name, "GPT-4o");
        assert_eq!(models[0].context_window, Some(128_000));
        assert!(!models[0].reasoning);
    }

    #[test]
    fn parse_omp_models_rejects_malformed_and_empty_payloads() {
        assert!(parse_omp_models(b"").is_err());
        assert!(parse_omp_models(b"not json").is_err());
        assert!(parse_omp_models(br#"{}"#).is_err());
        assert!(parse_omp_models(br#"{"models":[]}"#).unwrap().is_empty());
    }

    #[test]
    fn claude_code_cold_argv_is_fixed_except_for_prompt_slot() {
        // The spec-table argv is fixed: every token is a constant except the
        // `--system-prompt` value, which receives `request.prompt`. Dictated
        // text is NOT here (it travels via stdin).
        let argv = cold_argv(&CLAUDE_CODE_SPEC, "my system prompt");
        assert_eq!(
            argv,
            vec![
                "-p",
                "--setting-sources",
                "",
                "--tools",
                "",
                "--strict-mcp-config",
                "--no-chrome",
                "--model",
                "haiku",
                "--system-prompt",
                "my system prompt",
                "--output-format",
                "json",
            ]
        );
        // Regression guard: the `--bare` flag that disabled credentials and the
        // dropped `--no-session-persistence` must never return to the argv.
        assert!(!argv.contains(&"--bare".to_string()));
        assert!(!argv.contains(&"--no-session-persistence".to_string()));
    }

    #[test]
    fn claude_code_cold_argv_has_no_shell_or_input_text_slot() {
        // Security invariant: dictated text must NEVER appear in argv (no shell
        // string, no `sh -c`). The only interpolated slot is the system prompt.
        let dangerous_input = "rm -rf /; cat /etc/passwd; $(pwned)";
        let argv = cold_argv(&CLAUDE_CODE_SPEC, "polish this");
        assert!(!argv.contains(&dangerous_input.to_string()));
        assert!(argv.iter().all(|arg| !arg.contains("sh -c")));
        assert_eq!(argv.len(), 13);
    }

    #[test]
    fn pi_cold_argv_is_fixed_except_for_prompt_slot() {
        // Dictation is fed via stdin; empty model selection must omit
        // `--model` while retaining every mandatory isolation flag.
        let argv = cold_argv(&PI_SPEC, "my system prompt");
        assert_eq!(
            argv,
            vec![
                "-p",
                "--no-tools",
                "--no-session",
                "--no-extensions",
                "--no-skills",
                "--no-prompt-templates",
                "--no-context-files",
                "--thinking",
                "off",
                "--mode",
                "json",
                "--system-prompt",
                "my system prompt",
            ]
        );
        assert!(!argv.iter().any(|arg| arg == "--model"));
        assert!(argv.iter().all(|arg| !arg.contains("sh -c")));
    }

    #[test]
    fn omp_cold_argv_is_fixed_except_for_prompt_slot() {
        // Empty model selection omits `--model`; omp still receives every
        // mandatory isolation flag and thinking-off policy.
        let argv = cold_argv(&OMP_SPEC, "my system prompt");
        assert_eq!(
            argv,
            vec![
                "-p",
                "--no-tools",
                "--no-session",
                "--no-skills",
                "--no-rules",
                "--no-extensions",
                "--no-lsp",
                "--no-title",
                "--thinking",
                "off",
                "--mode",
                "json",
                "--system-prompt",
                "my system prompt",
            ]
        );
        assert!(!argv.iter().any(|arg| arg == "--model"));
        assert!(argv.iter().all(|arg| !arg.contains("sh -c")));
        assert_eq!(OMP_SPEC.input_mode, InputMode::PositionalArg);
        assert_eq!(OMP_SPEC.cold_argv_suffix.len(), 0);
    }

    #[test]
    fn selected_model_argv_uses_discrete_values_for_all_cli_providers() {
        let claude = cold_argv_for_model(
            &CLAUDE_CODE_SPEC,
            "prompt",
            "sonnet",
            ClaudeCapabilities::default(),
        );
        assert_eq!(
            claude,
            vec![
                "-p",
                "--setting-sources",
                "",
                "--tools",
                "",
                "--strict-mcp-config",
                "--no-chrome",
                "--model",
                "sonnet",
                "--system-prompt",
                "prompt",
                "--output-format",
                "json",
            ]
        );

        let pi = cold_argv_for_model(
            &PI_SPEC,
            "prompt",
            "anthropic/sonnet",
            ClaudeCapabilities::default(),
        );
        let has_pair = |argv: &[String], flag: &str, value: &str| {
            argv.windows(2)
                .any(|pair| pair[0] == flag && pair[1] == value)
        };
        assert!(has_pair(&pi, "--model", "anthropic/sonnet"));
        assert!(!has_pair(&pi, "--model", ""));
        assert!(has_pair(&pi, "--thinking", "off"));

        let omp = cold_argv_for_model(
            &OMP_SPEC,
            "prompt",
            "openai/gpt-4o@exact",
            ClaudeCapabilities::default(),
        );
        assert!(has_pair(&omp, "--model", "openai/gpt-4o@exact"));
        assert!(!has_pair(&omp, "--model", ""));
        assert!(has_pair(&omp, "--thinking", "off"));
    }

    #[test]
    fn specs_select_correct_input_output_and_auth_modes() {
        // claude-code: stdin, claude-json, real auth status.
        assert_eq!(CLAUDE_CODE_SPEC.input_mode, InputMode::Stdin);
        assert_eq!(CLAUDE_CODE_SPEC.output, OutputParser::ClaudeJson);
        assert_eq!(CLAUDE_CODE_SPEC.auth, AuthMode::RealAuthStatus);
        // pi: stdin, pi-jsonl, optimistic auth.
        assert_eq!(PI_SPEC.input_mode, InputMode::Stdin);
        assert_eq!(PI_SPEC.output, OutputParser::PiJsonl);
        assert_eq!(PI_SPEC.auth, AuthMode::Optimistic);
        // omp: positional arg, pi-jsonl, optimistic auth.
        assert_eq!(OMP_SPEC.input_mode, InputMode::PositionalArg);
        assert_eq!(OMP_SPEC.output, OutputParser::PiJsonl);
        assert_eq!(OMP_SPEC.auth, AuthMode::Optimistic);
        // Dispatch covers all three providers.
        assert_eq!(
            spec_for(PROVIDER_CLAUDE_CODE).map(|s| s.binary),
            Some("claude")
        );
        assert_eq!(spec_for(PROVIDER_PI).map(|s| s.binary), Some("pi"));
        assert_eq!(spec_for(PROVIDER_OMP).map(|s| s.binary), Some("omp"));
        assert!(spec_for("unknown").is_none());
    }

    #[test]
    fn claude_help_capabilities_choose_safe_mode_and_effort() {
        let help = b"Options:\n  --safe-mode\n  --effort <low|medium|high>\n";
        let capabilities = claude_capabilities_from_help(help);
        assert_eq!(
            capabilities,
            ClaudeCapabilities {
                safe_mode: true,
                effort: true
            }
        );
        let argv = cold_argv_for_model(&CLAUDE_CODE_SPEC, "prompt", "sonnet", capabilities);
        assert!(argv
            .windows(2)
            .any(|pair| pair[0] == "--model" && pair[1] == "sonnet"));
        assert!(argv
            .windows(2)
            .any(|pair| pair[0] == "--effort" && pair[1] == "low"));
        assert!(argv.contains(&"--safe-mode".to_string()));
        assert!(!argv.contains(&"--setting-sources".to_string()));
    }

    #[test]
    fn claude_help_fallback_uses_setting_sources_and_haiku() {
        let argv = cold_argv_for_model(
            &CLAUDE_CODE_SPEC,
            "prompt",
            "",
            ClaudeCapabilities::default(),
        );
        assert!(argv
            .windows(2)
            .any(|pair| pair[0] == "--setting-sources" && pair[1].is_empty()));
        assert!(argv
            .windows(2)
            .any(|pair| pair[0] == "--model" && pair[1] == "haiku"));
        assert!(!argv.contains(&"--safe-mode".to_string()));
        assert!(!argv.contains(&"--effort".to_string()));
    }

    #[test]
    fn missing_mandatory_provider_capabilities_are_incompatible() {
        assert!(!required_capabilities_present(
            &PI_SPEC,
            b"--no-tools --no-session --no-extensions --no-skills"
        ));
        assert!(!required_capabilities_present(
            &OMP_SPEC,
            b"--no-tools --no-session --no-extensions --no-lsp"
        ));
        assert!(required_capabilities_present(
            &PI_SPEC,
            b"--no-tools --no-session --no-extensions --no-skills --no-prompt-templates --no-context-files --thinking"
        ));
        assert!(required_capabilities_present(
            &OMP_SPEC,
            b"--no-tools --no-session --no-skills --no-rules --no-extensions --no-lsp --no-title --thinking"
        ));
    }

    #[test]
    fn cli_error_sanitization_is_bounded_and_strips_control_sequences() {
        let raw = format!("\u{1b}[31m{}\nnext", "x".repeat(500));
        let clean = sanitize_cli_error(&raw);
        assert!(clean.chars().count() <= MAX_CLI_ERROR_CHARS);
        assert!(!clean.contains('\u{1b}'));
        assert!(!clean.contains('\n'));
    }

    #[test]
    fn nonzero_success_looking_payload_is_not_treated_as_polish() {
        let detail = extract_process_error(
            OutputParser::ClaudeJson,
            br#"{"result":"success-looking output","is_error":false}"#,
            b"",
        );
        assert_eq!(detail, "success-looking output");
    }

    #[test]
    fn launcher_allowlist_rejects_windows_and_script_forms() {
        assert!(is_windows_native_executable(Path::new(
            "C:\\Tools\\claude.EXE"
        )));
        assert!(!is_windows_native_executable(Path::new(
            "C:\\Tools\\claude.COM"
        )));
        for path in [
            "C:\\Tools\\claude.cmd",
            "C:\\Tools\\claude.bat",
            "C:\\Tools\\claude.ps1",
            "C:\\Tools\\claude.sh",
            "C:\\Tools\\claude.vbs",
            "C:\\Tools\\claude.wsh",
        ] {
            assert!(is_script_launcher(Path::new(path)), "{path}");
        }
        assert!(!is_script_launcher(Path::new("/usr/local/bin/claude")));
    }

    #[test]
    fn resolver_continues_after_unsafe_candidate() {
        let candidates = vec![
            PathBuf::from("/tmp/claude.cmd"),
            PathBuf::from("/tmp/claude"),
        ];
        assert_eq!(
            select_safe_candidate(candidates),
            BinaryResolution::Found(PathBuf::from("/tmp/claude"))
        );
        assert_eq!(
            select_safe_candidate([PathBuf::from("/tmp/claude.ps1")]),
            BinaryResolution::UnsafeLauncher
        );
        assert_eq!(
            select_safe_candidate(std::iter::empty::<PathBuf>()),
            BinaryResolution::Missing
        );
    }

    #[test]
    fn probe_state_mapping_distinguishes_wire_states() {
        let missing = BinaryResolution::Missing;
        assert_eq!(
            state_for_probe(&missing, false, false, false),
            AgentCliProbeState::Missing
        );
        let unsafe_launcher = BinaryResolution::UnsafeLauncher;
        assert_eq!(
            state_for_probe(&unsafe_launcher, false, false, false),
            AgentCliProbeState::UnsafeLauncher
        );
        let found = BinaryResolution::Found(PathBuf::from("/safe/cli"));
        assert_eq!(
            state_for_probe(&found, true, false, false),
            AgentCliProbeState::Incompatible
        );
        assert_eq!(
            state_for_probe(&found, true, true, false),
            AgentCliProbeState::NotAuthenticated
        );
        assert_eq!(
            state_for_probe(&found, true, true, true),
            AgentCliProbeState::Ready
        );
    }

    #[test]
    fn parse_claude_json_reads_result_field() {
        let json = br#"{"type":"result","result":"Hello, world.","is_error":false}"#;
        assert_eq!(parse_claude_json(json).unwrap(), "Hello, world.");
    }

    #[test]
    fn parse_claude_json_reads_content_string() {
        let json = br#"{"content":"Raw content here."}"#;
        assert_eq!(parse_claude_json(json).unwrap(), "Raw content here.");
    }

    #[test]
    fn parse_claude_json_reads_content_text_blocks() {
        let json =
            br#"{"content":[{"type":"text","text":"line one"},{"type":"text","text":"line two"}]}"#;
        assert_eq!(parse_claude_json(json).unwrap(), "line one\nline two");
    }

    #[test]
    fn parse_claude_json_recovers_from_noisy_prefix() {
        // A real CLI may emit an update notice or shell banner before the JSON.
        // We slice between the first `{` and the last `}` (parakeet precedent).
        let noisy = b"A new version of claude is available. Run npm i -g @anthropic-ai/claude-code to update.\n{\"result\":\"Polished text.\"}\n";
        assert_eq!(parse_claude_json(noisy).unwrap(), "Polished text.");
    }

    #[test]
    fn parse_claude_json_rejects_garbage() {
        assert!(parse_claude_json(b"not json at all").is_err());
        assert!(parse_claude_json(b"{}").is_err());
        assert!(parse_claude_json(b"{\"unrelated\":\"field\"}").is_err());
    }

    #[test]
    fn parse_claude_json_rejects_is_error_result_as_polish_text() {
        // Latent-bug guard: an `is_error:true` `result` is the CLI's OWN message
        // (e.g. "Not logged in · Please run /login"), NOT polished text. It must
        // surface as Err(AgentCli) carrying that message so it is never typed at
        // the cursor, and the executor falls back to the raw transcript.
        let json =
            r#"{"type":"result","result":"Not logged in · Please run /login","is_error":true}"#
                .as_bytes();
        let err =
            parse_claude_json(json).expect_err("an is_error result must NOT become polished text");
        match err.error {
            AiProviderError::AgentCli(message) => {
                assert_eq!(message, "Not logged in · Please run /login");
            }
            other => panic!("expected AgentCli error, got {other:?}"),
        }
    }

    #[test]
    fn parse_claude_json_rejects_subtype_error_marker() {
        // CLIs that signal failure via `subtype`/`type` = "error" instead of
        // `is_error` are also rejected; the message falls back to the `error`
        // field, then `message`, then a short generic.
        let json = br#"{"subtype":"error","error":"rate limit exceeded"}"#;
        let err = parse_claude_json(json).expect_err("subtype=error must be rejected");
        match err.error {
            AiProviderError::AgentCli(message) => assert_eq!(message, "rate limit exceeded"),
            other => panic!("expected AgentCli error, got {other:?}"),
        }
    }

    #[test]
    fn parse_claude_json_keeps_success_path_for_is_error_false() {
        // `is_error:false` (or absent) with a `result` stays on the success
        // path — the guard must not over-trigger on normal results.
        let json = br#"{"type":"result","result":"Hello, world.","is_error":false}"#;
        assert_eq!(parse_claude_json(json).unwrap(), "Hello, world.");
    }

    #[test]
    fn parse_pi_jsonl_returns_last_assistant_message_text() {
        // A real pi/omp `--mode json` stream: session → agent_start → turn_start
        // → user message → assistant message_start/update/end → turn_end →
        // agent_end. The polished text is the LAST assistant message's
        // `message.content[].text`. message_end carries the final text.
        let stream = b"{\"type\":\"session\",\"id\":\"abc\"}\n\
{\"type\":\"agent_start\"}\n\
{\"type\":\"turn_start\"}\n\
{\"type\":\"message_start\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"uhh fix the bug\"}]}}\n\
{\"type\":\"message_end\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"uhh fix the bug\"}]}}\n\
{\"type\":\"message_start\",\"message\":{\"role\":\"assistant\",\"content\":[]}}\n\
{\"type\":\"message_update\",\"assistantMessageEvent\":{\"type\":\"text_delta\",\"delta\":\"Fix\"},\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Fix\"}]}}\n\
{\"type\":\"message_update\",\"assistantMessageEvent\":{\"type\":\"text_end\",\"content\":\"Fix the bug.\"},\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Fix the bug.\"}]}}\n\
{\"type\":\"message_end\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Fix the bug.\"}]}}\n\
{\"type\":\"turn_end\"}\n\
{\"type\":\"agent_end\"}\n";
        assert_eq!(parse_pi_jsonl(stream).unwrap(), "Fix the bug.");
    }

    #[test]
    fn parse_pi_jsonl_picks_last_nonempty_when_multiple_assistant_messages() {
        // If the stream carries more than one assistant message (a retry, or
        // update + end both holding text), the LAST non-empty one wins — the
        // completed polished output. Here message_end's text overrides the
        // earlier message_update's shorter text.
        let stream = b"{\"type\":\"message_update\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"partial\"}]}}\n\
{\"type\":\"message_end\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Final polished text.\"}]}}\n";
        assert_eq!(parse_pi_jsonl(stream).unwrap(), "Final polished text.");
    }

    #[test]
    fn parse_pi_jsonl_joins_multiple_text_blocks() {
        // Multi-block assistant content joins with `\n` (mirrors parse_claude_json).
        let stream = b"{\"type\":\"message_end\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"line one\"},{\"type\":\"text\",\"text\":\"line two\"}]}}\n";
        assert_eq!(parse_pi_jsonl(stream).unwrap(), "line one\nline two");
    }

    #[test]
    fn parse_pi_jsonl_ignores_user_messages_and_unrelated_events() {
        // The user message_end must NOT be mistaken for the polished result.
        let stream = b"{\"type\":\"message_end\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"raw dictation\"}]}}\n\
{\"type\":\"turn_end\"}\n";
        assert!(parse_pi_jsonl(stream).is_err());
    }

    #[test]
    fn parse_pi_jsonl_rejects_garbage() {
        assert!(parse_pi_jsonl(b"").is_err());
        assert!(parse_pi_jsonl(b"not json at all").is_err());
        // Only session/non-assistant events, no assistant text.
        assert!(parse_pi_jsonl(b"{\"type\":\"session\"}\n{\"type\":\"agent_start\"}\n").is_err());
    }

    #[test]
    fn parse_pi_jsonl_tolerates_stray_non_json_lines() {
        // A shell banner / update notice before the stream must not fail the
        // parse — non-`{`-prefixed and unparseable lines are skipped.
        let stream = b"some shell banner\n\
{\"type\":\"message_end\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Polished.\"}]}}\n\
not a json line\n";
        assert_eq!(parse_pi_jsonl(stream).unwrap(), "Polished.");
    }

    #[test]
    fn parse_pi_jsonl_surfaces_error_event_as_agent_cli_message() {
        // Best-effort: a line with type=="error" (or an `error` field) carrying
        // a message surfaces as Err(AgentCli) so it is never typed at the cursor.
        let stream = b"{\"type\":\"error\",\"error\":\"not authenticated\"}\n";
        let err = parse_pi_jsonl(stream).expect_err("error event must be rejected");
        match err.error {
            AiProviderError::AgentCli(message) => assert_eq!(message, "not authenticated"),
            other => panic!("expected AgentCli error, got {other:?}"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn deadline_bounded_returns_timeout_on_pending_operation() {
        // The kill-timeout invariant: a wedged operation (simulated by a
        // `pending` future — no real process, no network) must surface
        // `Err(Timeout)` at the deadline rather than hanging. Mirrors
        // parakeet's `cancellable_dispatch_enforces_deadline_*` tests.
        let result = deadline_bounded(Duration::from_secs(1), || async {
            std::future::pending::<Result<String, MappedAiProviderError>>().await
        })
        .await;

        let err = result.expect_err("expected Timeout, not a response");
        assert_eq!(err.error, AiProviderError::Timeout);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn isolated_command_timeout_terminates_descendants() {
        let marker_dir = TempDir::new().expect("marker directory");
        let marker = marker_dir.path().join("descendant-survived");
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg("(sleep 0.25; printf survived > \"$MARKER\") & wait")
            .env("MARKER", &marker);

        let result = run_isolated_command(command, None, Duration::from_millis(50)).await;
        assert!(matches!(result, Err(ProcessFailure::Timeout)));

        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(
            !marker.exists(),
            "a descendant survived after the CLI process-group deadline"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn dropping_isolated_command_kills_and_reaps_descendants() {
        let marker_dir = TempDir::new().expect("marker directory");
        let marker = marker_dir.path().join("cancelled-descendant-survived");
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg("(sleep 0.25; printf survived > \"$MARKER\") & wait")
            .env("MARKER", &marker);

        let cancelled = tokio::time::timeout(
            Duration::from_millis(50),
            run_isolated_command(command, None, Duration::from_secs(30)),
        )
        .await;
        assert!(cancelled.is_err(), "outer deadline must cancel the command");

        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(
            !marker.exists(),
            "a descendant survived after the outer future was dropped"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn deadline_bounded_passes_through_success_before_deadline() {
        let result = deadline_bounded(Duration::from_secs(5), || async {
            Ok("polished".to_string())
        })
        .await;

        assert_eq!(result.unwrap(), "polished");
    }

    #[tokio::test(start_paused = true)]
    async fn deadline_bounded_passes_through_error_before_deadline() {
        let result = deadline_bounded(Duration::from_secs(5), || async {
            Err(MappedAiProviderError::new(AiProviderError::BadResponse))
        })
        .await;

        assert_eq!(result.unwrap_err().error, AiProviderError::BadResponse);
    }

    #[test]
    fn extract_json_payload_slices_between_braces() {
        assert_eq!(
            extract_json_payload("noise {\"a\":1} tail").unwrap(),
            "{\"a\":1}"
        );
        assert!(extract_json_payload("no braces").is_none());
        // `}` before `{` → empty slice rejected.
        assert!(extract_json_payload("} {").is_none());
    }

    #[test]
    fn map_spawn_error_classifies_not_found_as_unsupported() {
        // "Not installed" / PATH miss → UnsupportedProvider so the executor
        // surfaces the raw transcript (the CLI simply isn't available).
        let not_found = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        assert_eq!(
            map_spawn_error(&not_found),
            AiProviderError::UnsupportedProvider
        );

        let other = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        assert_eq!(map_spawn_error(&other), AiProviderError::Internal);
    }

    #[cfg(unix)]
    #[test]
    fn extract_path_between_markers_finds_path_line() {
        let marker = "VOICETYPR_PATH_PROBE_BOUNDARY";
        let stdout =
            format!("{marker}\nHOME=/home/user\nPATH=/usr/local/bin:/opt/homebrew/bin\n{marker}\n");
        assert_eq!(
            extract_path_between_markers(&stdout, marker).unwrap(),
            "/usr/local/bin:/opt/homebrew/bin"
        );
    }

    #[cfg(unix)]
    #[test]
    fn extract_path_between_markers_strips_quote_wrapping() {
        let marker = "VOICETYPR_PATH_PROBE_BOUNDARY";
        let stdout = format!("{marker}\nPATH=\"/usr/local/bin:/bin\"\n{marker}\n");
        assert_eq!(
            extract_path_between_markers(&stdout, marker).unwrap(),
            "/usr/local/bin:/bin"
        );
    }

    #[test]
    fn fallback_path_includes_common_user_bins() {
        let path = fallback_path();
        assert!(path.contains("/opt/homebrew/bin"));
        assert!(path.contains("/usr/local/bin"));
        // HOME-dependent entries are present when HOME is set (it is in tests).
        if let Ok(home) = std::env::var("HOME") {
            if !home.is_empty() {
                assert!(path.contains(&format!("{home}/.local/bin")));
                assert!(path.contains(&format!("{home}/.bun/bin")));
            }
        }
    }

    // ─── resolver PATH lookup (current-platform, temp fixture) ───────────────
    //
    // `resolve_binary` delegates its current-platform PATH lookup to
    // `which::which_in(binary, Some(path), cwd)` — permission-aware on Unix —
    // then REJECTS any `.cmd`/`.bat` batch shim before `Command::new`. These
    // defend that lookup against a temp executable fixture with an INJECTED
    // PATH string, so the global PATH env is never mutated (racy across
    // concurrent tests) and the ~8s login-shell probe is never triggered.

    /// On-disk fixture name `which` matches on the current platform: `.exe` on
    /// Windows (found via PATHEXT), bare name on Unix.
    fn resolver_fixture_name(base: &str) -> String {
        if cfg!(windows) {
            format!("{base}.exe")
        } else {
            base.to_string()
        }
    }

    /// Write a fixture file; on Unix it MUST carry the executable bit or the
    /// permission-aware lookup correctly rejects it.
    fn write_resolver_fixture(path: &std::path::Path) {
        std::fs::write(path, b"#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(path, perms).unwrap();
        }
    }

    #[test]
    fn which_in_resolves_executable_on_injected_path() {
        // The resolver's primitive resolves a real on-disk executable against
        // an explicit PATH. Inject ONLY the temp dir as the PATH.
        let dir = tempfile::TempDir::new().unwrap();
        let name = resolver_fixture_name("voicetypr-resolver-fixture");
        let bin = dir.path().join(&name);
        write_resolver_fixture(&bin);

        let path = std::env::join_paths(std::iter::once(dir.path())).unwrap();
        let resolved = which::which_in(&name, Some(path), dir.path());
        assert_eq!(resolved.unwrap(), bin);
    }

    #[test]
    fn which_in_returns_err_when_binary_not_on_injected_path() {
        // A miss surfaces an error; the resolver maps this to `None` (`.ok()`)
        // so the caller treats the CLI as "not installed".
        let dir = tempfile::TempDir::new().unwrap();
        let path = std::env::join_paths(std::iter::once(dir.path())).unwrap();
        let resolved = which::which_in("voicetypr-not-a-real-cli-xyz", Some(path), dir.path());
        assert!(resolved.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn which_in_skips_non_executable_match_on_unix() {
        // Permission-aware on Unix: a same-named file WITHOUT the execute bit
        // must NOT resolve. A naive `Path::is_file()` replacement would wrongly
        // return it — this guards the resolver's delegated primitive.
        let dir = tempfile::TempDir::new().unwrap();
        let name = "voicetypr-resolver-noexec";
        let bin = dir.path().join(name);
        std::fs::write(&bin, b"not executable").unwrap();
        // Deliberately leave mode 0o644 (no execute bit).
        let path = std::env::join_paths(std::iter::once(dir.path())).unwrap();
        let resolved = which::which_in(name, Some(path), dir.path());
        assert!(resolved.is_err());
    }

    // ─── extension policy: reject `.cmd`/`.bat` shims before Command::new ────
    //
    // `resolve_binary` post-filters its `which_in` result so a Windows batch
    // shim (`.cmd`/`.bat`) never reaches `Command::new` — executing it would
    // hand the child's argv to `cmd.exe`'s batch parser. The policy is a pure,
    // platform-independent string test on the extension; these pin it
    // case-insensitively without depending on Windows or a real shim on disk.

    #[test]
    fn batch_shim_extensions_are_rejected_case_insensitively() {
        // `.cmd`/`.bat` in ANY casing are the rejected batch extensions.
        assert!(is_windows_batch_shim(Path::new("C:\\Tools\\claude.cmd")));
        assert!(is_windows_batch_shim(Path::new("C:\\Tools\\claude.CMD")));
        assert!(is_windows_batch_shim(Path::new("C:\\Tools\\claude.Cmd")));
        assert!(is_windows_batch_shim(Path::new(
            "/usr/local/bin/claude.bat"
        )));
        assert!(is_windows_batch_shim(Path::new(
            "/usr/local/bin/claude.Bat"
        )));
        assert!(is_windows_batch_shim(Path::new(
            "/usr/local/bin/claude.BAT"
        )));
    }

    #[test]
    fn native_executable_extensions_pass_the_policy() {
        // Native executables and mixed-case variants are NOT batch shims.
        assert!(!is_windows_batch_shim(Path::new("C:\\Tools\\claude.exe")));
        assert!(!is_windows_batch_shim(Path::new("C:\\Tools\\claude.EXE")));
        assert!(!is_windows_batch_shim(Path::new("C:\\Tools\\claude.Exe")));
        assert!(!is_windows_batch_shim(Path::new("C:\\Tools\\edit.com")));
        assert!(!is_windows_batch_shim(Path::new("C:\\Tools\\edit.COM")));
    }

    #[test]
    fn extensionless_path_passes_the_policy() {
        // An extensionless binary (the Unix shape: `claude`, `pi`, `omp`) is
        // never a batch shim.
        assert!(!is_windows_batch_shim(Path::new("/usr/local/bin/claude")));
        assert!(!is_windows_batch_shim(Path::new("claude")));
        assert!(!is_windows_batch_shim(Path::new("/usr/local/bin/omp")));
    }

    #[test]
    fn batch_shim_policy_is_specific_to_cmd_and_bat() {
        // Only `.cmd`/`.bat` are rejected. Unrelated extensions, a `.cmd`-like
        // substring in the FILE NAME, and a trailing non-shim extension all
        // pass — guards against an over-broad or over-eager relaxation.
        assert!(!is_windows_batch_shim(Path::new("C:\\Tools\\claude.ps1")));
        assert!(!is_windows_batch_shim(Path::new("C:\\Tools\\claude.sh")));
        assert!(!is_windows_batch_shim(Path::new(
            "C:\\Tools\\claude.cmd-backup"
        )));
        // `cmd.bat.txt` → final extension is `.txt`, not a shim.
        assert!(!is_windows_batch_shim(Path::new("C:\\Tools\\cmd.bat.txt")));
    }

    #[test]
    fn which_result_then_policy_rejects_bat_shim_fixture() {
        // Integration guard: a `.bat` file that `which_in` WOULD resolve is
        // still rejected once the resolver's filter runs, so no batch file can
        // reach Command::new. The `.bat` name is taken literally on every
        // platform (on Unix `which` matches the whole name; on Windows via
        // PATHEXT), so the fixture resolves and the policy then rejects it.
        let dir = tempfile::TempDir::new().unwrap();
        let name = "voicetypr-shim-fixture.bat";
        let bin = dir.path().join(name);
        write_resolver_fixture(&bin);
        let path = std::env::join_paths(std::iter::once(dir.path())).unwrap();
        let resolved = which::which_in(name, Some(path), dir.path()).unwrap();
        assert_eq!(resolved, bin);
        // …the resolver's extension policy rejects it as a batch shim.
        assert!(is_windows_batch_shim(&resolved));
    }

    #[test]
    fn parse_auth_status_logged_in_json_is_true() {
        let json = br#"{"loggedIn":true,"subscriptionType":"max","email":"user@example.com"}"#;
        assert!(parse_auth_status(json));
    }

    #[test]
    fn parse_auth_status_logged_out_markers_are_false() {
        // loggedIn:false
        assert!(!parse_auth_status(br#"{"loggedIn":false}"#));
        // is_error JSON (logged-out shape)
        assert!(!parse_auth_status(
            br#"{"is_error":true,"content":"Not logged in"}"#
        ));
        // Plain-text logged-out marker
        assert!(!parse_auth_status(b"Not logged in"));
        // Noisy notice line before a logged-out JSON payload
        assert!(!parse_auth_status(
            b"new version available\n{\"loggedIn\":false}"
        ));
    }

    #[test]
    fn parse_auth_status_garbage_defaults_false() {
        assert!(!parse_auth_status(b""));
        assert!(!parse_auth_status(b"totally not json"));
    }

    #[test]
    fn extract_auth_detail_mines_json_message_field() {
        // A logged-out CLI that prints a message-bearing JSON payload yields the
        // CLI's own words for the badge (not a canned hint).
        let json = r#"{"loggedIn":false,"message":"Run `claude /login` to sign in."}"#;
        assert_eq!(extract_auth_detail(json), "Run `claude /login` to sign in.");
        // `content` string field is also mined.
        let json = r#"{"is_error":true,"content":"Not logged in"}"#;
        assert_eq!(extract_auth_detail(json), "Not logged in");
    }

    #[test]
    fn extract_auth_detail_keeps_plain_text_verbatim() {
        // pi/omp-style CLIs print a plain login instruction — keep it as-is.
        assert_eq!(
            extract_auth_detail("Not logged in. Run `omp login` to continue."),
            "Not logged in. Run `omp login` to continue."
        );
    }

    #[test]
    fn extract_auth_detail_returns_empty_when_nothing_useful() {
        // Bare status JSON with no message field → "" (badge falls back to hint).
        assert_eq!(extract_auth_detail(r#"{"loggedIn":false}"#), "");
        assert_eq!(extract_auth_detail(""), "");
        assert_eq!(extract_auth_detail("   "), "");
    }

    #[test]
    fn extract_auth_detail_truncates_chatty_output() {
        let long = "x".repeat(500);
        let detail = extract_auth_detail(&long);
        assert_eq!(detail.chars().count(), 201); // 200 chars + ellipsis
        assert!(detail.ends_with('…'));
    }

    /// Live pi/omp model-listing smokes are intentionally ignored: they require
    /// the CLI to be installed and authenticated. They call only the public
    /// `list_models` API — no model completion, transcript, or session
    /// persistence — and are run manually with `cargo test agent_cli -- --ignored`.
    /// Claude's curated model list is covered by pure tests above; it does not
    /// need a live model-listing call.
    #[tokio::test]
    #[ignore = "requires pi CLI installed + authenticated; no completion request; run with cargo test agent_cli -- --ignored"]
    async fn real_pi_model_listing_smoke() {
        let models = list_models(PROVIDER_PI)
            .await
            .expect("pi model listing should succeed locally");
        let default = models
            .first()
            .expect("pi model listing must include the CLI default entry");
        assert_eq!(default.id, "");
        assert_eq!(default.name, "CLI default");
        assert!(default.cli_default);
        assert!(default.recommended);
        assert_eq!(default.source_provider, None);

        let discovered = models.iter().skip(1).find(|model| {
            !model.id.is_empty()
                && model
                    .source_provider
                    .as_deref()
                    .is_some_and(|provider| !provider.is_empty())
        });
        assert!(
            discovered.is_some(),
            "pi listing must include a discovered model with a nonempty id and source provider; got {models:?}"
        );
    }

    #[tokio::test]
    #[ignore = "requires omp CLI installed + authenticated; no completion request; run with cargo test agent_cli -- --ignored"]
    async fn real_omp_model_listing_smoke() {
        let models = list_models(PROVIDER_OMP)
            .await
            .expect("omp model listing should succeed locally");
        let default = models
            .first()
            .expect("omp model listing must include the CLI default entry");
        assert_eq!(default.id, "");
        assert_eq!(default.name, "CLI default");
        assert!(default.cli_default);
        assert!(default.recommended);
        assert_eq!(default.source_provider, None);

        // Omp model IDs are the exact selectors accepted by `omp --model`,
        // including any provider/version suffixes; never reconstruct them from
        // separate provider/id fields.
        let selectors = models
            .iter()
            .skip(1)
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>();
        assert!(
            !selectors.is_empty(),
            "omp listing must include at least one discovered exact selector; got {models:?}"
        );
        assert!(
            selectors.iter().all(|selector| !selector.trim().is_empty()),
            "omp listing returned an empty exact selector: {selectors:?}"
        );
    }

    /// A real `claude` round-trip is gated behind `#[ignore]` — it requires the
    /// CLI installed + authenticated and burns subscription quota, so it never
    /// runs in CI. Run locally with `cargo test agent_cli -- --ignored`.
    #[tokio::test]
    #[ignore = "requires claude CLI installed + authenticated; not run in CI"]
    async fn real_claude_code_cold_spawn_round_trip() {
        let runtime = AgentCliRuntime::new();
        let request = AiPolishRequest {
            provider_id: PROVIDER_CLAUDE_CODE.to_string(),
            model_id: String::new(),
            input_text: "uhh so basically like um lets fix the bug".to_string(),
            prompt: "Clean up this voice dictation into clear written English. Output only the fixed text.".to_string(),
            timeout_ms: 9_000,
        };
        let result = runtime.polish(&request).await;
        let polished = result.expect("claude cold-spawn polish should succeed locally");
        assert!(!polished.trim().is_empty());
        println!("claude-code polished output: {polished}");
    }

    /// A real `pi` round-trip — gated behind `#[ignore]` (requires the CLI
    /// installed + authenticated to a provider, burns quota, and the first
    /// `resolve_binary` triggers the login-shell PATH probe). Empirically pi
    /// reads stdin in `--mode json`. Run with `cargo test agent_cli -- --ignored`.
    #[tokio::test]
    #[ignore = "requires pi CLI installed + authenticated; not run in CI"]
    async fn real_pi_cold_spawn_round_trip() {
        let runtime = AgentCliRuntime::new();
        let request = AiPolishRequest {
            provider_id: PROVIDER_PI.to_string(),
            model_id: String::new(),
            input_text: "uhh so basically like um lets fix the bug".to_string(),
            prompt: "Clean up this voice dictation into clear written English. Output only the fixed text.".to_string(),
            timeout_ms: 9_000,
        };
        let result = runtime.polish(&request).await;
        let polished = result.expect("pi cold-spawn polish should succeed locally");
        assert!(!polished.trim().is_empty());
        println!("pi polished output: {polished}");
    }

    /// A real `omp` (oh-my-pi) round-trip — gated behind `#[ignore]`. Empirically
    /// omp takes dictation as a positional argv arg (it did not read stdin in
    /// `--mode json`). Run with `cargo test agent_cli -- --ignored`.
    #[tokio::test]
    #[ignore = "requires omp CLI installed + authenticated; not run in CI"]
    async fn real_omp_cold_spawn_round_trip() {
        let runtime = AgentCliRuntime::new();
        let request = AiPolishRequest {
            provider_id: PROVIDER_OMP.to_string(),
            model_id: String::new(),
            input_text: "hello; echo $HOME $(whoami)".to_string(),
            prompt: "Clean up this voice dictation into clear written English. Output only the fixed text.".to_string(),
            timeout_ms: 9_000,
        };
        let result = runtime.polish(&request).await;
        let polished = result.expect("omp cold-spawn polish should succeed locally");
        assert!(!polished.trim().is_empty());
        assert!(
            polished.contains("echo $HOME $(whoami)"),
            "omp must preserve shell metacharacters as literal content: {polished}"
        );
        println!("omp polished output: {polished}");
    }
}
