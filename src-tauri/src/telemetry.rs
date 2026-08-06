//! Opt-out, anonymous error reporting and curated operational telemetry
//! (Sentry SDK -> self-hosted GlitchTip 6.2).
//!
//! Privacy posture (non-negotiable):
//! - On by default (opt-out). Active unless the user has explicitly opted out,
//!   AND only in release builds: the DSN is compiled in for release only, so
//!   dev/debug builds have no DSN and the client is never created (fully inert).
//! - No native minidumps: we use the `sentry` crate directly — no
//!   `tauri-plugin-sentry`, no browser-SDK injection, and no envelope/breadcrumb
//!   IPC — so the only capture paths are the Rust SDK and four independent
//!   egress gates, each checking consent:
//!   1. **Events**: `before_send` → [`scrub_event`] — rebuilds from an
//!      allowlist, scrubs secrets, preserves sanitized debug metadata.
//!   2. **Logs**: `before_send_log` → [`scrub_log`] — rebuilds from a strict
//!      attribute allowlist, injects only safe fixed metadata.
//!   3. **Transactions**: [`TelemetryTransaction::finish`] checks [`is_enabled`]
//!      before sending; sampled at 1%.
//!   4. **Transport**: [`ConsentTransport`] rechecks consent when each envelope
//!      is handed to HTTP, dropping structured logs buffered before opt-out.
//! - No breadcrumbs, no PII, no `release-health`/session tracking (the feature
//!   is not compiled in, so sessions are impossible), no `contexts` integration,
//!   `traces_sample_rate = 0.01`.
//! - [`scrub_event`] REBUILDS every event from a tiny allowlist (allowlist by
//!   construction) and scrubs structured secret runs (file paths, URLs, IPs,
//!   emails, keys, target app/window names), so those never leave the device.
//!   Raw audio is never captured. A frontend-reported error keeps its type and
//!   (length-capped) message for debuggability: the regex scrub strips
//!   structured secrets from the message but not arbitrary prose, so an opted-in
//!   error report can contain free-form frontend error text.
//! - Native symbolication: `debug-images` is enabled so `DebugImagesIntegration`
//!   attaches debug metadata. [`scrub_debug_meta`] reduces every image/debug
//!   filename to a basename (dropping directory components) while preserving the
//!   debug IDs, code IDs, image sizes, and addresses needed for server-side
//!   symbolication. [`scrub_frame`] retains `instruction_addr`, `image_addr`,
//!   and `symbol_addr` alongside function/line/column/in-app.
//! - Curated structured logs (`logs` feature): the `logs` feature enables the
//!   Sentry structured-log *protocol* (the `Log` envelope item and `capture_log`
//!   API) but NOT `sentry-log` (which would forward the `log` crate). The only
//!   log producer is [`log_transcription`], which accepts a closed enum, an
//!   optional integer duration, and nothing else — no free-form strings.
//!   [`scrub_log`] rebuilds each log from a strict allowlist before egress,
//!   preserving `trace_id` for safe correlation with sampled transactions.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, SystemTime};

use regex::Regex;
use sentry::protocol::{
    DebugImage, DebugMeta, Event, Exception, Frame, Level, Log, LogAttribute, LogLevel, Map,
    Stacktrace, Values,
};
use sentry::ClientInitGuard;

/// GlitchTip DSN. Compiled into RELEASE builds only; dev/debug builds have no
/// DSN and are fully inert (no client is ever created). A DSN is a client
/// ingestion key — it can only send events, never read — so embedding it is
/// expected/safe. The server is the self-hosted GlitchTip 6.2 instance at
/// `glitchtip.ideaplexa.com`, org `ideaplexa`, project `voicetypr-desktop`.
#[cfg(debug_assertions)]
const SENTRY_DSN: Option<&str> = None;
#[cfg(not(debug_assertions))]
const SENTRY_DSN: Option<&str> =
    Some("https://dc30154073564c529440b97bf18f1fdc@glitchtip.ideaplexa.com/1");

/// Environment tag attached to every event/transaction. Always `production` for
/// release builds (debug builds never send — no DSN).
const ENVIRONMENT: &str = "production";

/// Returns true when the semver version string contains a prerelease suffix
/// (e.g. `"2.0.4-beta"` → true, `"2.0.4"` → false). Const-evaluated so the
/// result is computed at compile time.
const fn is_prerelease(version: &str) -> bool {
    let bytes = version.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'-' {
            return true;
        }
        i += 1;
    }
    false
}

/// Release channel tag: derived at compile time from the crate version — `beta`
/// when the version is a prerelease (contains `-`), `stable` otherwise.
pub const RELEASE_CHANNEL: &str = if is_prerelease(env!("CARGO_PKG_VERSION")) {
    "beta"
} else {
    "stable"
};

/// Trace sample rate: 1% of transactions are sampled and sent to GlitchTip.
const TRACES_SAMPLE_RATE: f32 = 0.01;

/// Store file (tauri-plugin-store) + keys that hold consent state. The store is
/// a flat top-level JSON object, so a raw reader can parse these keys before the
/// Tauri app (and its plugins) are built.
const SETTINGS_STORE_FILE: &str = "settings";
pub const KEY_TELEMETRY_ENABLED: &str = "telemetry_enabled";
pub const KEY_TELEMETRY_INSTALL_ID: &str = "telemetry_install_id";
/// Default consent when no explicit choice is stored (fresh installs, upgraders
/// who completed onboarding before diagnostics existed). Opt-out: reporting is
/// on unless the user explicitly disabled it. An explicit `telemetry_enabled:
/// false` always wins.
pub const TELEMETRY_DEFAULT_ENABLED: bool = true;

/// In-process consent gate, read on every `before_send` and before every manual
/// capture. Revoking consent stops egress immediately within the session; a full
/// re-enable still needs a restart because the client is only wired at startup.
/// The `false` initializer only covers the tiny pre-init window; `init()`
/// overwrites it from stored consent at startup.
static TELEMETRY_ENABLED: AtomicBool = AtomicBool::new(false);

/// True when this build is capable of reporting at all (a DSN was compiled in).
pub fn is_available() -> bool {
    SENTRY_DSN.is_some()
}

