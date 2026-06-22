//! Opt-in, anonymous error reporting (Sentry SDK -> self-hosted Bugsink).
//!
//! Privacy posture (non-negotiable):
//! - Disabled by default. Only active when the user has opted in AND a DSN was
//!   baked in at build time via `VOICETYPR_SENTRY_DSN`. OSS / dev builds have no
//!   DSN, so the client is never created (fully inert).
//! - No native minidumps: the `tauri-plugin-sentry` `minidump` feature is OFF and
//!   `minidump::init` is never called, so no raw process-memory snapshot (which
//!   could contain audio PCM, transcripts, or decrypted keys) is ever captured.
//! - No breadcrumbs, no PII, `traces_sample_rate = 0`.
//! - `before_send` rebuilds every event from a tiny allowlist and scrubs free
//!   text, so transcripts, audio, file paths, URLs, keys, emails, and target
//!   app/window names never leave the device.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};

use regex::Regex;
use sentry::protocol::Event;
use sentry::ClientInitGuard;

/// DSN baked in at build time for official builds. Absent in OSS / dev builds,
/// in which case telemetry is fully compiled-inert (no client is ever created).
const SENTRY_DSN: Option<&str> = option_env!("VOICETYPR_SENTRY_DSN");

/// Store file (tauri-plugin-store) + keys that hold consent state. The store is
/// a flat top-level JSON object, so a raw reader can parse these keys before the
/// Tauri app (and its plugins) are built.
const SETTINGS_STORE_FILE: &str = "settings";
pub const KEY_TELEMETRY_ENABLED: &str = "telemetry_enabled";
pub const KEY_TELEMETRY_INSTALL_ID: &str = "telemetry_install_id";

/// In-process consent gate, read on every `before_send`. Revoking consent stops
/// egress immediately within the session; a full re-enable still needs a restart
/// because the Sentry client/plugin are only wired at startup.
static TELEMETRY_ENABLED: AtomicBool = AtomicBool::new(false);

/// True when this build is capable of reporting at all (a DSN was compiled in).
pub fn is_available() -> bool {
    SENTRY_DSN.is_some()
}

/// Whether reporting is currently allowed this session.
pub fn is_enabled() -> bool {
    TELEMETRY_ENABLED.load(Ordering::Relaxed)
}

/// Flip the in-process gate. Disabling takes effect immediately.
pub fn set_enabled(enabled: bool) {
    TELEMETRY_ENABLED.store(enabled, Ordering::Relaxed);
}

// --- Scrubbing ---------------------------------------------------------------

static RE_PATH: LazyLock<Regex> = LazyLock::new(|| {
    // Unix home/system paths, Windows user paths, and JS `app:///`/`file://` URIs.
    Regex::new(
        r#"(?i)([a-z]:\\[^\s"'`]*|/(?:users|home|var|private|tmp|library|applications)/[^\s"'`]*|(?:file|app|asset)://[^\s"'`]*)"#,
    )
    .expect("valid path regex")
});
static RE_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)https?://[^\s"'`]+"#).expect("valid url regex"));
static RE_EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}"#).expect("valid email regex")
});
static RE_LONG_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    // Long opaque runs: API keys, bearer tokens, hashes, foreign UUIDs.
    Regex::new(r#"\b[A-Za-z0-9_\-]{24,}\b"#).expect("valid token regex")
});

/// Redacts free-form text that may carry user content or environment detail.
pub fn scrub_text(input: &str) -> String {
    let mut s = RE_PATH.replace_all(input, "[path]").into_owned();
    s = RE_URL.replace_all(&s, "[url]").into_owned();
    s = RE_EMAIL.replace_all(&s, "[email]").into_owned();
    s = RE_LONG_TOKEN.replace_all(&s, "[redacted]").into_owned();
    s
}

