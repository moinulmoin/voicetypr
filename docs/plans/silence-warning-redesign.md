# Plan: Silence-warning redesign

## Summary
Replace VoiceTypr’s current destructive 10s silence auto-stop with a tiered, non-destructive silence state machine. The recorder audio callback will only emit sparse `Copy` tier-transition events over a new `std::sync::mpsc` side channel; it will no longer stop the recorder for silence. A backend listener in `commands/audio.rs` will turn warning tiers into persistent pill-toast warnings and will perform the 60s terminal action deliberately: `TimeoutWithSpeech` goes through the full stop/transcribe/paste/history flow, while `TimeoutNoSpeech` calls the existing cancel/discard flow. The existing recorder watchdog stays focused on genuine recorder worker self-termination, such as device errors and the unchanged 500MB backstop.

## Assumptions and decisions
- No user-facing configurability for the 5s/10s/60s thresholds in this change.
- The RMS voice threshold remains `0.005` and speech means `rms > threshold` (strictly supra-threshold, matching the current detector behavior).
- Dead-mic and long-silence warnings should be persistent pill toasts, not short transient toasts, because the detector emits each tier only once and the dead-mic warning must clear when speech returns.
- No genuine product fork remains for the user: the plan chooses a dedicated silence-event listener, not watchdog extension or frontend polling.
- The planner ran no build/test/project commands; this is a read-only implementation plan.

---

## Changes

### 1. `src-tauri/src/audio/silence_detector.rs` — replace bool timeout with tiered state machine
Current code: `/Volumes/1tb-drive/developer/oss/worktrees/voicetypr-stuck-fix/src-tauri/src/audio/silence_detector.rs:1-33` has `SilenceDetector::new(Duration)` and `update(rms) -> bool`, with only `last_voice_time`, `silence_duration`, and `voice_threshold`.

Implement this exact public contract:

```rust
use std::time::{Duration, Instant};

pub const VOICE_RMS_THRESHOLD: f32 = 0.005;
pub const NO_SPEECH_WARNING_AFTER: Duration = Duration::from_secs(5);
pub const LONG_SILENCE_WARNING_AFTER: Duration = Duration::from_secs(10);
pub const SILENCE_TIMEOUT_AFTER: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SilenceDetectorEvent {
    Clear,
    DeadMicWarn,
    LongSilenceWarn,
    TimeoutWithSpeech,
    TimeoutNoSpeech,
}

impl SilenceDetectorEvent {
    #[inline]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::TimeoutWithSpeech | Self::TimeoutNoSpeech)
    }
}

pub struct SilenceDetector {
    started_at: Instant,
    last_voice_time: Instant,
    last_event: SilenceDetectorEvent,
    speech_detected: bool,
    voice_threshold: f32,
    no_speech_warning_after: Duration,
    long_silence_warning_after: Duration,
    silence_timeout_after: Duration,
}

impl SilenceDetector {
    pub fn new() -> Self;
    pub fn update(&mut self, rms: f32) -> Option<SilenceDetectorEvent>;
    pub fn speech_detected(&self) -> bool;
}
```

Private/testable helpers:

```rust
fn new_at(now: Instant) -> Self;
fn update_at(&mut self, rms: f32, now: Instant) -> Option<SilenceDetectorEvent>;
fn emit_if_changed(&mut self, event: SilenceDetectorEvent) -> Option<SilenceDetectorEvent>;
```

State-machine rules:
- Initialize `started_at` and `last_voice_time` to the same `Instant`; initialize `last_event` to `Clear`; initialize `speech_detected` to `false`.
- Return `None` immediately if `last_event.is_terminal()`; terminal events are one-shot and should not be followed by `Clear` if the user starts speaking during the tiny race before app-layer cancellation/stop completes.
- On `rms > voice_threshold`:
  - update `last_voice_time = now`;
  - set `speech_detected = true` if this is the first speech;
  - emit `Clear` only if the previous emitted tier was not `Clear`.
- On `rms <= voice_threshold` and `!speech_detected`:
  - use `now.saturating_duration_since(started_at)`;
  - `>= 60s` emits `TimeoutNoSpeech`;
  - else `>= 5s` emits `DeadMicWarn`;
  - else stays `Clear` with no event.
