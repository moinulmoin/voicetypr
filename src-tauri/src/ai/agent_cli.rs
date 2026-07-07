//! Cold-spawn runtime for agent-CLI polish providers (Claude Code first).
//!
//! Phase 4C-i scope: COLD-SPAWN ONLY. The warm-session manager (`CliSessionManager`)
//! lands in 4C-ii alongside pi/omp and the "Preparing…" pill.
//!
//! # Spawning model
//!
//! The user's installed coding CLI is spawned headless via raw
//! `tokio::process::Command` (NOT `tauri-plugin-shell` — the shell scope governs
//! the untrusted webview; trusted Rust native code is not subject to it, and we
//! avoid a wildcard `shell:allow-execute`). Text to polish travels via **stdin**;
//! the polished text is parsed from the CLI's JSON output. A hard kill-timeout
//! bounds the spawn so a wedged CLI cannot hang dictation — on elapse the child
//! is hard-killed (`start_kill` + `kill_on_drop(true)`) and the executor falls
//! back to the raw transcript via the existing `ai_error` path (no new plumbing).
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
//! Dictated text is fed via stdin — NEVER a shell string, NEVER `sh -c`. The
//! argv is fixed in shape; only the `--system-prompt` slot receives
//! `request.prompt`. Isolation flags (`--tools ""`, `--strict-mcp-config`,
//! `--setting-sources ""`) prevent the CLI from touching the filesystem,
//! running tools, or loading project CLAUDE.md/skills/plugins/hooks. The child
//! also runs from an EMPTY temp cwd so claude discovers no project config.

use super::contract::AiPolishRequest;
use super::error::{AiProviderError, MappedAiProviderError};
use super::providers::{PROVIDER_CLAUDE_CODE, PROVIDER_OMP, PROVIDER_PI};
use serde_json::Value;
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Hard wall-clock cap for a single cold-spawn polish. The executor's own
/// `polish()` budget also bounds this (per-runtime `timeout_ms`), but a CLI can
/// wedge independently of HTTP semantics, so we hard-kill at this deadline
/// regardless and surface `Err(Timeout)` → raw-transcript fallback.
const COLD_SPAWN_TIMEOUT: Duration = Duration::from_secs(9);

/// Per-provider cold-spawn spec. The argv is FIXED in shape — only the
/// `--system-prompt` slot receives `request.prompt`. Isolation flags
/// (`--tools ""`, `--strict-mcp-config`, `--setting-sources ""`, or pi/omp's
/// `--no-tools --no-session --no-skills --no-rules`) prevent the CLI from
/// touching the filesystem, running tools, or loading project config; the child
/// also runs from an EMPTY temp cwd so no project is discovered.
///
/// The spec also selects how dictated text reaches the child (`InputMode`) and
/// how the polished result is parsed from the child's stdout (`OutputParser`),
/// plus how auth is probed (`AuthMode`) — so one runtime backs claude-code,
/// pi, and omp end-to-end.
struct AgentCliSpec {
    /// Provider id this spec serves (documentational; the dispatch match in
    /// `spec_for` is the source of truth). Kept on the struct so the spec
    /// table reads as a self-describing row.
    #[allow(dead_code)]
    provider_id: &'static str,
    binary: &'static str,
    /// Fixed argv BEFORE the `--system-prompt` value (the flag itself is the
    /// last element here).
    cold_argv_prefix: &'static [&'static str],
    /// Fixed argv AFTER the `--system-prompt` value.
    cold_argv_suffix: &'static [&'static str],
    /// How dictated text reaches the child process.
    input_mode: InputMode,
    /// How the polished result is parsed from stdout.
    output: OutputParser,
    /// How `probe()` determines the authed state.
    auth: AuthMode,
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
    /// omp did not read stdin in `--mode json`; the positional arg is its input
    /// channel.
    PositionalArg,
}

