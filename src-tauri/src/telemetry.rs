//! Opt-in, anonymous error reporting (Sentry SDK -> self-hosted Bugsink).
//!
//! Privacy posture (non-negotiable):
//! - Disabled by default. Only active when the user has opted in AND it is a
//!   release build: the DSN is compiled in for release only, so dev/debug builds
//!   have no DSN and the client is never created (fully inert).
//! - No native minidumps: we use the `sentry` crate directly — no
//!   `tauri-plugin-sentry`, no browser-SDK injection, and no envelope/breadcrumb
//!   IPC — so the only capture path is the Rust SDK and `before_send` is the
//!   single egress chokepoint.
//! - No breadcrumbs, no PII, `traces_sample_rate = 0`.
//! - `before_send` REBUILDS every event from a tiny allowlist (allowlist by
//!   construction) and scrubs free text, so transcripts, audio, file paths, URLs,
//!   IPs, emails, keys, and target app/window names never leave the device.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};

use regex::Regex;
use sentry::protocol::{Event, Exception, Frame, Level, Stacktrace, Values};
use sentry::ClientInitGuard;

/// Bugsink DSN. Compiled into RELEASE builds only; dev/debug builds have no DSN
/// and are fully inert (no client is ever created). A DSN is a client ingestion
/// key — it can only send events, never read — so embedding it is expected/safe.
#[cfg(debug_assertions)]
const SENTRY_DSN: Option<&str> = None;
#[cfg(not(debug_assertions))]
const SENTRY_DSN: Option<&str> =
    Some("https://2d96c3759e8742309e92a0eb9c9659b4@bugsink.ideaplexa.com/1");

/// Store file (tauri-plugin-store) + keys that hold consent state. The store is
/// a flat top-level JSON object, so a raw reader can parse these keys before the
/// Tauri app (and its plugins) are built.
const SETTINGS_STORE_FILE: &str = "settings";
pub const KEY_TELEMETRY_ENABLED: &str = "telemetry_enabled";
pub const KEY_TELEMETRY_INSTALL_ID: &str = "telemetry_install_id";

/// In-process consent gate, read on every `before_send` and before every manual
/// capture. Revoking consent stops egress immediately within the session; a full
/// re-enable still needs a restart because the client is only wired at startup.
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
    // Windows drive paths, UNC shares, drive-less user/system dirs, Unix home/
    // system roots, and JS `file://`/`app://`/`asset://` URIs.
    Regex::new(
        r#"(?i)([a-z]:\\[^\s"'`]*|\\\\[^\s"'`]+|\\(?:users|windows|programdata)\\[^\s"'`]*|/(?:users|home|var|private|tmp|library|applications)/[^\s"'`]*|(?:file|app|asset)://[^\s"'`]*)"#,
    )
    .expect("valid path regex")
});
static RE_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)https?://[^\s"'`]+"#).expect("valid url regex"));
static RE_IP: LazyLock<Regex> = LazyLock::new(|| {
    // Bare IPv4, with optional :port (covers LAN endpoints like 192.168.1.20:8080).
    Regex::new(r#"\b(?:\d{1,3}\.){3}\d{1,3}(?::\d+)?\b"#).expect("valid ip regex")
});
static RE_EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}"#).expect("valid email regex")
});
static RE_LONG_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    // Long opaque runs: API keys, bearer tokens, hashes, foreign UUIDs.
    Regex::new(r#"\b[A-Za-z0-9_\-]{24,}\b"#).expect("valid token regex")
});

/// Redacts free-form text that may carry user content or environment detail.
pub fn scrub_text(input: &str) -> String {
    // Order matters: paths and URLs first so their inner hosts/IPs are consumed.
    let mut s = RE_PATH.replace_all(input, "[path]").into_owned();
    s = RE_URL.replace_all(&s, "[url]").into_owned();
    s = RE_IP.replace_all(&s, "[ip]").into_owned();
    s = RE_EMAIL.replace_all(&s, "[email]").into_owned();
    s = RE_LONG_TOKEN.replace_all(&s, "[redacted]").into_owned();
    s
}