/// Whether reporting is currently allowed this session.
pub fn is_enabled() -> bool {
    TELEMETRY_ENABLED.load(Ordering::SeqCst)
}

/// Flip the in-process gate. Disabling takes effect immediately.
pub fn set_enabled(enabled: bool) {
    TELEMETRY_ENABLED.store(enabled, Ordering::SeqCst);
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
/// logger, sdk, breadcrumbs, threads, ...) is dropped because it is never copied
/// into the fresh event. The event `release`, `environment`, and scrubbed
/// `debug_meta` are preserved (needed for symbolication); `debug_meta` is
/// sanitized by [`scrub_debug_meta`] to reduce every image filename to a
/// basename while keeping IDs/sizes/addresses.
pub fn scrub_event(event: Event<'static>, install_id: Option<&str>) -> Event<'static> {
    let scrubbed_meta = scrub_debug_meta(event.debug_meta.into_owned());
    let mut clean = Event {
        event_id: event.event_id,
        level: event.level,
        timestamp: event.timestamp,
        platform: event.platform,
        // Our own release string ("voicetypr@<version>") — not identifying.
        release: event.release,
        // "production" for release builds; never identifying.
        environment: event.environment,
        message: event.message.map(|m| scrub_text(&m)),
        exception: Values {
            values: event
                .exception
                .values
                .into_iter()
                .map(scrub_exception)
                .collect(),
        },
        // Sanitized native debug metadata — basenames only, IDs/sizes/addresses
        // preserved for server-side symbolication.
        debug_meta: Cow::Owned(scrubbed_meta),
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
    clean
        .tags
        .insert("release_channel".into(), RELEASE_CHANNEL.into());
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

/// Keeps function / line / column / in-app, AND the native addresses needed for
/// server-side symbolication (`instruction_addr`, `image_addr`, `symbol_addr`,
/// `addr_mode`). Drops filename, abs_path, module, package, symbol, registers,
/// context lines, and local variables — all potentially path/host-bearing.
fn scrub_frame(frame: Frame) -> Frame {
    Frame {
        function: frame.function,
        lineno: frame.lineno,
        colno: frame.colno,
        in_app: frame.in_app,
        // Native addresses required for server-side symbolication. These are
        // memory addresses, not user data.
        image_addr: frame.image_addr,
        instruction_addr: frame.instruction_addr,
        symbol_addr: frame.symbol_addr,
        addr_mode: frame.addr_mode,
        ..Default::default()
    }
}

// --- Native debug-metadata scrubbing -----------------------------------------

/// Reduces any path-bearing string to its file-name component (the last path
/// segment after either `/` or `\`). Splits on both separators independently of
/// the host OS so Windows paths are correctly reduced on any platform.
fn basename(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

/// Sanitizes [`DebugMeta`]: keeps `sdk_info` and the list of images, but every
/// image filename/debug-file/code-file is reduced to a basename while the debug
/// IDs, code IDs, image sizes, and addresses (needed for server-side
/// symbolication) are preserved verbatim.
fn scrub_debug_meta(meta: DebugMeta) -> DebugMeta {
    DebugMeta {
        sdk_info: meta.sdk_info,
        images: meta.images.into_iter().map(scrub_debug_image).collect(),
    }
}

/// Reduces every path-bearing string on a [`DebugImage`] to its basename,
/// preserving all identifiers, sizes, and addresses for symbolication.
fn scrub_debug_image(image: DebugImage) -> DebugImage {
    match image {
        DebugImage::Apple(img) => DebugImage::Apple(sentry::protocol::AppleDebugImage {
            name: basename(&img.name),
            ..img
        }),
        DebugImage::Symbolic(img) => DebugImage::Symbolic(sentry::protocol::SymbolicDebugImage {
            name: basename(&img.name),
            debug_file: img.debug_file.map(|f| basename(&f)),
            ..img
        }),
        DebugImage::Wasm(img) => DebugImage::Wasm(sentry::protocol::WasmDebugImage {
            name: basename(&img.name),
            debug_file: img.debug_file.map(|f| basename(&f)),
            code_file: basename(&img.code_file),
            ..img
        }),
        // Proguard images carry only a UUID — no path-bearing fields.
        other => other,
    }
}

// --- Consent (early, opt-out default) ----------------------------------------

/// Reads telemetry consent + install id for the given app identifier. Opt-out
/// default: any missing / malformed / unreadable value yields
/// `(TELEMETRY_DEFAULT_ENABLED, None)`, so telemetry is on unless the user has
/// explicitly opted out. An explicit `telemetry_enabled: false` is always honored.
pub fn read_consent(identifier: &str) -> (bool, Option<String>) {
    match settings_store_path(identifier) {
        Some(path) => read_consent_from_path(&path),
        None => (TELEMETRY_DEFAULT_ENABLED, None),
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
        Err(_) => return (TELEMETRY_DEFAULT_ENABLED, None),
    };
    let value: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => return (TELEMETRY_DEFAULT_ENABLED, None),
    };
    let enabled = value
        .get(KEY_TELEMETRY_ENABLED)
        .and_then(|v| v.as_bool())
        .unwrap_or(TELEMETRY_DEFAULT_ENABLED);
    let install_id = value
        .get(KEY_TELEMETRY_INSTALL_ID)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    (enabled, install_id)
}

// --- Init + capture ----------------------------------------------------------

/// Transport-level consent gate. Structured logs are batched by the SDK for up
/// to five seconds, after `before_send_log` has run; checking again here drops
/// that buffered envelope if diagnostics were disabled before the actual send.
/// Events and transactions also pass through this final egress chokepoint.
struct ConsentTransport {
    inner: Arc<dyn sentry::Transport>,
}

impl sentry::Transport for ConsentTransport {
    fn send_envelope(&self, envelope: sentry::Envelope) {
        if is_enabled() {
            self.inner.send_envelope(envelope);
        }
    }

    fn flush(&self, timeout: Duration) -> bool {
        self.inner.flush(timeout)
    }

    fn shutdown(&self, timeout: Duration) -> bool {
        self.inner.shutdown(timeout)
    }
}

#[derive(Clone)]
struct ConsentTransportFactory;

impl sentry::TransportFactory for ConsentTransportFactory {
    fn create_transport(&self, options: &sentry::ClientOptions) -> Arc<dyn sentry::Transport> {
        let inner = sentry::TransportFactory::create_transport(
            &sentry::transports::DefaultTransportFactory,
            options,
        );
        Arc::new(ConsentTransport { inner })
    }
}

/// Initializes Sentry (→ GlitchTip) when reporting is enabled (on by default
/// unless the user opted out) and a DSN was compiled in. Returns the guard,
/// which the caller MUST keep alive for the program's lifetime; returns `None`
/// (no client created) otherwise. We do NOT register `tauri-plugin-sentry` (no
/// JS injection / no envelope IPC) — JS errors are captured explicitly via
/// [`capture_frontend_error`].
///
/// **Four egress gates**: (1) `before_send` → [`scrub_event`] for events,
/// (2) `before_send_log` → [`scrub_log`] for structured logs, (3) consent
/// check inside [`TelemetryTransaction::finish`] for sampled transactions, and
/// (4) [`ConsentTransport`] at actual envelope send time. Together they make
/// revocation effective even for logs buffered by the SDK before opt-out.
///
/// Configuration: `environment = "production"`, `traces_sample_rate = 0.01`,
/// `release-health`/session tracking is NOT compiled in (sessions are
/// impossible), structured logs are enabled.
pub fn init(enabled: bool, install_id: Option<String>) -> Option<ClientInitGuard> {
    set_enabled(enabled);

    let dsn = SENTRY_DSN?;
    if !enabled {
        return None;
    }

    let log_install_id = install_id.clone();
    let event_install_id = install_id;
    let guard = sentry::init((
        dsn,
        sentry::ClientOptions {
            release: sentry::release_name!(),
            environment: Some(Cow::Borrowed(ENVIRONMENT)),
            send_default_pii: false,
            traces_sample_rate: TRACES_SAMPLE_RATE,
            max_breadcrumbs: 0,
            before_breadcrumb: Some(Arc::new(|_breadcrumb| None)),
            // Gate 1 — events: rebuild from allowlist + scrub secrets.
            before_send: Some(Arc::new(move |event| {
                if !is_enabled() {
                    return None;
                }
                Some(scrub_event(event, event_install_id.as_deref()))
            })),
            // Gate 2 — structured logs: rebuild from strict allowlist +
            // inject safe metadata. The only producer is `log_transcription`
            // (closed API); revoking consent drops queued logs.
            enable_logs: true,
            before_send_log: Some(Arc::new(move |log| {
                if !is_enabled() {
                    return None;
                }
                scrub_log(log, log_install_id.as_deref())
            })),
            // Final egress gate, evaluated when an envelope is handed to the
            // HTTP transport (after structured-log batching).
            transport: Some(Arc::new(ConsentTransportFactory)),
            ..Default::default()
        },
    ));
    Some(guard)
}

/// Captures a frontend-reported error as a Sentry event. Gated on consent and
/// routed through `capture_event` so `before_send` scrubs it. No-op when
/// telemetry is disabled or no client was initialized.
///
/// Privacy: the frontend `message` is untrusted free-form text, so `before_send`
/// (`scrub_exception` -> `scrub_text`) redacts *structured* secret runs
/// (paths/URLs/IPs/emails/tokens) from the value before it leaves the process,
/// and the message is length-capped here to bound payload size. Telemetry is
/// on by default (opt-out).
pub fn capture_frontend_error(name: Option<&str>, message: &str) {
    if !is_enabled() {
        return;
    }
    let event = build_frontend_error_event(name, message);
    sentry::capture_event(event);
}

/// Max characters of a frontend error message forwarded to telemetry.
const FRONTEND_ERROR_MAX_LEN: usize = 2000;

/// Constructs the event for a frontend-reported error: the stable error
/// type/category plus the length-capped message. Pure (no Sentry client needed)
/// so it is unit-testable; structured-secret redaction runs in `before_send`.
fn build_frontend_error_event(name: Option<&str>, message: &str) -> Event<'static> {
    let value: String = message.chars().take(FRONTEND_ERROR_MAX_LEN).collect();
    Event {
        level: Level::Error,
        exception: Values {
            values: vec![Exception {
                ty: name.unwrap_or("FrontendError").to_string(),
                value: Some(value),
                ..Default::default()
            }],
        },
        ..Default::default()
    }
}

// --- Operational telemetry (closed API) -------------------------------------

/// Fixed transcription lifecycle phases for operational telemetry. The only
/// accepted "name" type — no free-form strings reach the log payload. Phases
/// follow the actual transcription flow: recording → decode → formatting →
/// delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptionPhase {
    // --- Recording ---
    /// Recording started.
    RecordingStarted,
    /// Recording stopped after capture completed.
    RecordingStopped,
    /// Recording cancelled before transcription.
    RecordingCancelled,
    // --- Decode ---
    /// Decode started.
    DecodeStarted,
    /// Decode succeeded.
    DecodeSucceeded,
    /// Decode failed.
    DecodeFailed,
    /// Decode cancelled.
    DecodeCancelled,
    // --- Formatting ---
    /// Formatting (post-processing) succeeded.
    FormattingSucceeded,
    /// Formatting failed.
    FormattingFailed,
    // --- Delivery ---
    /// Delivery (pasting to target app / clipboard) succeeded.
    DeliverySucceeded,
    /// Delivery failed.
    DeliveryFailed,
}

impl TranscriptionPhase {
    /// Fixed Sentry structured-log body for this phase. These are the ONLY
    /// bodies that ever leave the device.
    pub const fn log_body(&self) -> &'static str {
        match self {
            Self::RecordingStarted => "transcription.recording.started",
            Self::RecordingStopped => "transcription.recording.stopped",
            Self::RecordingCancelled => "transcription.recording.cancelled",
            Self::DecodeStarted => "transcription.decode.started",
            Self::DecodeSucceeded => "transcription.decode.succeeded",
            Self::DecodeFailed => "transcription.decode.failed",
            Self::DecodeCancelled => "transcription.decode.cancelled",
            Self::FormattingSucceeded => "transcription.formatting.succeeded",
            Self::FormattingFailed => "transcription.formatting.failed",
            Self::DeliverySucceeded => "transcription.delivery.succeeded",
            Self::DeliveryFailed => "transcription.delivery.failed",
        }
    }

    /// Fixed log level for this phase.
    pub const fn log_level(&self) -> LogLevel {
        match self {
            Self::DecodeFailed | Self::FormattingFailed | Self::DeliveryFailed => LogLevel::Error,
            _ => LogLevel::Info,
        }
    }

    /// Fixed attribute value (static name) identifying the phase.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::RecordingStarted => "recording.started",
            Self::RecordingStopped => "recording.stopped",
            Self::RecordingCancelled => "recording.cancelled",
            Self::DecodeStarted => "decode.started",
            Self::DecodeSucceeded => "decode.succeeded",
            Self::DecodeFailed => "decode.failed",
            Self::DecodeCancelled => "decode.cancelled",
            Self::FormattingSucceeded => "formatting.succeeded",
            Self::FormattingFailed => "formatting.failed",
            Self::DeliverySucceeded => "delivery.succeeded",
            Self::DeliveryFailed => "delivery.failed",
        }
    }
}