/// Which stdout shape a CLI emits, selecting the parser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputParser {
    /// claude's `--output-format json`: a single object with `.result` (string)
    /// or `.content` (string / `{type:"text",text:"..."}` blocks).
    ClaudeJson,
    /// pi/omp's `--mode json`: a JSONL event stream (one JSON object per line).
    /// The polished text is the LAST assistant message's
    /// `message.content[].text` (type=="text").
    PiJsonl,
}

/// How `probe()` decides the `authed` flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AuthMode {
    /// Run `<bin> auth status` and parse its JSON for a logged-in account
    /// (claude-code). Defaults to false on any probe failure.
    RealAuthStatus,
    /// Installed => authed (optimistic). pi/omp have no clean `auth status`
    /// command; a not-authed failure surfaces during the next polish via the
    /// existing AgentCli-error toast (the executor falls back to the raw
    /// transcript). No auth command is run, so detection stays cheap.
    Optimistic,
}

/// Claude Code spec (4C-i first cut).
///
/// `claude -p --setting-sources "" --tools "" --strict-mcp-config --no-chrome
///  --model haiku --system-prompt <PROMPT> --output-format json`, dictated text
/// on stdin. `--setting-sources ""` skips CLAUDE.md/skills/plugins/hooks but
/// KEEPS credentials (Keychain); `--no-chrome` skips the terminal UI, and the
/// child runs from an EMPTY temp cwd so claude discovers no project config.
/// (`--bare`, used in an earlier cut, skips credentials too and returns "Not
/// logged in" for logged-in users — it is intentionally absent.) Output is
/// JSON; we read `.result` (string form) and fall back to `.content[].text`
/// (the streamed-message shape).
const CLAUDE_CODE_SPEC: AgentCliSpec = AgentCliSpec {
    provider_id: PROVIDER_CLAUDE_CODE,
    binary: "claude",
    cold_argv_prefix: &[
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
    ],
    cold_argv_suffix: &["--output-format", "json"],
    input_mode: InputMode::Stdin,
    output: OutputParser::ClaudeJson,
    auth: AuthMode::RealAuthStatus,
};

/// pi spec. `pi -p --no-tools --no-session --thinking off --mode json
/// --system-prompt <PROMPT>`, dictated text on stdin (pi reads stdin in
/// `--mode json`). No `--model` — pi is multi-provider and we cannot assume
/// which provider the user authed; it uses its default. Output is a JSONL
/// event stream; the polished text is the last assistant message's text.
/// Empty cold_argv_suffix: the `--system-prompt` value is the final argv token.
const PI_SPEC: AgentCliSpec = AgentCliSpec {
    provider_id: PROVIDER_PI,
    binary: "pi",
    cold_argv_prefix: &[
        "-p",
        "--no-tools",
        "--no-session",
        "--thinking",
        "off",
        "--mode",
        "json",
        "--system-prompt",
    ],
    cold_argv_suffix: &[],
    input_mode: InputMode::Stdin,
    output: OutputParser::PiJsonl,
    auth: AuthMode::Optimistic,
};