- On `rms <= voice_threshold` and `speech_detected`:
  - use `now.saturating_duration_since(last_voice_time)`;
  - `>= 60s` emits `TimeoutWithSpeech`;
  - else `>= 10s` emits `LongSilenceWarn`;
  - else stays `Clear` with no event.
- All returned values are small `Copy` enums. Do not allocate strings or heap data in this file.

### 2. `src-tauri/src/audio/recorder.rs` — add silence side channel and remove `StopSilence`
Relevant current ranges:
- Struct/channel fields and `RecorderCommand`: `/Volumes/1tb-drive/developer/oss/worktrees/voicetypr-stuck-fix/src-tauri/src/audio/recorder.rs:26-68`
- Audio-level side-channel precedent: `:101-113`, `:145-170`, `:468-553`
- Realtime callback silence stop: `:221-280`
- Worker stop reason strings: `:360-455`
- Existing tests: `:552-670`

Concrete changes:
- Import both detector types:

```rust
use super::silence_detector::{SilenceDetector, SilenceDetectorEvent};
```

- Extend `AudioRecorder` with a receiver matching the existing audio-level pattern:

```rust
pub struct AudioRecorder {
    recording_handle: Arc<Mutex<Option<RecordingHandle>>>,
    audio_level_receiver: Arc<Mutex<Option<mpsc::Receiver<f64>>>>,
    silence_event_receiver: Arc<Mutex<Option<mpsc::Receiver<SilenceDetectorEvent>>>>,
}
```

- Initialize and clear `silence_event_receiver` in `new()`, `Drop`, `start_recording()`, and `stop_recording()` alongside `audio_level_receiver`.
- Add:

```rust
pub fn take_silence_event_receiver(&mut self) -> Option<mpsc::Receiver<SilenceDetectorEvent>>;
```

- In `start_recording()`:
  - remove local `silence_duration = Duration::from_secs(10)`;
  - create `let (silence_event_tx, silence_event_rx) = mpsc::channel::<SilenceDetectorEvent>();` near the audio-level channel;
  - construct `SilenceDetector::new()`;
  - clone `silence_event_tx` into the callback.
- In the audio callback, replace the current stop-on-silence block with:

```rust
if let Ok(mut detector) = silence_detector_clone.try_lock() {
    if let Some(event) = detector.update(rms) {
        let _ = silence_event_tx_clone.send(event);
    }
}
```

- Do **not** send any recorder stop command from the callback for silence. This is the core behavioral cutover.
- Keep the 500MB size-cap check exactly as a backstop: `RecordingSize::MAX_RECORDING_SIZE = 500 * 1024 * 1024` and the callback size check around `recorder.rs:278` remain a self-stop path.
- Delete `RecorderCommand::StopSilence`; `RecorderCommand` should only need `Stop` after this change.
- Remove the `StopSilence => "Recording stopped due to silence"` worker message branch. The app layer now owns silence notifications; stop-message substring detection must go away.
- Update `recording_thread_finished()` comments/tests to no longer list silence timeout as an autonomous self-stop. It remains true for genuine worker self-termination such as device errors and size cap.

### 3. `src-tauri/src/commands/audio.rs` — extend toast API without creating a second convention
Relevant current ranges:
- Current toast payload/API: `/Volumes/1tb-drive/developer/oss/worktrees/voicetypr-stuck-fix/src-tauri/src/commands/audio.rs:120-171`
- Existing toast call sites include `:1825-1827`, `:1931-1933`, `:2253-2255`, `:2643-2670`.

Update the existing unified pill-toast API; do not create a new window or new event name.

Exact backend payload/signatures:

```rust
#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PillToastAction {
    Show,
    Clear,
}

#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PillToastVariant {
    Info,
    Warning,
}

#[derive(serde::Serialize, Clone)]
pub struct PillToastPayload {
    pub id: u64,
    pub action: PillToastAction,
    pub message: String,
    pub duration_ms: u64,
    pub variant: PillToastVariant,
    pub persistent: bool,
}

pub fn pill_toast(app: &AppHandle, message: &str, duration_ms: u64) -> u64;
pub fn pill_toast_with_variant(
    app: &AppHandle,
    message: &str,
    duration_ms: u64,
    variant: PillToastVariant,
) -> u64;
pub fn pill_toast_persistent(
    app: &AppHandle,
    message: &str,
    variant: PillToastVariant,
) -> u64;
pub fn clear_pill_toast(app: &AppHandle, toast_id: u64);
```