/// Fixed child-span operation names within a transcription transaction.
/// The callsite uses [`TelemetryTransaction::start_span`] with these — no
/// arbitrary `&str` reaches Sentry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptionSpan {
    /// Audio/model decode phase.
    Decode,
    /// Formatting / post-processing phase.
    Formatting,
    /// Text delivery phase.
    Delivery,
}

impl TranscriptionSpan {
    /// Fixed Sentry operation name for this span.
    pub const fn op(&self) -> &'static str {
        match self {
            Self::Decode => "decode",
            Self::Formatting => "formatting",
            Self::Delivery => "delivery",
        }
    }
}

/// Emits a curated, consent-gated transcription lifecycle log. Accepts only a
/// closed enum phase and an optional integer duration — no free-form strings.
pub fn log_transcription(phase: TranscriptionPhase, duration_ms: Option<u64>) {
    capture_transcription_log(phase, duration_ms, None);
}

/// Emits a curated lifecycle log correlated with a sampled transcription
/// transaction. The transaction wrapper is closed and carries no user data.
pub fn log_transcription_for_transaction(
    transaction: Option<&TelemetryTransaction>,
    phase: TranscriptionPhase,
    duration_ms: Option<u64>,
) {
    capture_transcription_log(phase, duration_ms, transaction);
}