/// omp (oh-my-pi) spec. `omp -p --no-tools --no-session --no-skills --no-rules
/// --thinking off --mode json --system-prompt <PROMPT> <DICTATION>`, dictated
/// text as the FINAL positional argv element (omp did NOT read stdin in
/// `--mode json`; positional works). No `--model` — omp is multi-provider.
/// Output is the same JSONL event stream as pi. The dictation is appended at
/// spawn time as a discrete `Command::arg` value (never a shell string).
const OMP_SPEC: AgentCliSpec = AgentCliSpec {
    provider_id: PROVIDER_OMP,
    binary: "omp",
    cold_argv_prefix: &[
        "-p",
        "--no-tools",
        "--no-session",
        "--no-skills",
        "--no-rules",
        "--thinking",
        "off",
        "--mode",
        "json",
        "--system-prompt",
    ],
    cold_argv_suffix: &[],
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

/// Build the fixed cold-spawn argv. Only the `--system-prompt` value is
/// interpolated (from `request.prompt`); every other token is a constant from
/// the spec table. Dictated text is NOT here — it travels via stdin (Stdin) or
/// is appended as the final argv element at spawn time (PositionalArg).
fn cold_argv(spec: &AgentCliSpec, prompt: &str) -> Vec<String> {
    let mut argv: Vec<String> = spec.cold_argv_prefix.iter().map(|s| s.to_string()).collect();
    argv.push(prompt.to_string());
    argv.extend(spec.cold_argv_suffix.iter().map(|s| s.to_string()));
    argv
}

/// Cold-spawn runtime. Stateless in 4C-i (no warm session) — each `polish` call
/// spawns a fresh child. The warm-session manager (`CliSessionManager`) is 4C-ii.
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
    pub async fn polish(
        &self,
        request: &AiPolishRequest,
    ) -> Result<String, MappedAiProviderError> {
        let spec = spec_for(&request.provider_id).ok_or_else(|| {
            MappedAiProviderError::new(AiProviderError::UnsupportedProvider)
        })?;

        let binary_path = resolve_binary(spec.binary)
            .await
            .ok_or_else(|| MappedAiProviderError::new(AiProviderError::UnsupportedProvider))?;
        // The spawn+wait+parse future owns the child; `kill_on_drop(true)`
        // ensures a wedged child is hard-killed when the deadline drops it.
        let prompt = request.prompt.clone();
        let input_text = request.input_text.clone();
        let input_mode = spec.input_mode;
        let output = spec.output;
        let argv = cold_argv(spec, &prompt);
        let operation = async move {
            cold_spawn_and_collect(&binary_path, &argv, input_mode, &input_text, output).await
        };

        deadline_bounded(COLD_SPAWN_TIMEOUT, move || operation).await
    }
}

/// Spawn the binary, deliver `input_text` per `input_mode`, wait for exit, and
/// parse the polish result from stdout via `output`. Extracted from `polish` so
/// the deadline + kill invariant composes cleanly. The child is created with
/// `kill_on_drop(true)`, so dropping the in-flight future (on deadline elapse)
/// hard-kills the child — no orphans.
async fn cold_spawn_and_collect(
    binary_path: &str,
    argv: &[String],
    input_mode: InputMode,
    input_text: &str,
    output: OutputParser,
) -> Result<String, MappedAiProviderError> {
    let mut command = Command::new(binary_path);
    command.args(argv);
    match input_mode {
        InputMode::Stdin => {
            // Dictation travels via stdin; argv holds no dictated text.
            command.stdin(Stdio::piped());
        }
        InputMode::PositionalArg => {
            // Dictation appended as a DISCRETE argv value (a single OS exec
            // argument) — never a shell string, never `sh -c`, so it is
            // injection-safe regardless of content. stdin is null (omp does
            // not read stdin in --mode json).
            command.arg(input_text);
            command.stdin(Stdio::null());
        }
    }
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());
    // Run from an EMPTY temp cwd so the CLI discovers no project config
    // (CLAUDE.md / .claude / .pi / etc., which would re-add the setting
    // sources we just disabled).
    command.current_dir(std::env::temp_dir());
    // Dodge the tmux auto-start hook some shells fire under `-i -l`.
    command.env("ZSH_TMUX_AUTOSTART", "false");
    // The resolved binary is typically a node/bun script whose shebang
    // (`#!/usr/bin/env node`) resolves its interpreter via PATH; under the
    // stripped macOS GUI PATH the interpreter isn't found, so the CLI fails to
    // launch. Restore the resolved login-shell PATH (cached) for the child.
    command.env("PATH", resolved_path().await);
    // Hard-kill on drop — the deadline-bounded future drops the child on
    // timeout, so a wedged CLI cannot linger as an orphan.
    command.kill_on_drop(true);

    #[cfg(target_os = "windows")]
    {
        // Don't flash a console window for the headless CLI on Windows.
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = command
        .spawn()
        .map_err(|error| MappedAiProviderError::new(map_spawn_error(&error)))?;

    if input_mode == InputMode::Stdin {
        // Feed dictated text via stdin, then close (EOF signals the child to
        // flush). PositionalArg already supplied the text as an argv value.
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(input_text.as_bytes()).await;
            // stdin drops here, closing the pipe → EOF.
        }
    }

    // Read stdout concurrently with wait so a child that writes then blocks
    // still surfaces its output before the deadline kills it.
    let mut stdout = child.stdout.take().expect("stdout piped");
    let stdout_task = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf).await;
        buf
    });

    let status = child
        .wait()
        .await
        .map_err(|_| MappedAiProviderError::new(AiProviderError::Internal))?;

    let stdout_bytes = stdout_task.await.unwrap_or_default();
    // Parse stdout REGARDLESS of exit status. A CLI that fails (e.g. not logged
    // in) commonly exits non-zero AND emits its own message on stdout — claude:
    // {"result":"Not logged in · Please run /login","is_error":true}; pi/omp: an
    // error event. The parser maps that to Err(AgentCli(<message>)) so the user
    // sees the CLI's OWN words; returning a generic BadResponse here would
    // swallow it. The parser still returns BadResponse when stdout carries
    // neither a result nor a message.
    if !status.success() {
        log::warn!(
            "agent-cli polish exited non-zero ({}); parsing stdout ({}B) for the CLI's own message",
            status,
            stdout_bytes.len()
        );
    }

    parse_cli_output(output, &stdout_bytes)
}