Implementation notes:
- Keep the event name `"toast"` and continue emitting with `app.emit("toast", payload)` from `pill_toast*`; this is an extension of the current unified API at `src-tauri/src/commands/audio.rs:120-171`, not a second convention.
- `pill_toast()` now returns the generated `u64` id. Existing callers can ignore it.
- `pill_toast()` delegates to `pill_toast_with_variant(..., PillToastVariant::Info)`.
- `pill_toast_with_variant()` delegates to an internal helper with `persistent=false`; it shows the `toast` window and spawns the current backend hide timer only for non-persistent toasts.
- `pill_toast_persistent()` uses the same payload/event/window but does not spawn a hide timer.
- `clear_pill_toast(app, toast_id)` must only clear if that toast is still current. Use `TOAST_ID_COUNTER.compare_exchange(toast_id, toast_id.wrapping_add(1), SeqCst, SeqCst)`. If it fails, do nothing so speech-return does not clear an unrelated newer toast such as ESC. If it succeeds, hide the `toast` window and emit `PillToastPayload { id: clear_id, action: Clear, message: String::new(), duration_ms: 0, variant: Info, persistent: false }`.
- Remove the old `if stop_message.contains("silence") { pill_toast("No sound detected", ...) }` block at `audio.rs:1825-1827`; silence copy now comes from the event listener.

## 4. App-layer silence-event listener (`src-tauri/src/commands/audio.rs`)

Do not extend `RecorderWatchdog`; it must remain the recovery layer for genuine recorder worker self-termination in `src-tauri/src/audio/recorder_watchdog.rs:1-90`. Silence no longer self-terminates the recorder, so the watchdog should not observe silence terminals.

Add a dedicated listener in `start_recording()` (`audio.rs:1196-1668`) beside the existing audio-level receiver code (`audio.rs:1551-1572`). Change the local receiver extraction from one receiver to two:

```rust
let (audio_level_rx, silence_event_rx) = match recorder.start_recording(...) {
    Ok(_) => {
        let level_rx = recorder.take_audio_level_receiver();
        let silence_rx = recorder.take_silence_event_receiver();
        (level_rx, silence_rx)
    }
    Err(e) => ...,
};
drop(recorder);

if let Some(silence_event_rx) = silence_event_rx {
    spawn_silence_event_listener(app.clone(), silence_event_rx);
}
if let Some(audio_level_rx) = audio_level_rx { /* existing level thread */ }
```

Add helper near the toast helpers or just above `start_recording()`:

```rust
fn spawn_silence_event_listener(
    app: AppHandle,
    silence_event_rx: std::sync::mpsc::Receiver<SilenceDetectorEvent>,
);
```

Implementation: use `std::thread::spawn` like the audio-level consumer because the receiver is `std::sync::mpsc::Receiver`. The thread owns `AppHandle`, drains `while let Ok(event) = silence_event_rx.recv()`, and exits naturally when the recorder stops and the sender is dropped. Keep `let mut active_silence_toast_id: Option<u64> = None;` inside the listener thread.

Mapping:
- `DeadMicWarn`: `active_silence_toast_id = Some(pill_toast_persistent(&app, "No audio detected — check your microphone", PillToastVariant::Warning));`
- `LongSilenceWarn`: `active_silence_toast_id = Some(pill_toast_persistent(&app, "Long silence detected", PillToastVariant::Warning));`
- `Clear`: if `active_silence_toast_id.take()` is `Some(id)`, call `clear_pill_toast(&app, id)`.
- `TimeoutWithSpeech`: immediately replace any warning with `pill_toast_with_variant(&app, "Ended after long silence", 1500, PillToastVariant::Info)`, then spawn async stop/transcribe and break the listener loop.
- `TimeoutNoSpeech`: spawn async cancel/discard, then show `pill_toast_with_variant(&app, "No audio captured", 1500, PillToastVariant::Warning)` on success; break the listener loop.