fn capture_transcription_log(
    phase: TranscriptionPhase,
    duration_ms: Option<u64>,
    transaction: Option<&TelemetryTransaction>,
) {
    if !is_enabled() {
        return;
    }
    let log = build_transcription_log(phase, duration_ms);
    sentry::Hub::with_active(|hub| match transaction {
        Some(transaction) => hub.with_scope(
            |scope| scope.set_span(Some(transaction.inner.clone().into())),
            || hub.capture_log(log),
        ),
        None => hub.capture_log(log),
    });
}

/// Constructs a [`Log`] for a transcription lifecycle event. Pure (no client
/// needed) so it is unit-testable. The body is a fixed static string; the only
/// attributes are the fixed phase name and the integer duration — never
/// free-form text.
fn build_transcription_log(phase: TranscriptionPhase, duration_ms: Option<u64>) -> Log {
    let mut attributes = Map::new();
    attributes.insert("phase".into(), LogAttribute::from(phase.as_str()));
    if let Some(ms) = duration_ms {
        attributes.insert("duration_ms".into(), LogAttribute::from(ms));
    }
    Log {
        level: phase.log_level(),
        body: phase.log_body().to_string(),
        timestamp: SystemTime::now(),
        attributes,
        trace_id: None,
        severity_number: None,
    }
}

/// Returns the [`TranscriptionPhase`] whose `log_body()` exactly matches the
/// given body string, or `None` if no known phase matches.
fn valid_phase_for_body(body: &str) -> Option<TranscriptionPhase> {
    const ALL: [TranscriptionPhase; 11] = [
        TranscriptionPhase::RecordingStarted,
        TranscriptionPhase::RecordingStopped,
        TranscriptionPhase::RecordingCancelled,
        TranscriptionPhase::DecodeStarted,
        TranscriptionPhase::DecodeSucceeded,
        TranscriptionPhase::DecodeFailed,
        TranscriptionPhase::DecodeCancelled,
        TranscriptionPhase::FormattingSucceeded,
        TranscriptionPhase::FormattingFailed,
        TranscriptionPhase::DeliverySucceeded,
        TranscriptionPhase::DeliveryFailed,
    ];
    ALL.into_iter().find(|phase| phase.log_body() == body)
}

/// Rebuilds a [`Log`] from a strict allowlist, mirroring [`scrub_event`] for
/// events. Returns `None` (reject) unless the `body` is an exact match for a
/// known [`TranscriptionPhase::log_body`] AND the `phase` attribute matches
/// that phase's `as_str()`. This ensures only logs from the closed
/// [`build_transcription_log`] producer pass through — a direct
/// `sentry::capture_log` with arbitrary text is silently dropped.
///
/// On accept: only the fixed `phase` and `duration_ms` attributes survive from
/// the original; any scope-enriched attributes are dropped. Safe fixed metadata
/// (OS, arch, app version, release channel, install id) is injected. The
/// `trace_id` is preserved for safe correlation with sampled transactions.
fn scrub_log(log: Log, install_id: Option<&str>) -> Option<Log> {
    // Validate body is an exact known phase log_body.
    let phase = valid_phase_for_body(&log.body)?;

    // Validate phase attribute matches the body's expected phase. If missing
    // or mismatched, the log didn't come from our closed builder.
    let phase_attr = log.attributes.get("phase")?;
    if phase_attr.0.as_str() != Some(phase.as_str()) {
        return None;
    }

    let mut safe = Map::new();
    safe.insert("phase".into(), LogAttribute::from(phase.as_str()));
    if let Some(val) = log.attributes.get("duration_ms") {
        safe.insert("duration_ms".into(), val.clone());
    }
    safe.insert("os".into(), LogAttribute::from(std::env::consts::OS));
    safe.insert("arch".into(), LogAttribute::from(std::env::consts::ARCH));
    safe.insert(
        "app_version".into(),
        LogAttribute::from(env!("CARGO_PKG_VERSION")),
    );
    safe.insert(
        "release_channel".into(),
        LogAttribute::from(RELEASE_CHANNEL),
    );
    if let Some(id) = install_id {
        safe.insert("install_id".into(), LogAttribute::from(id));
    }
    Some(Log {
        level: log.level,
        body: log.body,
        trace_id: log.trace_id,
        timestamp: log.timestamp,
        severity_number: None,
        attributes: safe,
    })
}

