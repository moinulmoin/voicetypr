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
use super::providers::PROVIDER_CLAUDE_CODE;
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
/// `--system-prompt` slot receives `request.prompt`. Dictated text travels via
/// stdin, NEVER as an argv string (no shell interpolation, no `sh -c`).
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
};

fn spec_for(provider_id: &str) -> Option<&'static AgentCliSpec> {
    match provider_id {
        PROVIDER_CLAUDE_CODE => Some(&CLAUDE_CODE_SPEC),
        _ => None,
    }
}

/// Build the fixed cold-spawn argv. Only the `--system-prompt` value is
/// interpolated (from `request.prompt`); every other token is a constant from
/// the spec table. The dictated text is NOT here — it travels via stdin.
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
        let argv = cold_argv(spec, &prompt);
        let operation = async move {
            cold_spawn_and_collect(&binary_path, &argv, &input_text).await
        };

        deadline_bounded(COLD_SPAWN_TIMEOUT, move || operation).await
    }
}

/// Spawn the binary, feed `input_text` via stdin, wait for exit, and parse the
/// JSON polish result from stdout. Extracted from `polish` so the deadline +
/// kill invariant composes cleanly. The child is created with
/// `kill_on_drop(true)`, so dropping the in-flight future (on deadline elapse)
/// hard-kills the child — no orphans.
async fn cold_spawn_and_collect(
    binary_path: &str,
    argv: &[String],
    input_text: &str,
) -> Result<String, MappedAiProviderError> {
    let mut command = Command::new(binary_path);
    command.args(argv);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());
    // Run from an EMPTY temp cwd so claude discovers no project CLAUDE.md /
    // .claude config (which would re-add the setting sources we just disabled).
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

    // Feed dictated text via stdin, then close (EOF signals the child to flush).
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input_text.as_bytes()).await;
        // stdin drops here, closing the pipe → EOF.
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
    if !status.success() {
        log::warn!(
            "agent-cli polish exited non-zero ({}); stdout={}B",
            status,
            stdout_bytes.len()
        );
        return Err(MappedAiProviderError::new(AiProviderError::BadResponse));
    }

    parse_polish_output(&stdout_bytes)
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

/// Parse the CLI's JSON polish result. Claude's `--output-format json` emits an
/// object with `.result` (string) or `.content` (string / array of
/// `{type:"text", text:"..."}` blocks). A noisy prefix line (update notice,
/// shell banner) before the JSON is tolerated by slicing between the first `{`
/// and the last `}` — mirrors parakeet's `extract_json_payload`.
fn parse_polish_output(stdout: &[u8]) -> Result<String, MappedAiProviderError> {
    let text = String::from_utf8_lossy(stdout);
    let payload = extract_json_payload(&text).unwrap_or(text.as_ref());
    let value: Value = serde_json::from_str(payload)
        .map_err(|_| MappedAiProviderError::new(AiProviderError::BadResponse))?;

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

fn extract_json_payload(raw: &str) -> Option<&str> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    // `.then_some` is eager — it would compute the slice before the guard
    // runs, panicking when `}` precedes `{`. The closure form `.then` is lazy.
    (start < end).then(|| &raw[start..=end])
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
/// the resolved PATH and `--version` exits 0. `authed` reflects a real
/// `<bin> auth status` probe (bounded, JSON-first with a plain fallback); we
/// NEVER read credential files. On any probe failure `authed` defaults to
/// false so the UI surfaces a "log in" hint rather than a guaranteed-to-fail
/// polish attempt.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentCliProbe {
    pub installed: bool,
    pub authed: bool,
}

impl AgentCliProbe {
    fn unavailable() -> Self {
        Self {
            installed: false,
            authed: false,
        }
    }
}

/// Probe an agent-CLI provider: locate its binary on the resolved PATH, run
/// `<bin> --version`, then run `<bin> auth status` to detect a logged-in
/// account. Fixed argv — NEVER reads credential files. Cache-friendly (the
/// frontend calls this at setup, not per-dictation).
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

    // Real auth check: run `<bin> auth status` (JSON first, plain fallback),
    // bounded by a short timeout, in an empty temp cwd. NEVER reads credential
    // files — only the CLI's own auth-status output. Defaults to false on any
    // failure so the UI shows a "log in" hint instead of a failing polish.
    let authed = check_auth_status(&binary_path).await;

    AgentCliProbe { installed, authed }
}

/// Run `<binary> auth status` with a short timeout and report whether the CLI
/// reports a logged-in account. Tries `--output-format json` first (clean parse
/// of `loggedIn`); if that errors (unsupported flag / older CLI), retries plain
/// `auth status`. NEVER reads credential files — only the CLI's own output.
/// Defaults to `false` on any failure or timeout.
async fn check_auth_status(binary_path: &str) -> bool {
    /// Bound for the auth-status probe. A wedged CLI must not block detection.
    const AUTH_STATUS_TIMEOUT: Duration = Duration::from_secs(3);

    let path = resolved_path().await;

    // `claude auth status` already emits JSON ({"loggedIn":true,...}) on its own.
    // The `auth` subcommand does NOT accept --output-format (verified: it exits
    // non-zero on the flag), so we call it plain.
    match run_auth_status(binary_path, &["auth", "status"], &path, AUTH_STATUS_TIMEOUT).await {
        Some(stdout) => parse_auth_status(&stdout),
        None => false,
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
    fn parse_polish_output_reads_result_field() {
        let json = br#"{"type":"result","result":"Hello, world.","is_error":false}"#;
        assert_eq!(
            parse_polish_output(json).unwrap(),
            "Hello, world."
        );
    }

    #[test]
    fn parse_polish_output_reads_content_string() {
        let json = br#"{"content":"Raw content here."}"#;
        assert_eq!(parse_polish_output(json).unwrap(), "Raw content here.");
    }

    #[test]
    fn parse_polish_output_reads_content_text_blocks() {
        let json = br#"{"content":[{"type":"text","text":"line one"},{"type":"text","text":"line two"}]}"#;
        assert_eq!(
            parse_polish_output(json).unwrap(),
            "line one\nline two"
        );
    }

    #[test]
    fn parse_polish_output_recovers_from_noisy_prefix() {
        // A real CLI may emit an update notice or shell banner before the JSON.
        // We slice between the first `{` and the last `}` (parakeet precedent).
        let noisy = b"A new version of claude is available. Run npm i -g @anthropic-ai/claude-code to update.\n{\"result\":\"Polished text.\"}\n";
        assert_eq!(parse_polish_output(noisy).unwrap(), "Polished text.");
    }

    #[test]
    fn parse_polish_output_rejects_garbage() {
        assert!(parse_polish_output(b"not json at all").is_err());
        assert!(parse_polish_output(b"{}").is_err());
        assert!(parse_polish_output(b"{\"unrelated\":\"field\"}").is_err());
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
}