Before acting on any event, check `crate::get_recording_state(&app)`. Warnings should be shown only while `Recording` (or `Starting` if the callback somehow fires before the state commit). Terminal events should be ignored if state is already `Stopping`, `Transcribing`, `Idle`, or `Error`, because a user/manual stop won the race.

Terminal async calls:

```rust
// TimeoutWithSpeech
let app_for_stop = app.clone();
tauri::async_runtime::spawn(async move {
    let st = app_for_stop.state::<RecorderState>();
    if let Err(e) = stop_recording_after_long_silence(app_for_stop.clone(), st).await {
        log::error!("Long-silence stop failed: {}", e);
    }
});

// TimeoutNoSpeech
let app_for_cancel = app.clone();
tauri::async_runtime::spawn(async move {
    match cancel_recording(app_for_cancel.clone()).await {
        Ok(()) => pill_toast_with_variant(&app_for_cancel, "No audio captured", 1500, PillToastVariant::Warning),
        Err(e) => {
            log::error!("No-speech timeout cancel failed: {}", e);
            pill_toast_with_variant(&app_for_cancel, "Recording error", 1500, PillToastVariant::Warning);
        }
    }
});
```

Add a private stop intent so `TimeoutWithSpeech` really means full pipeline. Refactor current `stop_recording(app, state)` body (`audio.rs:1734-2705`) into:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StopRecordingIntent { Normal, LongSilenceWithSpeech }

fn should_discard_likely_silence(intent: StopRecordingIntent) -> bool {
    // Discard the likely-silence audio ONLY for normal/manual stops.
    // A speech-bearing 60s timeout (LongSilenceWithSpeech) MUST be transcribed, never discarded.
    matches!(intent, StopRecordingIntent::Normal)
}

async fn stop_recording_internal(
    app: AppHandle,
    state: State<'_, RecorderState>,
    intent: StopRecordingIntent,
) -> Result<String, String>;

#[tauri::command]
pub async fn stop_recording(app: AppHandle, state: State<'_, RecorderState>) -> Result<String, String> {
    stop_recording_internal(app, state, StopRecordingIntent::Normal).await
}

async fn stop_recording_after_long_silence(
    app: AppHandle,
    state: State<'_, RecorderState>,
) -> Result<String, String> {
    stop_recording_internal(app, state, StopRecordingIntent::LongSilenceWithSpeech).await
}
```

Inside the existing normalized-audio silence branch at `audio.rs:2253-2268`, only discard/delete/return when `should_discard_likely_silence(intent)` is true. For `LongSilenceWithSpeech`, log that the detector saw speech before the terminal and continue to transcription. Keep the header-only WAV guard at `audio.rs:1931-1933`; a file with no audio bytes is still no content.

## 5. Frontend toast window (`src/components/FeedbackToast.tsx`)

Current component path: `src/components/FeedbackToast.tsx:1-60`; entry: `src/toast.tsx:1-12`; toast window created in `src-tauri/src/lib.rs:969-1008`.

Update the TS contract explicitly:

```ts
type PillToastAction = "show" | "clear";
type PillToastVariant = "info" | "warning";

interface PillToastPayload {
  id: number;
  action: PillToastAction;
  message: string;
  duration_ms: number;
  variant: PillToastVariant;
  persistent: boolean;
}