/// Rebuilds an event from an allowlist: keep the exception type + scrubbed
/// message + stack *shape*; drop every section that can carry PII; re-attach
/// only coarse, non-identifying tags.
pub fn scrub_event(mut event: Event<'static>, install_id: Option<&str>) -> Event<'static> {
    // Drop entire PII-bearing sections outright.
    event.user = None;
    event.request = None;
    event.server_name = None;
    event.extra.clear();
    event.contexts.clear();
    event.tags.clear();
    event.breadcrumbs.values.clear();

    // Scrub the top-level message if present.
    if let Some(message) = event.message.take() {
        event.message = Some(scrub_text(&message));
    }

    // Scrub exception messages and strip file paths / locals from frames, but
    // keep function / module / line — that is the stack shape we want.
    for exception in event.exception.values.iter_mut() {
        if let Some(value) = exception.value.take() {
            exception.value = Some(scrub_text(&value));
        }
        for stacktrace in exception
            .stacktrace
            .iter_mut()
            .chain(exception.raw_stacktrace.iter_mut())
        {
            scrub_stacktrace(stacktrace);
        }
    }

    // Threads carry the same stack-frame leak vector (paths, locals, context).
    for thread in event.threads.values.iter_mut() {
        for stacktrace in thread
            .stacktrace
            .iter_mut()
            .chain(thread.raw_stacktrace.iter_mut())
        {
            scrub_stacktrace(stacktrace);
        }
    }

    // Drop remaining free-text / path-bearing sections entirely.
    event.logentry = None;
    event.debug_meta = Default::default();

    // Re-attach a tiny, non-identifying allowlist.
    event.tags.insert("os".into(), std::env::consts::OS.into());
    event
        .tags
        .insert("arch".into(), std::env::consts::ARCH.into());
    event
        .tags
        .insert("app_version".into(), env!("CARGO_PKG_VERSION").into());
    if let Some(id) = install_id {
        event.tags.insert("install_id".into(), id.into());
    }

    event
}

/// Strips file paths and local-variable/context data from a stacktrace while
/// keeping the call shape (function / module / line).
fn scrub_stacktrace(stacktrace: &mut sentry::protocol::Stacktrace) {
    for frame in stacktrace.frames.iter_mut() {
        frame.abs_path = None;
        frame.filename = frame.filename.take().map(|f| scrub_text(&f));
        frame.context_line = None;
        frame.pre_context.clear();
        frame.post_context.clear();
        frame.vars.clear();
    }
}

// --- Consent (early, fail-closed) --------------------------------------------

/// Reads telemetry consent + install id for the given app identifier. Fail-closed:
/// any error / missing / malformed value yields `(false, None)` so telemetry never
/// activates by accident.
pub fn read_consent(identifier: &str) -> (bool, Option<String>) {
    match settings_store_path(identifier) {
        Some(path) => read_consent_from_path(&path),
        None => (false, None),
    }
}

/// Mirrors tauri-plugin-store's default AppData base: `data_dir/<identifier>/<file>`.
fn settings_store_path(identifier: &str) -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join(identifier).join(SETTINGS_STORE_FILE))
}

/// Parses the flat top-level JSON store at `path` for the consent keys.
pub fn read_consent_from_path(path: &Path) -> (bool, Option<String>) {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return (false, None),
    };
    let value: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => return (false, None),
    };
    let enabled = value
        .get(KEY_TELEMETRY_ENABLED)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let install_id = value
        .get(KEY_TELEMETRY_INSTALL_ID)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    (enabled, install_id)
}

// --- Init --------------------------------------------------------------------