// --- Trace primitives (sampled, consent-gated) -------------------------------

/// Consent-gated wrapper around a Sentry [`Transaction`]. [`finish`](Self::finish)
/// checks consent before sending — if the user has opted out mid-trace, the
/// transaction (and all child spans) is silently dropped, never reaching
/// GlitchTip. This is the transaction gate alongside the event/log callbacks
/// and the final [`ConsentTransport`] envelope gate.
pub struct TelemetryTransaction {
    inner: sentry::Transaction,
}

impl TelemetryTransaction {
    /// Starts a fixed-operation child span. The operation is chosen from the
    /// closed [`TranscriptionSpan`] enum — no arbitrary `&str` payload reaches
    /// Sentry. The span must be finished via [`TelemetrySpan::finish`] when the
    /// phase completes.
    #[must_use = "a span must be explicitly closed via finish()"]
    pub fn start_span(&self, span: TranscriptionSpan) -> TelemetrySpan {
        TelemetrySpan {
            inner: self.inner.start_child(span.op(), span.op()),
        }
    }

    /// Finishes the transaction. If consent has been revoked since the
    /// transaction was started, the entire transaction tree is dropped without
    /// sending.
    pub fn finish(self) {
        if !is_enabled() {
            return;
        }
        self.inner.finish();
    }
}

/// Consent-gated wrapper around a Sentry [`Span`]. [`finish`](Self::finish)
/// checks consent before recording — if the user has opted out, the span is
/// dropped without being attached to the parent transaction.
pub struct TelemetrySpan {
    inner: sentry::Span,
}

impl TelemetrySpan {
    /// Finishes the span, attaching it to the parent transaction. If consent
    /// has been revoked, the span is silently dropped.
    pub fn finish(self) {
        if !is_enabled() {
            return;
        }
        self.inner.finish();
    }
}

/// Starts a sampled transcription transaction. Returns `None` when consent is
/// not granted or the SDK client is absent (debug builds); returns `None` when
/// the transaction is not selected by the 1% sample rate. The returned
/// [`TelemetryTransaction`] is `Send + Sync` and safe to move into async tasks.
///
/// The caller creates fixed child spans via
/// [`TelemetryTransaction::start_span`] and finishes the transaction with
/// `.finish()` when the lifecycle ends.
pub fn start_transcription_transaction() -> Option<TelemetryTransaction> {
    if !is_enabled() {
        return None;
    }
    let ctx = sentry::TransactionContext::new("transcription", "transcribe");
    let tx = sentry::start_transaction(ctx);
    if !tx.is_sampled() {
        return None;
    }
    Some(TelemetryTransaction { inner: tx })
}

#[cfg(test)]
mod tests {
    use super::*;