interface VisibleToast {
  id: number;
  message: string;
  variant: PillToastVariant;
}
```

Use `useRef<number | null>(null)` with `window.setTimeout/window.clearTimeout`; do not keep `ReturnType<typeof setTimeout>`. Track `latestIdRef` and ignore stale payloads/timers. On `action === "clear"`, clear the timer and set visible toast to `null`. On `action === "show"`, set visible toast; if `persistent` is false and `duration_ms > 0`, schedule a timer that only clears if `latestIdRef.current === payload.id`.

Minimal variant rendering:
- Import `Info` and `TriangleAlert` from `lucide-react`.
- Info: keep the current compact dark pill look (`bg-black text-white ring-white/30`) with `Info` or the existing app icon if preferred.
- Warning: use semantic Tailwind color names, e.g. `bg-amber-950 text-amber-50 ring-amber-400/40`, icon `TriangleAlert className="h-4 w-4 text-amber-300"`.
- Add `role="status"` and `aria-live="polite"` to the rendered toast container.

## Thresholds
Defined only in `src-tauri/src/audio/silence_detector.rs`:
- `VOICE_RMS_THRESHOLD: f32 = 0.005`
- `NO_SPEECH_WARNING_AFTER = Duration::from_secs(5)`
- `LONG_SILENCE_WARNING_AFTER = Duration::from_secs(10)`
- `SILENCE_TIMEOUT_AFTER = Duration::from_secs(60)`

The 500MB cap in `src-tauri/src/audio/recorder.rs:12-24` is unchanged.

## Modes
Both Toggle and PTT call the same backend `start_recording()` and `stop_recording()` paths from `src-tauri/src/recording/hotkeys.rs:115-263`. Because the detector lives in `AudioRecorder` and the listener is spawned from `start_recording()`, warnings and terminal behavior fire identically in both modes. PTT release can still stop before 60s; if the 60s terminal wins first, later key release only clears `ptt_key_held` and should not restart/stop anything.

## Tests

Rust detector tests: co-locate in `src-tauri/src/audio/silence_detector.rs`.
- `starts_clear_without_speech_before_threshold`
- `dead_mic_warns_once_after_five_seconds_without_speech`
- `speech_above_threshold_flips_speech_detected`
- `threshold_equal_to_voice_threshold_is_not_speech`
- `dead_mic_warning_clears_when_speech_arrives`
- `post_speech_long_silence_warns_once_after_ten_seconds`
- `long_silence_warning_clears_when_speech_resumes`
- `post_speech_timeout_emits_timeout_with_speech_once_after_sixty_seconds`
- `no_speech_timeout_emits_timeout_no_speech_once_after_sixty_seconds`
- `terminal_event_is_final_and_never_clears_after_late_speech`
Use private `new_at()`/`update_at()` with `Instant + Duration`; no sleeps.

Rust recorder tests: update existing `src-tauri/src/audio/recorder.rs` tests.
- Adjust `recording_thread_finished_is_true_after_worker_self_exits` comments to remove silence as a fake autonomous stop.
- Add `take_silence_event_receiver_consumes_receiver` if the new receiver method is non-trivial.

Rust command tests: add to existing `#[cfg(test)] mod tests` in `src-tauri/src/commands/audio.rs:418-520`.
- `silence_terminal_with_speech_routes_to_stop_and_transcribe`
- `silence_terminal_without_speech_routes_to_cancel_and_discard`
- `silence_warnings_have_no_terminal_action`
- `speech_timeout_never_discards_likely_silence_audio`
- `normal_stop_still_discards_likely_silence_audio`
- `clear_pill_toast_only_clears_matching_current_toast_id` if helper is easy to isolate; otherwise test via pure id helper.

Frontend tests: create `src/components/FeedbackToast.test.tsx` using existing Tauri event mock in `src/test/setup.ts:170-184`.
- show transient info toast and auto-clear after duration;
- show persistent warning and verify it does not auto-clear;
- clear payload hides current toast;
- stale transient timer does not clear newer persistent warning;
- warning variant renders `TriangleAlert`/warning class.

## Parallelization grouping

Sequenced shared-contract work:
1. `silence_detector.rs` enum/API/constants first; recorder and command code import this type.
2. Toast payload/helper API in `commands/audio.rs` before frontend and listener code rely on `action/variant/persistent`.

Parallel after contracts are in place:
- Backend recorder wiring in `src-tauri/src/audio/recorder.rs` can proceed independently of frontend rendering.
- Frontend toast rendering/tests in `src/components/FeedbackToast.tsx` and `.test.tsx` can proceed independently once payload shape is fixed.
- `stop_recording_internal` intent refactor and command pure tests in `commands/audio.rs` can proceed independently of recorder callback details.

Final sequenced integration:
- Add `spawn_silence_event_listener()` and wire receiver extraction in `start_recording()` after recorder receiver method and toast helpers exist.
- Then run focused tests and final gates.