/// Rebuilds an event from scratch — allowlist by construction. Only known-safe,
/// non-identifying fields are carried over; everything else (contexts, extra,
/// user, request, server_name, modules, fingerprint, culprit, transaction,
/// logger, sdk, debug_meta, breadcrumbs, threads, ...) is dropped because it is
/// never copied into the fresh event.
pub fn scrub_event(event: Event<'static>, install_id: Option<&str>) -> Event<'static> {
    let mut clean = Event {
        event_id: event.event_id,
        level: event.level,
        timestamp: event.timestamp,
        platform: event.platform,
        // Our own release string ("voicetypr@<version>") — not identifying.
        release: event.release,
        message: event.message.map(|m| scrub_text(&m)),
        exception: Values {
            values: event
                .exception
                .values
                .into_iter()
                .map(scrub_exception)
                .collect(),
        },
        ..Default::default()
    };

    // Re-attach a tiny, non-identifying allowlist as tags.
    clean.tags.insert("os".into(), std::env::consts::OS.into());
    clean
        .tags
        .insert("arch".into(), std::env::consts::ARCH.into());
    clean
        .tags
        .insert("app_version".into(), env!("CARGO_PKG_VERSION").into());
    if let Some(id) = install_id {
        clean.tags.insert("install_id".into(), id.into());
    }

    clean
}

/// Keeps the exception type + scrubbed message + sanitized stack; drops module,
/// mechanism, raw stacktrace, thread id (all potentially path/host-bearing).
fn scrub_exception(exception: Exception) -> Exception {
    Exception {
        ty: exception.ty,
        value: exception.value.map(|v| scrub_text(&v)),
        stacktrace: exception.stacktrace.map(scrub_stacktrace),
        ..Default::default()
    }
}

/// Keeps only the frame call shape; drops registers and frame-omitted markers.
fn scrub_stacktrace(stacktrace: Stacktrace) -> Stacktrace {
    Stacktrace {
        frames: stacktrace.frames.into_iter().map(scrub_frame).collect(),
        ..Default::default()
    }
}

/// Keeps function / line / column / in-app only. Drops filename, abs_path,
/// module, package, symbol, all addresses, context lines, and local variables.
fn scrub_frame(frame: Frame) -> Frame {
    Frame {
        function: frame.function,
        lineno: frame.lineno,
        colno: frame.colno,
        in_app: frame.in_app,
        ..Default::default()
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

// --- Init + capture ----------------------------------------------------------

/// Initializes Sentry when (and only when) the user has opted in and a DSN was
/// compiled in. Returns the guard, which the caller MUST keep alive for the
/// program's lifetime; returns `None` (no client created) otherwise. We do NOT
/// register `tauri-plugin-sentry` (no JS injection / no envelope IPC) — JS errors
/// are captured explicitly via `capture_frontend_error`, so `before_send` is the
/// single egress chokepoint.
pub fn init(enabled: bool, install_id: Option<String>) -> Option<ClientInitGuard> {
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

/// Captures a frontend-reported error as a Sentry event. Gated on consent and
/// routed through `capture_event` so `before_send` scrubs it. No-op when
/// telemetry is disabled or no client was initialized.
pub fn capture_frontend_error(name: Option<&str>, message: &str) {
    if !is_enabled() {
        return;
    }
    let event = Event {
        level: Level::Error,
        exception: Values {
            values: vec![Exception {
                ty: name.unwrap_or("FrontendError").to_string(),
                value: Some(message.to_string()),
                ..Default::default()
            }],
        },
        ..Default::default()
    };
    sentry::capture_event(event);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_text_redacts_sensitive_runs() {
        let input = r"open C:\Users\alice\secret.txt and \\fileserver\share\x from https://api.example.com/p at 10.0.0.5:9000 key sk_live_ABCDEFGHIJKLMNOPQRSTUVWX mail a@b.com";
        let out = scrub_text(input);
        assert!(!out.contains("alice"), "win path leaked: {out}");
        assert!(!out.contains("fileserver"), "unc path leaked: {out}");
        assert!(!out.contains("api.example.com"), "url leaked: {out}");
        assert!(!out.contains("10.0.0.5"), "ip leaked: {out}");
        assert!(
            !out.contains("sk_live_ABCDEFGHIJKLMNOPQRSTUVWX"),
            "token leaked: {out}"
        );
        assert!(!out.contains("a@b.com"), "email leaked: {out}");
    }

    #[test]
    fn scrub_event_rebuilds_from_allowlist() {
        let frame = Frame {
            function: Some("do_work".into()),
            filename: Some("/Users/alice/app/src/main.rs".into()),
            abs_path: Some("/Users/alice/app/src/main.rs".into()),
            module: Some("app::secret".into()),
            lineno: Some(42),
            ..Default::default()
        };
        let exception = Exception {
            ty: "PanicException".into(),
            value: Some("failed reading /Users/alice/secret.txt at 192.168.1.20:8080".into()),
            module: Some("app::io".into()),
            stacktrace: Some(Stacktrace {
                frames: vec![frame],
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut event = Event {
            server_name: Some("alices-macbook".into()),
            transaction: Some("/Users/alice/route".into()),
            exception: Values {
                values: vec![exception],
            },
            ..Default::default()
        };
        event
            .extra
            .insert("transcript".into(), "my secret words".into());
        event.tags.insert("device".into(), "alices-macbook".into());

        let scrubbed = scrub_event(event, Some("install-123"));

        // Whole PII-bearing sections gone (dropped by reconstruction).
        assert!(scrubbed.server_name.is_none());
        assert!(scrubbed.transaction.is_none());
        assert!(scrubbed.extra.is_empty());
        let ex = &scrubbed.exception.values[0];
        assert_eq!(ex.ty, "PanicException", "error type must be kept");
        assert!(ex.module.is_none(), "exception.module must be dropped");
        let value = ex.value.as_deref().unwrap();
        assert!(!value.contains("/Users/alice"), "path leaked: {value}");
        assert!(!value.contains("192.168.1.20"), "ip leaked: {value}");
        let frame = &ex.stacktrace.as_ref().unwrap().frames[0];
        assert_eq!(frame.function.as_deref(), Some("do_work"), "shape kept");
        assert_eq!(frame.lineno, Some(42));
        assert!(frame.filename.is_none(), "filename dropped");
        assert!(frame.abs_path.is_none(), "abs_path dropped");
        assert!(frame.module.is_none(), "frame.module dropped");
        // Only the allowlisted tags survive (the injected "device" is gone).
        assert!(!scrubbed.tags.contains_key("device"));
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

        let missing = dir.path().join("settings");
        assert_eq!(read_consent_from_path(&missing), (false, None));

        let bad = dir.path().join("bad");
        std::fs::write(&bad, b"not json").unwrap();
        assert_eq!(read_consent_from_path(&bad), (false, None));

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

        let off = dir.path().join("off");
        std::fs::write(&off, br#"{"telemetry_enabled": false}"#).unwrap();
        assert_eq!(read_consent_from_path(&off), (false, None));
    }
}