/// Deadline-bound an operation, returning `Err(Timeout)` on elapse. Extracted
/// from `polish` so the kill-timeout invariant is unit-testable without
/// spawning a real process: a test passes a `pending` future and asserts the
/// timeout fires within the deadline (mirrors parakeet's `timed_request`).
async fn deadline_bounded<F, Fut>(
    timeout: Duration,
    make_operation: F,
) -> Result<String, MappedAiProviderError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<String, MappedAiProviderError>>,
{
    match tokio::time::timeout(timeout, make_operation()).await {
        Ok(result) => result,
        Err(_) => Err(MappedAiProviderError::new(AiProviderError::Timeout)),
    }
}

/// Select the parser for a CLI's stdout shape. claude emits a single JSON
/// object; pi/omp emit a JSONL event stream.
fn parse_cli_output(
    output: OutputParser,
    stdout: &[u8],
) -> Result<String, MappedAiProviderError> {
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
    if value.get("is_error").and_then(Value::as_bool).unwrap_or(false) {
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
                return trimmed.to_string();
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
            return Err(MappedAiProviderError::new(AiProviderError::AgentCli(message)));
        }
        if let Some(message_text) = assistant_message_text(&value) {
            if !message_text.trim().is_empty() {
                last_assistant_text = Some(message_text);
            }
        }
    }
    last_assistant_text
        .ok_or_else(|| MappedAiProviderError::new(AiProviderError::BadResponse))
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
                return Some(trimmed.to_string());
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

/// Cached resolved PATH. A Finder-launched macOS GUI app gets only
/// `/usr/bin:/bin:/usr/sbin:/sbin`; we replace it with the user's real
/// login-shell PATH (or a minimal fallback). Resolved once, reused for every
/// cold spawn + probe.
static RESOLVED_PATH: OnceLock<String> = OnceLock::new();

/// Resolve a CLI binary name to its absolute path using the user's real
/// login-shell PATH. Returns `None` if not found (caller treats as "not
/// installed" → raw-transcript fallback / "Install" hint).
pub async fn resolve_binary(binary: &str) -> Option<String> {
    let path = resolved_path().await;
    for dir in path_split(&path) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidate = dir.join(binary);
        if is_executable(&candidate) {
            return candidate.to_str().map(|s| s.to_string());
        }
    }
    None
}

async fn resolved_path() -> String {
    if let Some(cached) = RESOLVED_PATH.get() {
        return cached.clone();
    }
    let resolved = resolve_login_shell_path()
        .await
        .unwrap_or_else(fallback_path);
    // Race-tolerant set: whichever task won the race produced an equivalent PATH.
    let _ = RESOLVED_PATH.set(resolved.clone());
    RESOLVED_PATH.get().cloned().unwrap_or(resolved)
}