/// Initializes Sentry when (and only when) the user has opted in and a DSN was
/// compiled in. Returns the guard, which the caller MUST keep alive for the
/// program's lifetime; returns `None` (no client created) otherwise.
pub fn init(enabled: bool, install_id: Option<String>) -> Option<ClientInitGuard> {
    // Always sync the gate so `before_send` and callers agree, even when we do
    // not actually create a client.
    set_enabled(enabled);

    let dsn = SENTRY_DSN?;
    if !enabled {
        return None;
    }

    let scrub_install_id = install_id;
    let guard = sentry::init((
        dsn,
        sentry::ClientOptions {
            release: sentry::release_name!(),
            send_default_pii: false,
            traces_sample_rate: 0.0,
            max_breadcrumbs: 0,
            before_breadcrumb: Some(Arc::new(|_breadcrumb| None)),
            before_send: Some(Arc::new(move |event| {
                if !is_enabled() {
                    return None;
                }
                Some(scrub_event(event, scrub_install_id.as_deref()))
            })),
            ..Default::default()
        },
    ));
    Some(guard)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentry::protocol::{Event, Exception, Frame, Stacktrace};

    #[test]
    fn scrub_text_redacts_sensitive_runs() {
        let input = "open /Users/alice/Documents/secret.txt from https://api.example.com/x key sk_live_ABCDEFGHIJKLMNOPQRSTUVWX mail a@b.com";
        let out = scrub_text(input);
        assert!(!out.contains("/Users/alice"), "path leaked: {out}");
        assert!(!out.contains("api.example.com"), "url leaked: {out}");
        assert!(
            !out.contains("sk_live_ABCDEFGHIJKLMNOPQRSTUVWX"),
            "token leaked: {out}"
        );
        assert!(!out.contains("a@b.com"), "email leaked: {out}");
        assert!(out.contains("[path]") && out.contains("[url]") && out.contains("[email]"));
    }

    #[test]
    fn scrub_event_keeps_shape_drops_pii() {
        let mut event = Event {
            server_name: Some("alices-macbook".into()),
            ..Default::default()
        };
        event
            .extra
            .insert("transcript".into(), "my secret words".into());

        let frame = Frame {
            function: Some("do_work".into()),
            filename: Some("/Users/alice/app/src/main.rs".into()),
            abs_path: Some("/Users/alice/app/src/main.rs".into()),
            lineno: Some(42),
            ..Default::default()
        };
        let exception = Exception {
            ty: "PanicException".into(),
            value: Some("failed reading /Users/alice/secret.txt".into()),
            stacktrace: Some(Stacktrace {
                frames: vec![frame],
                ..Default::default()
            }),
            ..Default::default()
        };
        event.exception.values.push(exception);

        let scrubbed = scrub_event(event, Some("install-123"));

        // PII gone.
        assert!(scrubbed.server_name.is_none());
        assert!(scrubbed.extra.is_empty());
        let ex = &scrubbed.exception.values[0];
        assert_eq!(ex.ty, "PanicException", "error type must be kept");
        assert!(!ex.value.as_deref().unwrap().contains("/Users/alice"));
        let frame = &ex.stacktrace.as_ref().unwrap().frames[0];
        assert!(frame.abs_path.is_none());
        assert!(!frame.filename.as_deref().unwrap().contains("/Users/alice"));
        // Shape kept.
        assert_eq!(frame.function.as_deref(), Some("do_work"));
        assert_eq!(frame.lineno, Some(42));
        // Allowlisted tags re-attached.
        assert_eq!(
            scrubbed.tags.get("os").map(|s| s.as_str()),
            Some(std::env::consts::OS)
        );
        assert_eq!(
            scrubbed.tags.get("install_id").map(|s| s.as_str()),
            Some("install-123")
        );
    }

    #[test]
    fn read_consent_is_fail_closed() {
        let dir = tempfile::tempdir().unwrap();

        // Missing file.
        let missing = dir.path().join("settings");
        assert_eq!(read_consent_from_path(&missing), (false, None));

        // Malformed JSON.
        let bad = dir.path().join("bad");
        std::fs::write(&bad, b"not json").unwrap();
        assert_eq!(read_consent_from_path(&bad), (false, None));

        // Valid opt-in with install id.
        let good = dir.path().join("good");
        std::fs::write(
            &good,
            br#"{"telemetry_enabled": true, "telemetry_install_id": "abc", "hotkey": "Cmd+Space"}"#,
        )
        .unwrap();
        assert_eq!(
            read_consent_from_path(&good),
            (true, Some("abc".to_string()))
        );

        // Explicit opt-out.
        let off = dir.path().join("off");
        std::fs::write(&off, br#"{"telemetry_enabled": false}"#).unwrap();
        assert_eq!(read_consent_from_path(&off), (false, None));
    }
}