    static CONSENT_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn scrub_text_redacts_sensitive_runs() {
        // Build the Stripe-style token at runtime so the literal never appears
        // verbatim in source (avoids tripping secret scanners on this fixture).
        let token = format!("sk_{}_{}", "live", "ABCDEFGHIJKLMNOPQRSTUVWX");
        let input = format!(
            r"open C:\Users\alice\secret.txt and \\fileserver\share\x from https://api.example.com/p at 10.0.0.5:9000 key {token} mail a@b.com"
        );
        let out = scrub_text(&input);
        assert!(!out.contains("alice"), "win path leaked: {out}");
        assert!(!out.contains("fileserver"), "unc path leaked: {out}");
        assert!(!out.contains("api.example.com"), "url leaked: {out}");
        assert!(!out.contains("10.0.0.5"), "ip leaked: {out}");
        assert!(!out.contains(&token), "token leaked: {out}");
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
    fn read_consent_defaults_on_unless_opted_out() {
        let dir = tempfile::tempdir().unwrap();

        let missing = dir.path().join("settings");
        assert_eq!(read_consent_from_path(&missing), (true, None));

        let bad = dir.path().join("bad");
        std::fs::write(&bad, b"not json").unwrap();
        assert_eq!(read_consent_from_path(&bad), (true, None));

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

        // Explicit opt-out must always be honored even under the opt-out default.
        let off = dir.path().join("off");
        std::fs::write(&off, br#"{"telemetry_enabled": false}"#).unwrap();
        assert_eq!(read_consent_from_path(&off), (false, None));
    }
    #[test]
    fn frontend_error_event_keeps_type_and_message() {
        // An error message is diagnostic and is kept; `before_send` scrubs
        // structured secrets from it, and it is length-capped here.
        let event = build_frontend_error_event(Some("TypeError"), "kaboom");
        let ex = &event.exception.values[0];
        assert_eq!(ex.ty, "TypeError", "stable error type must be kept");
        assert_eq!(ex.value.as_deref(), Some("kaboom"), "message must be kept");

        // Over-long messages are length-capped.
        let long = "x".repeat(FRONTEND_ERROR_MAX_LEN + 500);
        let capped = build_frontend_error_event(None, &long);
        assert_eq!(
            capped.exception.values[0].value.as_deref().map(str::len),
            Some(FRONTEND_ERROR_MAX_LEN),
            "message must be length-capped"
        );
    }

    #[test]
    fn frontend_error_event_defaults_type_when_name_absent() {
        let event = build_frontend_error_event(None, "boom");
        assert_eq!(event.exception.values[0].ty, "FrontendError");
        assert_eq!(event.level, Level::Error);
    }

    // --- Native debug-metadata scrubbing tests --------------------------------

    #[test]
    fn basename_strips_directories() {
        // Unix paths.
        assert_eq!(basename("/usr/local/lib/libfoo.dylib"), "libfoo.dylib");
        assert_eq!(basename("/Users/alice/app/src/main.rs"), "main.rs");
        // Windows paths — must work on any host OS.
        assert_eq!(basename("C:\\Users\\alice\\app.exe"), "app.exe");
        assert_eq!(basename("\\\\server\\share\\lib\\foo.dll"), "foo.dll");
        // No separators — returned unchanged.
        assert_eq!(basename("libfoo.dylib"), "libfoo.dylib");
        assert_eq!(basename(""), "");
    }

    #[test]
    fn scrub_debug_meta_reduces_paths_keeps_ids() {
        use sentry::protocol::debugid::DebugId;
        use sentry::protocol::{Addr, AppleDebugImage, SymbolicDebugImage};
        use sentry::types::Uuid;

        let debug_id: DebugId = "5d2c9413-2edb-4a9e-9e9a-5d2c94132edb".parse().unwrap();
        let images = vec![
            DebugImage::Symbolic(SymbolicDebugImage {
                name: "/usr/local/lib/libvoicetypr.dylib".into(),
                arch: Some("arm64".into()),
                image_addr: Addr(0x100000),
                image_size: 65536,
                image_vmaddr: Addr(0x0),
                id: debug_id,
                code_id: None,
                debug_file: Some(
                    "/build/voicetypr.dylib.dSYM/Contents/Resources/DWARF/voicetypr.dylib".into(),
                ),
            }),
            DebugImage::Apple(AppleDebugImage {
                name: "/Users/builder/app/Frameworks/MyFw.framework/MyFw".into(),
                arch: Some("arm64".into()),
                cpu_type: None,
                cpu_subtype: None,
                image_addr: Addr(0x200000),
                image_size: 32768,
                image_vmaddr: Addr(0x0),
                uuid: Uuid::nil(),
            }),
        ];

        let meta = DebugMeta {
            sdk_info: None,
            images,
        };
        let scrubbed = scrub_debug_meta(meta);
        assert_eq!(scrubbed.images.len(), 2);

        // --- Symbolic image ---
        match &scrubbed.images[0] {
            DebugImage::Symbolic(img) => {
                assert_eq!(img.name, "libvoicetypr.dylib", "name must be basename");
                assert!(!img.name.contains('/'), "no path in name");
                assert_eq!(
                    img.debug_file.as_deref(),
                    Some("voicetypr.dylib"),
                    "debug_file must be basename"
                );
                assert_eq!(img.image_addr, Addr(0x100000), "image_addr preserved");
                assert_eq!(img.image_size, 65536, "image_size preserved");
                assert_eq!(img.id, debug_id, "debug id preserved");
            }
            _ => panic!("expected Symbolic image"),
        }

        // --- Apple image ---
        match &scrubbed.images[1] {
            DebugImage::Apple(img) => {
                assert_eq!(img.name, "MyFw", "name must be basename");
                assert!(!img.name.contains('/'), "no path in name");
                assert_eq!(img.uuid, Uuid::nil(), "uuid preserved");
                assert_eq!(img.image_addr, Addr(0x200000), "image_addr preserved");
                assert_eq!(img.image_size, 32768, "image_size preserved");
            }
            _ => panic!("expected Apple image"),
        }
    }

    #[test]
    fn scrub_frame_preserves_native_addresses() {
        use sentry::protocol::Addr;

        let frame = Frame {
            function: Some("transcribe".into()),
            filename: Some("/Users/alice/src/lib.rs".into()),
            abs_path: Some("/Users/alice/src/lib.rs".into()),
            module: Some("voicetypr::transcribe".into()),
            package: Some("voicetypr".into()),
            symbol: Some("_ZN12voicetypr10transcribe17h1234".into()),
            lineno: Some(42),
            colno: Some(8),
            in_app: Some(true),
            image_addr: Some(Addr(0x100000)),
            instruction_addr: Some(Addr(0x1000a0)),
            symbol_addr: Some(Addr(0x100080)),
            addr_mode: Some("abs".into()),
            vars: {
                let mut m = Map::new();
                m.insert("secret".into(), "value".into());
                m
            },
            ..Default::default()
        };

        let scrubbed = scrub_frame(frame);

        // Addresses preserved (needed for server-side symbolication).
        assert_eq!(scrubbed.instruction_addr, Some(Addr(0x1000a0)));
        assert_eq!(scrubbed.image_addr, Some(Addr(0x100000)));
        assert_eq!(scrubbed.symbol_addr, Some(Addr(0x100080)));
        assert_eq!(scrubbed.addr_mode.as_deref(), Some("abs"));

        // Path-bearing / PII fields dropped.
        assert!(scrubbed.filename.is_none(), "filename dropped");
        assert!(scrubbed.abs_path.is_none(), "abs_path dropped");
        assert!(scrubbed.module.is_none(), "module dropped");
        assert!(scrubbed.package.is_none(), "package dropped");
        assert!(scrubbed.symbol.is_none(), "symbol dropped");
        assert!(scrubbed.vars.is_empty(), "vars dropped");

        // Shape preserved.
        assert_eq!(scrubbed.function.as_deref(), Some("transcribe"));
        assert_eq!(scrubbed.lineno, Some(42));
        assert_eq!(scrubbed.colno, Some(8));
        assert_eq!(scrubbed.in_app, Some(true));
    }

    #[test]
    fn scrub_event_preserves_environment_release_channel() {
        let event = Event {
            level: Level::Error,
            release: Some("voicetypr@2.0.4".into()),
            environment: Some("production".into()),
            message: Some("boom".into()),
            ..Default::default()
        };

        let scrubbed = scrub_event(event, None);

        // Environment and release survive scrubbing.
        assert_eq!(scrubbed.environment.as_deref(), Some("production"));
        assert_eq!(scrubbed.release.as_deref(), Some("voicetypr@2.0.4"));

        // Release channel tag is present.
        assert_eq!(
            scrubbed.tags.get("release_channel").map(|s| s.as_str()),
            Some(RELEASE_CHANNEL)
        );

        // Arbitrary sections remain absent.
        assert!(scrubbed.server_name.is_none());
        assert!(scrubbed.user.is_none());
        assert!(scrubbed.request.is_none());
        assert!(scrubbed.extra.is_empty());
        assert!(scrubbed.contexts.is_empty());
        assert!(scrubbed.sdk.is_none());
        assert!(scrubbed.transaction.is_none());
        assert!(scrubbed.culprit.is_none());
    }

    #[test]
    fn scrub_event_preserves_debug_meta_through_pipeline() {
        use sentry::protocol::debugid::DebugId;
        use sentry::protocol::{Addr, SymbolicDebugImage};

        // An event with debug_meta that the DebugImagesIntegration would have
        // attached. scrub_event must carry the sanitized debug_meta through.
        let mut event: Event<'static> = Event {
            level: Level::Error,
            ..Default::default()
        };
        event.debug_meta = Cow::Owned(DebugMeta {
            sdk_info: None,
            images: vec![DebugImage::Symbolic(SymbolicDebugImage {
                name: "/usr/local/lib/libsecret.dylib".into(),
                arch: None,
                image_addr: Addr(0x400000),
                image_size: 131072,
                image_vmaddr: Addr(0x0),
                id: DebugId::default(),
                code_id: None,
                debug_file: None,
            })],
        });

        let scrubbed = scrub_event(event, None);

        // debug_meta survived and was scrubbed.
        assert_eq!(scrubbed.debug_meta.images.len(), 1);
        match &scrubbed.debug_meta.images[0] {
            DebugImage::Symbolic(img) => {
                assert_eq!(img.name, "libsecret.dylib", "path reduced to basename");
                assert_eq!(img.image_addr, Addr(0x400000), "address preserved");
            }
            _ => panic!("expected Symbolic image"),
        }
    }

    // --- Release channel derivation test --------------------------------------

    #[test]
    fn release_channel_derived_from_version() {
        assert!(is_prerelease("2.0.4-beta"), "prerelease with dash");
        assert!(is_prerelease("3.0.0-rc.1"), "rc prerelease");
        assert!(!is_prerelease("2.0.4"), "stable release");
        assert!(!is_prerelease("2.1.0"), "stable release");
        // The actual crate version — currently "2.0.4" (no dash) → stable.
        // When bumped to e.g. "2.1.0-beta", the const switches to "beta".
        assert_eq!(
            RELEASE_CHANNEL,
            if is_prerelease(env!("CARGO_PKG_VERSION")) {
                "beta"
            } else {
                "stable"
            }
        );
    }

    // --- Operational log API tests -------------------------------------------

    #[test]
    fn transcription_log_uses_fixed_body_and_safe_attributes() {
        // Recording phases.
        let log = build_transcription_log(TranscriptionPhase::RecordingStarted, None);
        assert_eq!(log.body, "transcription.recording.started");
        assert_eq!(log.level, LogLevel::Info);
        assert_eq!(log.attributes.len(), 1, "only phase attribute");
        assert!(!log.attributes.contains_key("duration_ms"));

        // Decode failure with duration.
        let log = build_transcription_log(TranscriptionPhase::DecodeFailed, Some(1234));
        assert_eq!(log.body, "transcription.decode.failed");
        assert_eq!(log.level, LogLevel::Error);
        assert_eq!(log.attributes.len(), 2, "phase + duration_ms");
        assert!(log.attributes.contains_key("phase"));
        assert!(log.attributes.contains_key("duration_ms"));

        // Every phase produces a body with the fixed prefix.
        let all_phases = [
            TranscriptionPhase::RecordingStarted,
            TranscriptionPhase::RecordingStopped,
            TranscriptionPhase::RecordingCancelled,
            TranscriptionPhase::DecodeStarted,
            TranscriptionPhase::DecodeSucceeded,
            TranscriptionPhase::DecodeFailed,
            TranscriptionPhase::DecodeCancelled,
            TranscriptionPhase::FormattingSucceeded,
            TranscriptionPhase::FormattingFailed,
            TranscriptionPhase::DeliverySucceeded,
            TranscriptionPhase::DeliveryFailed,
        ];
        for phase in all_phases {
            let log = build_transcription_log(phase, Some(0));
            assert!(
                log.body.starts_with("transcription."),
                "body must be fixed prefix: {}",
                log.body
            );
            // Failure phases are Error level; all others Info.
            let expected_level = matches!(
                phase,
                TranscriptionPhase::DecodeFailed
                    | TranscriptionPhase::FormattingFailed
                    | TranscriptionPhase::DeliveryFailed
            );
            assert_eq!(
                log.level == LogLevel::Error,
                expected_level,
                "level mismatch for {:?}: {:?}",
                phase,
                log.level
            );
        }
    }

    #[test]
    fn scrub_log_rebuilds_from_strict_allowlist() {
        let mut log = build_transcription_log(TranscriptionPhase::DecodeStarted, Some(100));
        // Simulate scope-injected dangerous attributes.
        log.attributes.insert(
            "user_path".into(),
            LogAttribute::from("/Users/alice/secret"),
        );
        log.attributes
            .insert("transcript".into(), LogAttribute::from("hello world"));

        let scrubbed = scrub_log(log, Some("install-xyz")).expect("valid log must pass");

        // Body and level preserved (fixed by builder).
        assert_eq!(scrubbed.body, "transcription.decode.started");
        assert_eq!(scrubbed.level, LogLevel::Info);

        // Only allowlisted original attributes survive.
        assert!(scrubbed.attributes.contains_key("phase"));
        assert!(scrubbed.attributes.contains_key("duration_ms"));

        // Dangerous attributes dropped.
        assert!(!scrubbed.attributes.contains_key("user_path"));
        assert!(!scrubbed.attributes.contains_key("transcript"));

        // Safe fixed metadata injected.
        assert_eq!(
            scrubbed
                .attributes
                .get("os")
                .map(|a| a.0.as_str())
                .flatten(),
            Some(std::env::consts::OS)
        );
        assert_eq!(
            scrubbed
                .attributes
                .get("arch")
                .map(|a| a.0.as_str())
                .flatten(),
            Some(std::env::consts::ARCH)
        );
        assert_eq!(
            scrubbed
                .attributes
                .get("app_version")
                .map(|a| a.0.as_str())
                .flatten(),
            Some(env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(
            scrubbed
                .attributes
                .get("release_channel")
                .map(|a| a.0.as_str())
                .flatten(),
            Some(RELEASE_CHANNEL)
        );
        assert_eq!(
            scrubbed
                .attributes
                .get("install_id")
                .map(|a| a.0.as_str())
                .flatten(),
            Some("install-xyz")
        );

        // trace_id preserved (None here since build_transcription_log sets None).
        assert!(scrubbed.trace_id.is_none());
        assert!(scrubbed.severity_number.is_none());
    }

    #[test]
    fn scrub_log_rejects_arbitrary_body() {
        let mut log = build_transcription_log(TranscriptionPhase::DecodeStarted, None);
        // Tamper with the body — simulates a direct sentry::capture_log bypass.
        log.body = "arbitrary text with /Users/alice/secret".into();
        assert!(
            scrub_log(log, None).is_none(),
            "arbitrary body must be rejected"
        );
    }

    #[test]
    fn scrub_log_rejects_mismatched_phase() {
        let mut log = build_transcription_log(TranscriptionPhase::DecodeStarted, None);
        // Body says "decode.started" but phase attribute says "delivery.failed".
        log.attributes
            .insert("phase".into(), LogAttribute::from("delivery.failed"));
        assert!(
            scrub_log(log, None).is_none(),
            "mismatched phase/body pair must be rejected"
        );
    }

    #[test]
    fn scrub_log_rejects_missing_phase_attribute() {
        let mut log = build_transcription_log(TranscriptionPhase::DeliverySucceeded, None);
        log.attributes.remove("phase");
        assert!(
            scrub_log(log, None).is_none(),
            "missing phase attribute must be rejected"
        );
    }

    #[test]
    fn scrub_log_preserves_trace_id() {
        use sentry::protocol::TraceId;
        let mut log = build_transcription_log(TranscriptionPhase::DeliverySucceeded, None);
        // Simulate scope setting a trace_id.
        log.trace_id = Some(TraceId::default());
        let scrubbed = scrub_log(log, None).expect("valid log must pass");
        assert!(
            scrubbed.trace_id.is_some(),
            "trace_id must survive scrub_log for safe correlation"
        );
    }

    #[test]
    fn scrub_log_drops_attributes_without_install_id() {
        let log = build_transcription_log(TranscriptionPhase::RecordingStopped, None);
        let scrubbed = scrub_log(log, None).expect("valid log must pass");
        // install_id not injected when None.
        assert!(!scrubbed.attributes.contains_key("install_id"));
        // But other safe metadata is still present.
        assert!(scrubbed.attributes.contains_key("os"));
        assert!(scrubbed.attributes.contains_key("app_version"));
    }

    #[test]
    fn consent_transport_drops_envelopes_after_opt_out() {
        #[derive(Default)]
        struct CountingTransport(std::sync::atomic::AtomicUsize);

        impl sentry::Transport for CountingTransport {
            fn send_envelope(&self, _envelope: sentry::Envelope) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let _lock = CONSENT_TEST_LOCK.lock().unwrap();
        let was = is_enabled();
        let inner = Arc::new(CountingTransport::default());
        let transport = ConsentTransport {
            inner: inner.clone(),
        };

        set_enabled(true);
        sentry::Transport::send_envelope(&transport, sentry::Envelope::new());
        assert_eq!(inner.0.load(Ordering::SeqCst), 1);

        set_enabled(false);
        sentry::Transport::send_envelope(&transport, sentry::Envelope::new());
        assert_eq!(
            inner.0.load(Ordering::SeqCst),
            1,
            "an envelope flushed after opt-out must be discarded"
        );
        set_enabled(was);
    }

    #[test]
    fn transaction_log_uses_sampled_transaction_trace_id() {
        #[derive(Default)]
        struct NoopTransport;

        impl sentry::Transport for NoopTransport {
            fn send_envelope(&self, _envelope: sentry::Envelope) {}
        }

        let _lock = CONSENT_TEST_LOCK.lock().unwrap();
        let was = is_enabled();
        let observed_trace_id = Arc::new(std::sync::Mutex::new(None));
        let observed_for_callback = observed_trace_id.clone();
        let transport = Arc::new(NoopTransport);
        let client = sentry::Client::from(sentry::ClientOptions {
            dsn: Some("https://public@sentry.invalid/1".parse().unwrap()),
            transport: Some(Arc::new(transport)),
            traces_sample_rate: 1.0,
            enable_logs: true,
            before_send_log: Some(Arc::new(move |log| {
                *observed_for_callback.lock().unwrap() = log.trace_id;
                Some(log)
            })),
            ..Default::default()
        });
        let hub = Arc::new(sentry::Hub::new(
            Some(Arc::new(client)),
            Arc::new(Default::default()),
        ));
        let inner = hub.start_transaction(sentry::TransactionContext::new(
            "transcription",
            "transcribe",
        ));
        assert!(inner.is_sampled());
        let expected_trace_id = inner.get_trace_context().trace_id;
        let transaction = TelemetryTransaction { inner };

        set_enabled(true);
        sentry::Hub::run(hub, || {
            log_transcription_for_transaction(
                Some(&transaction),
                TranscriptionPhase::DecodeStarted,
                None,
            );
        });

        assert_eq!(*observed_trace_id.lock().unwrap(), Some(expected_trace_id));
        set_enabled(was);
    }

    #[test]
    fn log_transcription_respects_consent() {
        let _lock = CONSENT_TEST_LOCK.lock().unwrap();
        let was = is_enabled();
        set_enabled(false);
        // Must not panic or capture when disabled.
        log_transcription(TranscriptionPhase::RecordingStarted, None);
        log_transcription(TranscriptionPhase::DecodeFailed, Some(99));
        set_enabled(was);
    }

    // --- Trace primitive tests -----------------------------------------------

    #[test]
    fn trace_primitives_noop_without_consent_or_client() {
        let _lock = CONSENT_TEST_LOCK.lock().unwrap();
        let was = is_enabled();

        // Disabled: must return None.
        set_enabled(false);
        assert!(
            start_transcription_transaction().is_none(),
            "transaction must not start when disabled"
        );

        // Enabled but no client in the test process: still None (cannot be
        // sampled without a bound client).
        set_enabled(true);
        assert!(
            start_transcription_transaction().is_none(),
            "transaction must not start without a sampled client"
        );

        set_enabled(was);
    }

    #[test]
    fn transcription_span_ops_are_fixed_static_names() {
        assert_eq!(TranscriptionSpan::Decode.op(), "decode");
        assert_eq!(TranscriptionSpan::Formatting.op(), "formatting");
        assert_eq!(TranscriptionSpan::Delivery.op(), "delivery");
    }
}