#[cfg(unix)]
async fn resolve_login_shell_path() -> Option<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    const MARKER: &str = "VOICETYPR_PATH_PROBE_BOUNDARY";
    let script = format!("printf '%s\\n' '{MARKER}'; env; printf '%s\\n' '{MARKER}'");

    let mut command = tokio::process::Command::new(&shell);
    command.args(["-ilc", &script]);
    command.env("ZSH_TMUX_AUTOSTART", "false");
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());
    command.kill_on_drop(true);

    let output = tokio::time::timeout(Duration::from_secs(8), command.output())
        .await
        .ok()?
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    extract_path_between_markers(&stdout, MARKER)
}

#[cfg(not(unix))]
async fn resolve_login_shell_path() -> Option<String> {
    // Windows GUI apps inherit the user's full PATH from the registry (set by
    // installers), so the login-shell hack is unnecessary. The registry PATH is
    // already in std::env — no probe needed.
    None
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

#[cfg(unix)]
fn path_split(path: &str) -> Vec<std::path::PathBuf> {
    std::env::split_paths(path).collect()
}

#[cfg(not(unix))]
fn path_split(path: &str) -> Vec<std::path::PathBuf> {
    // Windows uses ';' as the separator; std::env::split_paths is platform-aware.
    std::env::split_paths(path).collect()
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    match std::fs::metadata(path) {
        Ok(meta) => meta.is_file() && (meta.permissions().mode() & 0o111 != 0),
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_executable(path: &std::path::Path) -> bool {
    std::fs::metadata(path).is_ok_and(|meta| meta.is_file())
}

// ─── probe (detection) ──────────────────────────────────────────────────────

/// Detection result for an agent-CLI provider. `installed` = the binary is on
/// the resolved PATH and `--version` exits 0. `authed` is per-CLI: claude-code
/// reflects a real bounded `<bin> auth status` probe; pi/omp are optimistic
/// (installed => authed). We NEVER read credential files. On any real-probe
/// failure `authed` defaults to false so the UI surfaces a "log in" hint rather
/// than a guaranteed-to-fail polish attempt.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentCliProbe {
    pub installed: bool,
    pub authed: bool,
    /// The CLI's OWN auth-status message (its login guidance, NOT a canned
    /// string), shown on the sign-in badge when installed-but-not-authed.
    /// Empty when nothing useful was captured so the UI falls back to its
    /// static hint.
    pub detail: String,
}

impl AgentCliProbe {
    fn unavailable() -> Self {
        Self {
            installed: false,
            authed: false,
            detail: String::new(),
        }
    }
}

/// Probe an agent-CLI provider: locate its binary on the resolved PATH and run
/// `<bin> --version`. Auth detection is per-CLI (`spec.auth`): claude-code runs
/// a real `<bin> auth status`; pi/omp are optimistic (installed => authed).
/// Fixed argv — NEVER reads credential files. Cache-friendly (the frontend
/// calls this at setup, not per-dictation).
pub async fn probe(provider: &str) -> AgentCliProbe {
    let Some(spec) = spec_for(provider) else {
        return AgentCliProbe::unavailable();
    };
    let Some(binary_path) = resolve_binary(spec.binary).await else {
        return AgentCliProbe::unavailable();
    };

    let mut command = Command::new(&binary_path);
    command.arg("--version");
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());
    command.kill_on_drop(true);
    // Same PATH-restore rationale as cold_spawn_and_collect: `claude` is a
    // node/bun script whose shebang needs its interpreter on PATH.
    command.env("PATH", resolved_path().await);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let installed = match tokio::time::timeout(Duration::from_secs(5), command.output()).await {
        Ok(Ok(output)) => output.status.success(),
        _ => false,
    };

    if !installed {
        return AgentCliProbe::unavailable();
    }

    // Auth detection is per-CLI: claude-code has a real `<bin> auth status`
    // probe (bounded, JSON-first, plain-text fallback; NEVER reads credential
    // files — only the CLI's own output). pi/omp have no clean `auth status`,
    // so they are OPTIMISTIC — installed => authed — and a not-authed failure
    // surfaces during the next polish via the AgentCli-error toast (the
    // executor falls back to the raw transcript). Defaults to false on any
    // real-probe failure so the UI shows a "log in" hint instead of a failing
    // polish.
    match spec.auth {
        AuthMode::RealAuthStatus => {
            let auth_status_raw = check_auth_status(&binary_path).await;
            let authed = parse_auth_status(auth_status_raw.as_bytes());
            // The badge shows the CLI's OWN login guidance (its exact words)
            // instead of a canned hint when available.
            let detail = extract_auth_detail(&auth_status_raw);
            AgentCliProbe {
                installed,
                authed,
                detail,
            }
        }
        AuthMode::Optimistic => AgentCliProbe {
            installed,
            authed: true,
            detail: String::new(),
        },
    }
}

/// Run `<binary> auth status` with a short timeout and return the raw stdout.
/// Empty on non-zero exit, spawn failure, or timeout (all map to "auth unknown
/// → not authed"). The caller derives the `authed` bool and a human-readable
/// badge line from it. `claude auth status` already emits JSON on its own; the
/// `auth` subcommand does NOT accept `--output-format` (it exits non-zero on
/// the flag), so we call it plain. NEVER reads credential files.
async fn check_auth_status(binary_path: &str) -> String {
    /// Bound for the auth-status probe. A wedged CLI must not block detection.
    const AUTH_STATUS_TIMEOUT: Duration = Duration::from_secs(3);

    let path = resolved_path().await;

    match run_auth_status(binary_path, &["auth", "status"], &path, AUTH_STATUS_TIMEOUT).await {
        Some(stdout) => String::from_utf8_lossy(&stdout).trim().to_string(),
        None => String::new(),
    }
}

/// Spawn `<binary> auth status <extra>` in an empty temp cwd with the resolved
/// PATH, bounded by `timeout`. Returns the stdout bytes on a clean (exit 0)
/// completion, or `None` on non-zero exit, spawn failure, or timeout (all map
/// to "auth unknown → false").
async fn run_auth_status(
    binary_path: &str,
    extra_argv: &[&str],
    path: &str,
    timeout: Duration,
) -> Option<Vec<u8>> {
    let mut command = Command::new(binary_path);
    command.args(extra_argv);
    // Empty temp cwd so claude discovers no project config (matches the polish
    // spawn isolation).
    command.current_dir(std::env::temp_dir());
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());
    command.env("ZSH_TMUX_AUTOSTART", "false");
    command.env("PATH", path);
    command.kill_on_drop(true);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    match tokio::time::timeout(timeout, command.output()).await {
        Ok(Ok(output)) if output.status.success() => Some(output.stdout),
        _ => None,
    }
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
    if obj.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false) {
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(argv.len(), CLAUDE_CODE_SPEC.cold_argv_prefix.len() + 1 + CLAUDE_CODE_SPEC.cold_argv_suffix.len());
    }

    #[test]
    fn pi_cold_argv_is_fixed_except_for_prompt_slot() {
        // pi: `pi -p --no-tools --no-session --thinking off --mode json
        // --system-prompt <PROMPT>`, dictated text via stdin (NOT in argv), no
        // --model (pi is multi-provider; uses its default). Empty suffix.
        let argv = cold_argv(&PI_SPEC, "my system prompt");
        assert_eq!(
            argv,
            vec![
                "-p",
                "--no-tools",
                "--no-session",
                "--thinking",
                "off",
                "--mode",
                "json",
                "--system-prompt",
                "my system prompt",
            ]
        );
        // Dictated text is fed via stdin (InputMode::Stdin) — it never appears
        // in argv, and pi carries no --model pin.
        assert!(!argv.iter().any(|arg| arg.starts_with("--model")));
        assert!(argv.iter().all(|arg| !arg.contains("sh -c")));
        assert_eq!(
            argv.len(),
            PI_SPEC.cold_argv_prefix.len() + 1 + PI_SPEC.cold_argv_suffix.len()
        );
    }

    #[test]
    fn omp_cold_argv_is_fixed_except_for_prompt_slot() {
        // omp: `omp -p --no-tools --no-session --no-skills --no-rules --thinking
        // off --mode json --system-prompt <PROMPT>`, dictated text as the FINAL
        // positional argv element (appended at spawn, not in cold_argv), no
        // --model. Empty suffix so the prompt value is the last cold_argv token.
        let argv = cold_argv(&OMP_SPEC, "my system prompt");
        assert_eq!(
            argv,
            vec![
                "-p",
                "--no-tools",
                "--no-session",
                "--no-skills",
                "--no-rules",
                "--thinking",
                "off",
                "--mode",
                "json",
                "--system-prompt",
                "my system prompt",
            ]
        );
        // Security invariant: cold_argv holds only the fixed prefix + prompt;
        // the dictated text is appended as a DISCRETE argv value at spawn time
        // (PositionalArg mode), never a shell string, never `sh -c`.
        assert!(!argv.iter().any(|arg| arg.starts_with("--model")));
        assert!(argv.iter().all(|arg| !arg.contains("sh -c")));
        assert_eq!(OMP_SPEC.input_mode, InputMode::PositionalArg);
        assert_eq!(OMP_SPEC.cold_argv_suffix.len(), 0);
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
        assert_eq!(spec_for(PROVIDER_CLAUDE_CODE).map(|s| s.binary), Some("claude"));
        assert_eq!(spec_for(PROVIDER_PI).map(|s| s.binary), Some("pi"));
        assert_eq!(spec_for(PROVIDER_OMP).map(|s| s.binary), Some("omp"));
        assert!(spec_for("unknown").is_none());
    }

    #[test]
    fn parse_claude_json_reads_result_field() {
        let json = br#"{"type":"result","result":"Hello, world.","is_error":false}"#;
        assert_eq!(
            parse_claude_json(json).unwrap(),
            "Hello, world."
        );
    }

    #[test]
    fn parse_claude_json_reads_content_string() {
        let json = br#"{"content":"Raw content here."}"#;
        assert_eq!(parse_claude_json(json).unwrap(), "Raw content here.");
    }

    #[test]
    fn parse_claude_json_reads_content_text_blocks() {
        let json = br#"{"content":[{"type":"text","text":"line one"},{"type":"text","text":"line two"}]}"#;
        assert_eq!(
            parse_claude_json(json).unwrap(),
            "line one\nline two"
        );
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
        let json = r#"{"type":"result","result":"Not logged in · Please run /login","is_error":true}"#.as_bytes();
        let err = parse_claude_json(json)
            .expect_err("an is_error result must NOT become polished text");
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
        assert_eq!(map_spawn_error(&not_found), AiProviderError::UnsupportedProvider);

        let other = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        assert_eq!(map_spawn_error(&other), AiProviderError::Internal);
    }

    #[cfg(unix)]
    #[test]
    fn extract_path_between_markers_finds_path_line() {
        let marker = "VOICETYPR_PATH_PROBE_BOUNDARY";
        let stdout = format!(
            "{marker}\nHOME=/home/user\nPATH=/usr/local/bin:/opt/homebrew/bin\n{marker}\n"
        );
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
        assert!(!parse_auth_status(b"new version available\n{\"loggedIn\":false}"));
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
            input_text: "uhh so basically like um lets fix the bug".to_string(),
            prompt: "Clean up this voice dictation into clear written English. Output only the fixed text.".to_string(),
            timeout_ms: 9_000,
        };
        let result = runtime.polish(&request).await;
        let polished = result.expect("omp cold-spawn polish should succeed locally");
        assert!(!polished.trim().is_empty());
        println!("omp polished output: {polished}");
    }
}