## Verification for implementer/orchestrator
- `cd src-tauri && cargo test silence_detector`
- `cd src-tauri && cargo test silence_terminal`
- `pnpm vitest run src/components/FeedbackToast.test.tsx`
- Final orchestrator gate after subagents: Rust tests/clippy with `-D warnings`, TS typecheck, and targeted frontend tests.

## Explicit non-goals
- No threshold settings UI or persisted configuration.
- No new Tauri window, main-window event, or frontend polling path for silence.
- No change to the RMS threshold value.
- No change to 500MB size-cap behavior.
- No redesign of PTT/toggle hotkey semantics.
- No broad migration of all existing info/error toasts to variants; only silence-warning paths need `Warning` now.
- No mocks for real audio hardware; detector tests feed RMS values directly.

## Oracle review corrections (AUTHORITATIVE — overrides any conflicting snippet above)

Incorporates the pre-implementation oracle review. Where these conflict with earlier text, THESE win.

**C1 (fixed inline in §4): discard predicate.** `should_discard_likely_silence` returns true ONLY for `StopRecordingIntent::Normal`. The likely-silence delete/return block at `audio.rs:2248-2268` is gated by it; `LongSilenceWithSpeech` logs and falls through to transcription.

**M2: realtime-safe callback channel.** Do NOT use an unbounded `mpsc::channel` + `send` from the audio callback. Create a bounded sync channel in `start_recording()`: `let (silence_event_tx, silence_event_rx) = std::sync::mpsc::sync_channel::<SilenceDetectorEvent>(8);`. Move the `SyncSender` clone into the callback and emit with `let _ = silence_event_tx_clone.try_send(event);` — non-blocking, no allocation, drops on the rare full buffer (acceptable: detector events are one-shot/sparse). `take_silence_event_receiver()` returns the `std::sync::mpsc::Receiver<SilenceDetectorEvent>`; the listener drains with `recv()`.

**M3: clear the persistent toast on EVERY listener exit path.** The persistent dead-mic / long-silence toast has no backend hide timer, so it MUST be explicitly cleared whenever the listener stops without showing a replacement toast: (a) channel closed (recorder stopped / manual stop / recorder error), (b) a terminal event ignored because state is no longer `Recording`/`Starting`, (c) after a terminal action if no synchronous replacement toast was shown. On each path run `if let Some(id) = active_silence_toast_id.take() { clear_pill_toast(&app, id); }`.

**M4: superseding toasts — ACCEPTED behavior, no restoration layer.** A newer transient toast (ESC "Press ESC again to cancel", "Hold on...", GPU-unavailable) may replace an active persistent silence warning, and the warning will NOT auto-restore after the transient one expires (the detector emits each tier once). Accepted: the warning already conveyed its info, competing toasts are rare and user-initiated, and a priority/restore layer is out of scope. The `clear_pill_toast` compare_exchange guard still prevents a stale Clear from clearing the newer toast.

**M5: cancellation-flag / CPU-decode semantics (must preserve).** `stop_recording_after_long_silence` (intent `LongSilenceWithSpeech`) goes through `stop_recording_internal` and MUST NOT set the `should_cancel_recording` flag or call `request_cancellation` — it preserves the existing stop/transcribe flow (audio.rs:1764-1765 deliberately does not cancel). `TimeoutNoSpeech` is the ONLY silence terminal allowed to call `cancel_recording` (which sets the cancellation flag and has the special CPU-decode abort, audio.rs:3397-3419). Tests MUST assert: speech-bearing timeout never touches the cancellation flag and reaches transcription; no-speech timeout cancels/discards.

**M6: recording-end sound.** `TimeoutWithSpeech` → stop pipeline → plays the normal recording-end sound (audio.rs:1803-1812). `TimeoutNoSpeech` → cancel semantics → does NOT play the end sound. Intended; do not add or duplicate sounds.

**M7: extract receivers before dropping the lock; spawn listeners outside the lock.** Return `(audio_level_rx, silence_event_rx)` from the scoped recorder section, drop the recorder mutex guard, THEN spawn the silence listener and the audio-level thread. Never spawn while holding the recorder mutex.
