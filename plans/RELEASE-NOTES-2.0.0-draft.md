# VoiceTypr 2.0.0 — Release Notes (draft)

VoiceTypr 2.0 is a ground-up architectural upgrade that ships an **agent & automation interface (CLI + local HTTP API) for driving VoiceTypr programmatically**, cloud transcription, remote LAN sharing, a Rust-native AI formatting engine, a new native hotkey system, rich transcript history, and dramatically improved recording reliability — all while keeping local transcription by default. The release covers 242 commits across the full V1→V2 divergence, touching every major subsystem from the Parakeet/Whisper engine all the way to the onboarding flow.

---

## Features

### Agent & automation surface

VoiceTypr ships a scriptable CLI plus the Network Sharing HTTP API, so **agents and scripts can drive transcription programmatically**.

- **Agent CLI** — `voicetypr status`, `voicetypr models`, `voicetypr transcribe --file <path>`, and `voicetypr record --until-silence`. Human-readable by default; add `--json` to any command for machine-readable output.
- **Structured JSON output** — local `transcribe --json` emits `{ text, words, metadata, model, engine }`; local `record --json` adds `stop_reason`. `status --json` emits `{ version, settings, availability }`; `models --json` emits the typed model-status response. (With `--server`, the result is the remote writing shape `{ text, output_language, mode, applied_operations, warnings, model, duration_ms }`, plus `stop_reason` for `record`.)
- **Per-call engine/model override (local path)** — `transcribe`/`record` take `--model <id>` and `--engine <whisper|parakeet|…>` to choose the recognizer per call without changing global settings. (When routing with `--server`, the host's shared model is used instead.)
- **Route to a running instance** — `transcribe`/`record` accept `--server <host:port>` + optional `--password` to send audio to a running VoiceTypr's Network Sharing server (`/api/v1/transcribe`; default sharing port 47842, but the CLI requires an explicit `host:port`). Without `--server`, the CLI transcribes in-process. Note: the HTTP API exists only while Network Sharing is enabled — it is not an always-on localhost daemon. No MCP server ships in 2.0.0.

### Cloud transcription

- **Five cloud STT providers in Models tab** — Choose Soniox, OpenAI (GPT-4o Transcribe), Groq (Whisper Large v3 Turbo), Deepgram (Nova-3), or Cohere from Settings → Models → Cloud transcription; each has its own API key slot, a speed/accuracy hint label, and can be selected as the active transcription source just like a local model.
- **Curated per-provider cloud models** — Each provider maps to one purpose-selected cloud model rather than an open-ended catalog; the choice is "which service" not "which of hundreds of checkpoints."
- **Provider-specific language clamping** — The spoken-language selector automatically limits choices to what each engine supports: Soniox/Cohere clamp to their static supported sets, Parakeet checks model language lists, OpenAI/Groq/Deepgram accept validated language codes.
- **Cloud custom vocabulary** — Your Words & Names dictionary is now sent to the cloud recognizers whose APIs support it: OpenAI and Groq receive it as the transcription `prompt`, and Deepgram (Nova-3) as repeatable `keyterm` hints, so jargon and proper nouns (brand names, technical terms like `shadcn/ui`) are recognized at the source. Soniox already received vocabulary; Cohere has no such API and is unaffected. The prompt is capped to stay within the providers' token limit.
- **Cloud speaker diarization for uploaded files** — Upload a multi-speaker recording via Deepgram or Soniox and the result groups speech into labeled `Speaker 0:` / `Speaker 1:` paragraphs that can be saved or copied; single-speaker audio and non-diarizing providers (OpenAI, Groq, Cohere) return a plain transcript. Live dictation is unaffected.
- **Cloud reliability policy** — All cloud STT calls share a 120 s request timeout, 15 s connect timeout, and a single automatic retry for transient network/rate-limit/server errors; auth failures return typed user-safe messages without leaking raw provider response bodies.
- **Cloud sources block network sharing** — When a cloud engine is active, the Network Sharing card warns "Cloud sources cannot be shared over the network" and disables sharing until a local model is selected.

### Local transcription & models

- **Parakeet custom vocabulary via CTC rescore** — macOS Apple-Silicon Parakeet models accept a custom vocabulary list that is applied at decode time, improving accuracy for domain-specific or uncommon words.
- **Custom vocabulary forwarded to Whisper** — Local Whisper transcription incorporates the same vocabulary context so specialized terms are recognized consistently across engines.
- **Parakeet model download repair** — The Models screen can trigger a repair flow that re-downloads a Parakeet model without losing existing models; download progress and deletion are guarded to prevent concurrent conflicts.
- **Verified Whisper model downloads** — Whisper model URLs are pinned to a specific Hugging Face revision; download completion is gated on an exact file-size match, and SHA256/SHA1 checksums are verified after download or when accepting an existing file. Checksum mismatches automatically delete the corrupt file.
- **Model download responsiveness** — The Models panel stays interactive during downloads; a `model-verifying` phase is shown after 100% so progress is never ambiguous. Duplicate download and delete requests for the same model are rejected immediately.
- **Parakeet sidecar engine for Apple Neural Engine models** — macOS Parakeet models run through a Swift/FluidAudio sidecar that loads, transcribes, and cancels via JSON protocol, with protocol hardening so CoreML stdout/stderr redirection cannot corrupt messages.

### GPU/CPU acceleration (Windows)

- **Windows transcription acceleration picker** — Settings → General and the onboarding flow show an Auto / GPU / CPU selector. Auto tries the Vulkan GPU sidecar and falls back to CPU transparently; explicit GPU mode shows "GPU unavailable, using CPU" when Vulkan is not available; macOS continues to use Metal automatically with no picker.
- **Windows Vulkan GPU sidecar** — A separate `whisper-vulkan-sidecar` process handles GPU transcription on Windows with probe/transcribe JSON requests, runtime status reporting ("Last transcription used GPU acceleration…"), abort support for cancel/watchdog, and model preload warming so the first recording after a manual preload is fast.
- **Windows CPU fast-decode profile** — CPU Whisper on Windows uses a greedy decoder (`best_of: 1`, no context/timestamps) and leaves one core free; Windows ARM64 caps thread count around performance cores. The result is meaningfully faster CPU transcription at comparable accuracy.
- **GPU cancel does not poison acceleration status** — Cancelling or timing out a GPU transcription does not mark the sidecar as broken; Auto mode stays in GPU preference for the next recording.

### AI formatting

- **Rust-native AI polish runtime** — AI formatting now runs entirely in the Rust backend using OpenAI, Anthropic, Gemini, or any Custom OpenAI-compatible endpoint. The Pi/Node formatting sidecar is gone; the app works without it and existing provider/model settings are honored unchanged.
- **Searchable AI provider and model catalog** — The Formatting tab has a search field that filters across provider names and model IDs. Results are grouped into Recommended and All, with per-model badges for reasoning support, context window, and cost. The underlying catalog is generated from a models.dev snapshot (~94 models) at build time; no network fetch happens at runtime.
- **Formatting modes** — Six formatting modes replace the old single-toggle: Personal Dictation, Clean Dictation, Writing, Notes, Message, and Code. AI-requiring modes (Writing, Notes, Message, Code) are locked with an icon until AI formatting is enabled; disabling AI falls back to Personal Dictation automatically.
- **Clearer Formatting screen + rewritten polish prompt** — The Formatting tab is reorganized into two labeled zones — "AI polish (optional)" and "Your text rules (always on)" — so it's obvious which settings need an AI provider versus which always run. The AI-polish prompt was rewritten to be plain and model-agnostic: ordered semantic-correction steps, a single output instruction, and a guard so dictated text can't hijack the model.
- **App Rules for per-app formatting** — Users can define rules that switch the formatting mode based on the active foreground app name (e.g. Slack → Message). The earlier fuzzy "context-aware cleanup" app-hint (which passed the app name/category into the AI prompt) was removed in favor of these explicit, predictable rules.
- **Deterministic writing pipeline** — Before any AI call, a deterministic stage applies transcript sanitize, text corrections, Words & Names substitutions, Text Shortcuts, and Voice Commands. If AI formatting subsequently fails for a non-translation path, the deterministic output is inserted rather than raw speech.
- **User-editable voice commands** — Voice Commands are now managed through the Formatting Options UI rather than being fixed; users can add, edit, and delete them.
- **Transcript sanitize** — A deterministic stage normalizes whitespace and strips control characters before any rules run. Semantic cleanup (fillers, grammar, punctuation, capitalization) is the AI's job, so the model still receives the raw disfluencies it needs to resolve self-corrections.
- **Custom OpenAI-compatible endpoint** — Users can point the Custom provider to any base URL with an optional model name and no-auth mode; the Test action probes `/chat/completions` with the chosen model (not the `/models` list) so a bad endpoint fails before it is saved.
- **Per-provider remembered model** — Switching providers restores the last model you used with that provider; the selection is saved per-provider so switching back does not lose context.
- **Bindable "Toggle AI formatting" shortcut** — Settings → Shortcuts → Formatting includes a "Toggle AI formatting" action; pressing it flips polish on/off, updates the pill and Enhancements UI, and shows a setup prompt if no AI model/key is configured.
- **Active model visibility** — The Formatting tab header shows "Active model: <provider> / <model>" or "Selected model … (AI formatting off)" so the configured model is always visible before enabling formatting.
- **Separate AI and STT key namespaces** — AI polish credentials live under `ai_api_key_{provider}` and cloud STT keys under `stt_api_key_{provider}`; they cannot silently share credentials even when both use the same provider.
- **Dictionary-guided AI spelling correction** — Your Words & Names dictionary is passed to AI polish as a sanitized, terms-only reference so the model fixes misheard or misspelled terms (e.g. "shad cn" → "shadcn/ui") to your preferred spelling — used only for spelling, never as instructions.

### Hotkeys & shortcuts

- **Native cross-platform keytrigger engine** — A new observation-only Rust engine (`keytrigger` crate) listens to physical keyboard events via macOS CGEventTap and Windows WH_KEYBOARD_LL and drives shortcut actions that the legacy global-shortcut plugin cannot express. The engine starts at launch and retries automatically after Accessibility is granted.
- **Hold-a-modifier push-to-talk** — Users can bind a bare side-specific modifier (e.g. Right Option on macOS, Right Alt on Windows) as "Hold to record": press starts recording, release stops, and the modifier is not consumed by the system so normal typing is unaffected.
- **Tap-a-modifier to toggle recording** — A bare modifier can also be configured as an isolated-tap toggle; it fires only when tapped alone, so combinations like Command+C still work normally.
- **Single-key shortcuts on any action** — Settings → Shortcuts allows safe single-key bindings (function keys, numpad keys, navigation keys) on recording, history, formatting, and dashboard actions, with a global cap of 5 single-key shortcuts. Typing keys (letters, numbers) are explicitly rejected with an explanatory message.
- **Simplified primary recording hotkey editor** — The General Settings hotkey row uses a single Edit → capture → Save/Cancel flow with a "Hold to talk (push-to-talk)" switch for bare modifiers; it directs users to Settings → Shortcuts for additional per-action bindings, removing the earlier duplicate-editor confusion.
- **Unified shortcut dispatch path** — Legacy combo/single-key shortcuts and native keytrigger events both route through the same dispatch function so recording, cancel, copy last transcription, formatting mode toggle, and dashboard actions behave consistently regardless of which binding triggered them.
- **Self-injected paste events filtered from the native engine** — After transcription, VoiceTypr's own paste does not re-trigger modifier-hold, double-tap, or chord shortcuts; macOS filters by PID, Windows drops `LLKHF_INJECTED` events.
- **Held toggle hotkeys ignore OS auto-repeat** — A toggle hotkey held down produces exactly one stop event; the existing 300 ms throttle also applies, preventing rapid-stop flapping.

### History & transcripts

- **Rich filterable transcription history** — Recent Recordings has source filters (All / Dictation / Upload / Remote), time filters (All time / Today / Last 7 days), and per-app filters. Each filter reports result counts and offers a Clear filters action. Old rows without metadata appear only under "All sources."
- **History metadata badges per entry** — Each row can display the engine/model, date/time, source type, recording duration, number of speakers (for diarized uploads), and the active app when recorded (shown when App Rules are configured).
- **Original vs formatted transcript toggle** — History rows where AI changed the text show "Show original / Show formatted"; the pre-AI text is stored locally only and never logged. The toggle appears only when AI actually modified the transcript.
- **Failed/retryable history rows** — When `save_recordings` is enabled and a transcription fails, a "Transcription failed — re-transcribe after resolving the issue" row is created that preserves the audio for a later retry. Cancellation and too-short clips never create preserved rows.
- **Translation-failed row markers** — When output-language translation is required but fails, the row is saved with an "untranslated" badge so users do not mistake raw speech text for a successful translation.
- **In-place re-transcription from history** — Any history row with a preserved recording can be re-transcribed using the current active source (local/cloud/remote) without re-recording. The re-transcription flow shows "Re-transcribing with <source>…" so the source is clear.
- **History self-documenting guide** — The Recent Recordings header includes a guide dialog explaining search, re-transcribe, and export so the screen is self-explanatory for new users.
- **Enriched TranscriptionResult DTO** — The shared transcription result type carries `source`, word-level timing data, and metadata fields so all downstream consumers (history, CLI, upload save) receive the same structured output.
- **Re-transcribe button and dropdown in history** — History items expose a re-transcribe action with a source dropdown so users can switch engine mid-history-item without going to Settings.

### Upload & export

- **Save uploaded transcript to .txt or .md** — After uploading a file and transcribing it, a Save button opens a native save dialog with `.txt` and `.md` file filters. The Markdown file includes a `# <filename>` heading. Cancelling the dialog writes nothing.
- **Upload transcription via remote server** — File upload can route through a remote VoiceTypr instance as the transcription engine, not only local models.

### Remote sharing (Network Sharing & Remote Transcription)

- **Network Sharing host** — Any VoiceTypr instance can expose its local transcription engine over the LAN via a built-in HTTP API (`/api/v1/status`, `/api/v1/transcribe`, `/api/v1/control/models`). Status advertises version, machine ID, engine/model, and capability metadata. An optional password protects the sharing endpoint.
- **Remote server management in Models tab** — The Models tab lists discovered LAN servers (via Scan) and saved remote devices; users can add, select, deselect, remove, and edit remote servers. Selecting a remote server routes all dictation to that machine.
- **Remote servers in tray menu** — The system tray model selector includes remote VoiceTypr servers alongside local models so switching is possible without opening the main window.
- **Remote model control** — When password-protected sharing is enabled, a remote client can view and switch the host's shared local model; cloud engines are blocked from sharing with a clear message.
- **Real active-connection count** — Network Sharing tracks distinct peer IPs that have made valid transcription requests in the last 5 minutes and displays that count in the sharing card, replacing the previous always-0 display.
- **Graceful IP binding with UI status feedback** — The sharing server binds to localhost plus individual IPv4 interface addresses (not 0.0.0.0) and shows each address with its bind status; failed addresses appear dimmed with "could not use this address."
- **Windows firewall detection** — On Windows, the Network Sharing card detects whether the Windows firewall may be blocking the sharing port and shows remediation steps with an open-settings/check-again action.
- **Provider capability matrix** — Remote hosts advertise their transcription capabilities so clients can make informed routing decisions.
- **Preserve recordings on remote transcription failure** — If a remote transcription fails, the local recording is preserved according to the `save_recordings` setting, exactly like local/cloud failures.

### Onboarding

- **Onboarding source picker** — Setup begins with a clear choice between "Use this device" (local, offline-capable) and "Remote VoiceTypr" (another machine on the network), with copy explaining the tradeoffs of each.
- **Explicit permission step in onboarding** — A dedicated step asks users to grant Microphone and Accessibility permissions, with clear explanations of why each is needed (recording vs system-wide hotkeys) and recheck buttons.
- **Onboarding model/readiness flow with GPU toggle** — Local setup shows model cards with download, cancel, and delete; Windows shows a "Use GPU acceleration" toggle. Remote setup lists discovered and saved servers with "Use this server" and "Add server" actions.
- **Onboarding hotkey mode picker** — The hotkey step accepts a combo shortcut, a safe single key, or a bare modifier. A "Hold to talk (push-to-talk)" switch determines whether a modifier means hold-to-record or tap-to-toggle; the selection is saved as the appropriate global or native binding.
- **First-transcription test with editable result** — The final onboarding step has Start sample / Stop and transcribe, a read-aloud example sentence, an editable transcript text box, and a "Skip for now" escape if the sample is not available.
- **Accessibility permission polling** — When Accessibility is needed but not yet granted, the app polls for permission changes without requiring a restart.

### Recording reliability

- **Cancel-safe start flow** — Pressing Escape or releasing a PTT key during startup (while audio device or model is initializing) cleanly stops the recorder, removes temp audio, resumes paused media, and returns to Idle. Previously this could leave a stuck pill or recorder.
- **Non-destructive silence warnings** — If the user stays silent for ~5 seconds, the pill shows "No audio detected — check your microphone" as a persistent non-blocking warning instead of stopping the recording. The warning clears automatically when sustained speech resumes.
- **Sustained-voice gating (300 ms threshold)** — Brief audio blips under ~300 ms do not count as speech; this prevents accidental noise from clearing the dead-mic warning or preserving a near-silent recording.
- **Silence-timeout never loses speech** — After 60 seconds of silence following prior speech, the recording stops and transcribes with "Ended after long silence." After 60 seconds of silence with no speech detected at all, the recording is discarded with "No audio captured."
- **RecorderWatchdog auto-recovery** — A background 250 ms watcher detects when the audio recorder worker has exited while the app still shows "Recording" state, and drives the normal stop flow once. The watcher re-arms for the next session.
- **Never-lose-speech on recorder device errors** — If a device error occurs after the WAV was successfully written, audio continues into transcription; only unfinalized failures show "Recording error." Dropped-chunk integrity failures show "Recording was interrupted."
- **Pause media during recording (default off)** — An opt-in General setting pauses media players at recording start and resumes them only if VoiceTypr was the one that paused them. Fresh configurations default this to off.
- **Shared desktop transcription executor** — Local Whisper, local Parakeet, and cloud recordings all enter a single executor that provides cancel-anywhere, decode watchdog, silence handling, device-disconnect recovery, and first-word preservation. Previously each engine had separate cancel/timeout paths.
- **Quiet-speech gain boost without amplifying noise** — Audio normalization now lifts genuinely soft speech up to ~32× (previously capped at 10×) so quiet dictation transcribes reliably — but only when the clip is modulated like real speech (a per-clip dynamic-range check). Steady noise (fans/HVAC) and pure tones stay at the conservative cap, so they are never amplified into spurious words.

### Pill & feedback

- **Recording pill state model** — The pill now has three distinct states: listening (with live audio-level dots), transcribing (soft pulse), and formatting (soft pulse with formatting indicator). Users can visually distinguish which phase the recording is in.
- **Actionable pill feedback overlay** — Permission failures, recording errors, and paste failures now show a short cause message plus a remediation suggestion (e.g. "Enable Microphone access in System Settings" or "Update the API key in Models") in the pill overlay instead of a generic error or internal string.
- **Pill recreate command for recovery** — A `recreate_pill_widget` command allows the recording pill to be rebuilt if the overlay window becomes unresponsive.

### Bug reporting & diagnostics

- **Full system specs in bug reports** — Bug reports and crash reports now include a System table with OS name/version, kernel, architecture, CPU brand and core count, RAM, and (on Windows) DXGI GPU adapter names. Spec collection is best-effort; a spec failure does not block report submission.

### Settings & UI

- **Transcription sources hub (redesigned Models tab)** — The Models tab is now "Transcription sources," grouping local models, cloud providers, and remote devices into a unified view with Active/Routing-to badges, spoken-language control, and a consistent Add/Select/Remove pattern across all source types.
- **Save recordings toggle and UI** — A dedicated setting controls whether recordings are kept after transcription; the copy explains retryability for failed transcriptions and offers "Open folder" when saving is on.
- **V2 application experience refresh** — The dashboard, onboarding, settings, and pill have been redesigned and updated to reflect the V2 feature set, new source types, and updated Enhancements/Formatting layout.

---

## Quality-of-life & polish

### Performance

- **Windows CPU Whisper significantly faster** — OpenMP linking plus a greedy decode profile (`best_of: 1`, no context or timestamps) with per-core thread cap makes Windows CPU transcription meaningfully faster compared to V1, at comparable accuracy.
- **Recording start chime no longer blocks capture** — The startup sound plays asynchronously; audio capture starts immediately, reducing first-word loss especially on Bluetooth headsets.
- **Cloud audio streamed in 64 KiB chunks** — OpenAI and Groq cloud uploads stream the WAV file in 64 KiB chunks rather than loading the entire file into memory before sending.
- **Model status refresh no longer spams logs** — Centralized model-availability fetching cuts repeated per-component model-status polls; the frontend logs a single state update instead of N redundant refreshes.
- **Tray menu build avoids HTTP calls** — The tray menu uses cached remote-server data so building the menu never blocks on network I/O.

### Reliability

- **Audio callback hot path uses bounded channels and recycled buffers** — The CPAL recording callback writes through a bounded channel; if chunks are dropped before WAV finalization, an integrity-failure warning is shown rather than producing a silently corrupt transcript.
- **Stop flow guarded against duplicate invocations** — Watchdog, hotkey, and silence-timeout stop paths all check a single stop-guard flag, preventing a second stop from racing and clobbering an in-progress transcription.
- **Empty/header-only WAV short-circuits early** — An empty WAV (no audio data beyond the header) returns "No audio captured" before normalization or transcription is attempted.
- **Model download deduplication** — Simultaneous download and delete requests for the same model are rejected immediately, preventing corrupt download state.
- **GPU preload skips when not useful** — Manual Whisper preload warms the Vulkan sidecar only when it can help; CPU-only mode and known GPU-unavailable status skip the probe.
- **Mount-safe event-listener cleanup in frontend** — Late `listen()` registrations that resolve after a component has already unmounted immediately unlisten, preventing leaked listeners and duplicate toasts.
- **Intel Mac LAN connectivity fix** — Network Sharing binds to individual IPv4 interface addresses rather than 0.0.0.0, fixing LAN reachability on Intel Macs.
- **AI polish bounded retry** — The AI executor retries exactly once for rate-limit/service-unavailable/network errors, honoring `Retry-After` only within the remaining request budget; partial streaming output is never accepted.
- **Parakeet sidecar protocol hardened** — The sidecar parser reads JSON from both stdout and stderr (CoreML can redirect stdout); non-protocol stderr lines are trace-only; stdout parse failures log byte count only, never raw content.
- **Whisper model checksum verification** — After download or accepting an existing file by exact size, SHA256/SHA1 is verified; a mismatch automatically removes the corrupt file.

### Logging / diagnostics

- **Unified frontend logger** — All frontend components now use a leveled logger (`debug`, `info`, `warn`, `error`) backed by `tauri-plugin-log`; frontend logs land alongside Rust logs in the same log file and are included in bug reports.
- **Third-party log noise filtered** — `whisper_rs`, `cpal`, `rubato`, `hound`, audio level-meter, and HTTP stack debug/info messages are filtered before reaching stdout and log-file targets.
- **Daily rotating log files** — Logs rotate daily with a 10 MB cap; Debug level in dev builds, Info in release builds.
- **Transcript content never logged** — Successful AI polish logs provider name, input/output lengths, and duration — not the dictated text. Provider error bodies are suppressed from logs and UI.
- **Parakeet sidecar stderr banners no longer cause log floods** — CoreML/FluidAudio startup banners that appear on stderr no longer trigger ERROR/WARN parse events or expose raw audio content in logs.
- **Custom OpenAI-compatible provider error bodies suppressed** — Auth failures from Custom endpoints return typed error categories to the UI; raw provider response bodies are never surfaced in the UI or logs.

### UX nits

- **Overlay error messages shortened and action-oriented** — AI key errors, model-not-loaded errors, and cloud failures now show a brief user-facing message with a direct remediation step ("Update the API key in Models") instead of an internal SDK string.
- **ESC cancel hint shown only in toggle mode** — The "Press Esc to cancel" hint in the pill is hidden in push-to-talk mode since releasing the key already cancels.
- **"When enabled…" text hidden when a remote server is active** — The remote server copy does not show local-server guidance when a remote is already selected.
- **Offline remote servers hidden from tray model selector** — The tray menu model list omits remote servers that are not reachable, keeping the list actionable.
- **Recording pill dark-surface styling restored** — The pill overlay regained correct dark background/text contrast that had regressed in a prior build.
- **Model selection UI responsiveness improved** — Selecting a model source responds immediately without waiting for a background operation to complete.
- **Network sharing hides localhost from copyable addresses** — The address list in the sharing card shows only LAN-reachable addresses; failed bind addresses appear dimmed.
- **Network sharing auto-restarts on local model change** — If the host's shared local model changes, the sharing server restarts with the new model and toasts the update.
- **Save Recordings simplified to a single dropdown** — The retention UI is a single drop-down rather than a multi-step toggle flow.
- **Dashboard Settings live-updates on tray changes** — Changing the sharing status via the tray menu is reflected in the Settings panel without requiring a page refresh.
- **True transcription count shown beyond 500** — History shows the real item count even when it exceeds 500 (previous hard-coded cap caused display confusion).
- **Re-transcribe shows active source name** — "Re-transcribing with Whisper medium…" confirms which engine will process the retry.
- **Duplicate shortcut detection at save time** — The Shortcuts editor blocks saving a binding that conflicts with an existing one and names the action already using that shortcut.
- **History empty state context-aware** — The empty-history view distinguishes "ready to record" from "Accessibility permission needed" and points to the appropriate action.
- **Translation failures clearly labeled** — History rows for translation-required failures are marked "untranslated" so raw text is never mistaken for a successful translation.
- **Accessibility remediation inline in General Settings** — When Accessibility is missing, the General tab shows the fix in context rather than only in the pill overlay.
- **Provider error bodies not leaked on Test failure** — The Custom endpoint Test action returns a typed category (invalid API key, network, etc.) without exposing provider auth-failure response bodies.

### Privacy

- **Clipboard previous content restored after paste** — When "Keep transcription in clipboard" is off, the previous clipboard content is restored 500 ms after paste — but only if the transcript is still there; rapid back-to-back dictations carry the original clipboard forward. If another app writes to the clipboard during the 500 ms window, VoiceTypr does not overwrite it.
- **Pre-AI raw transcript stored locally only** — The original (pre-formatting) transcript saved alongside formatted history entries never leaves the device and is not included in any log or report.
- **Clipboard insertion serialized** — A global insertion guard prevents duplicate paste and stale-restore races between back-to-back recordings.
- **Whisper context sanitized before transcription** — Compiled Whisper prompt context is sanitized to remove characters that could corrupt the decode context.

---

## Under the hood (internal — not user-facing)

- **`keytrigger` crate** — New first-party Rust crate providing a cross-platform observation-only keyboard event engine (macOS CGEventTap, Windows WH_KEYBOARD_LL), a deterministic state-machine matcher, and an example binary; drives the native shortcut backend without consuming key events.
- **Shared transcription executor and request DTOs** — A new `transcription/` module provides `TranscriptionRequest`, `TranscriptionResult`, `TranscriptionExecutor`, `TranscriptionError`, and `TranscriptionCapabilities` so all local/cloud/desktop paths share one set of types and one cancel/timeout policy.
- **AI provider contract and catalog** — A new `ai/` module provides `contract.rs` (stable DTO boundary), `catalog.rs` (generated models.dev catalog parser), `executor.rs` (reliability policy), `error.rs` (typed error taxonomy), `genai_runtime.rs` (native adapter), and `openai_compatible.rs` (custom endpoint runtime). The old per-provider modules (`anthropic.rs`, `gemini.rs`, `openai.rs`, `config.rs`) are removed.
- **Pi/Node formatting sidecar removed** — The bundled Pi formatting sidecar (`a0b3521`) is removed in `057bc6d`; the Rust-native executor is the only AI formatting runtime.
- **`writing.rs` pipeline** — A new `writing.rs` module (3,728 lines) encapsulates the full writing pipeline: settings schema, app rules, context policy, deterministic cleanup/library/voice-command stages, smart formatting, AI failure fallback, and translation-failure behavior.
- **`cloud_stt/` module** — New `cloud_stt/mod.rs`, `common.rs`, `soniox.rs`, `openai.rs`, `groq.rs`, `deepgram.rs`, `cohere.rs` provide the cloud STT provider seam with shared HTTP reliability policy, typed errors, diarization dispatch, and provider-specific API shapes.
- **`remote/` module** — New `remote/lifecycle.rs`, `http.rs`, `server.rs`, `client.rs`, `discovery.rs`, `settings.rs`, `transcription.rs`, `model_control.rs` implement the full Network Sharing stack (LAN server, client routing, settings persistence, active-connection tracking, model-control API).
- **`trigger/` module** — New `trigger/dispatch.rs`, `engine_host.rs`, `mapping.rs`, `mod.rs` bridge the keytrigger native engine into the recording dispatch path and manage engine lifecycle.
- **Parakeet FluidAudio upgraded to 0.15.2** — Sidecar dependency bump bringing runtime improvements for local Apple-Silicon transcription.
- **Whisper-vulkan sidecar extracted** — The GPU Whisper path (`whisper-vulkan/src/main.rs`) is a standalone sidecar process with a typed JSON request/response protocol, replaced the embedded GPU path.
- **macOS deployment target set to 14.0** — Native dependencies compile against macOS 14 minimum, aligning with current Tauri requirements.
- **Swift 6.0 / Xcode 16 on macOS CI runners** — Parakeet sidecar build uses Xcode 16 on macOS GitHub Actions runners; `MainActor` isolation fixes applied for Swift 6 concurrency.
- **Comprehensive Rust test suite expansion** — New test modules: `audio_recording_tests.rs`, `remote_client_tests.rs`, `remote_commands_tests.rs`, `remote_http_tests.rs`, `remote_lifecycle_tests.rs`, `remote_server_tests.rs`, `remote_settings_tests.rs`, `remote_transcription_tests.rs`, `shortcut_bindings.rs`, `recording_state_characterization.rs`, `normalizer_tests.rs`, `ai/runtime_tests.rs` (632 lines), plus WireMock coverage for cloud_stt reliability bar. All cargo clippy `--all-targets` lints cleared.
- **Frontend test suite expansion** — New component tests for `RemoteServerCard`, `AddServerModal`, `NetworkSharingCard`, `ShortcutsSection`, `RecentRecordings`, `GeneralSettings.acceleration`, `OnboardingDesktop`, `useModelAvailability`, `useTranscriptionHistory`, `logger`, and `updateService`.
- **CI/build hardening** — Single Windows installer with bundled runtimes; fast-fail on missing Windows bundle inputs; pnpm without action-setup; upgraded GitHub Actions runtimes; Xcode 16 pinned; Windows test-runner manifest embedding for `TaskDialogIndirect`; CI `run-tests.ps1` script; local CI scripts (`ci-local-macos.sh`, `ci-local-windows.ps1`); quality gate check script.
- **`pnpm-workspace.yaml` override location** — `hono` pnpm v11 supply-chain override moved to `pnpm-workspace.yaml` to eliminate a package-manager warning during install.
- **`pnpm tauri:dev` orphan guard** — `scripts/dev-tauri.cjs` reaps stale debug VoiceTypr/parakeet processes (macOS/Linux) before launching the dev config, preventing single-instance ghost conflicts.
- **State machine formalized** — Recording state transitions (`Idle → Starting → Recording → Stopping → Transcribing → Idle` and `Error → Idle`) are encoded explicitly in `state_machine.rs`.
- **Groq, OpenRouter, xAI, DeepSeek, Cohere dropped from AI-polish catalog** — Per-product decision; only OpenAI, Anthropic, Gemini, and Custom remain as production AI-formatting providers.
- **Agent/multi-agent coordination infrastructure** — Multi-agent development protocol (beads → GitHub Issues migration, worktree isolation, `.agent-counter`, AGENTS.md updates, CLAUDE.md coordination docs). Internal tooling only.
- **Gitignore maintenance** — `.worktrees`, `.beads`, `bv-site`, `skills-lock.json`, `.claude/skills`, `.agents`, launch assets added/managed.
- **Documentation / plan files** — Plans 014–029 archived or created; smoke checklists; README updates; V2 roadmap entries; remote transcription design docs; multi-agent instructions.
- **Miscellaneous dependency bumps** — `fluidaudio 0.15.2`, `whisper-vulkan` lockfile, `pnpm-lock.yaml` major overhaul (6,069 net lines), `components.json`, `tsconfig.json`, `vite.config.ts`, `vitest.config.ts` updates.

---

## Coverage ledger

**242 commits enumerated below — Features (A): 64, QoL/Polish (B): 54, Internal (C): 124**

### Commit classification summary

| SHA | Title | Class |
|---|---|---|
| 4620d76 | docs(smoke): list pending live smokes for 028/029 + normalizer gain | C |
| f4533ea | feat(audio): speech-gated quiet-clip gain with adaptive modulation gate | A |
| b35b640 | feat(stt): inject personal dictionary into OpenAI/Groq/Deepgram recognition | A |
| 0fd435a | feat(formatting): clarify AI-polish pipeline, rewrite prompt, drop app-hint | A |
| 66a3b84 | chore: gitignore stray agent-harness tooling | C |
| 6668699 | fix(hotkeys): ignore self-injected paste events in the keytrigger engine | B |
| 577c5de | docs: changelog + v2 roadmap plans (027 formatting/UX, 028 latency/streaming) | C |
| 467df63 | fix(hooks): mount-safe event-listener cleanup | B |
| fca4de3 | feat(history): surface pre-AI raw transcript in Recent Recordings | A |
| 43e7e4b | refactor(recording): clipboard paste rework + start_recording flow + tests | B |
| 77a70d0 | fix(transcription): WAV-header fallback when Parakeet reports duration=0 | B |
| d8ffeba | refactor(ai): remove standalone OpenAI provider module | C |
| 99b82b7 | chore(logging): filter third-party noise + demote tray/sidecar logs | B |
| ed1fe82 | fix(hotkeys): correct tap-to-toggle inversion; collapse editor to one inline control | B |
| e252ae9 | feat(hotkeys): single-tap-to-toggle + simplified shortcut editor | A |
| 3afa39a | chore(dev): guard tauri:dev against orphaned single-instance ghosts | B |
| 8b0638b | feat(logging): unify frontend logging + cut model-status log spam | B |
| ca8e390 | feat(onboarding): flexible recording hotkeys, model warmup/delete + reliability fixes | A |
| 3db5035 | docs(smoke): mark native-trigger smoke as optional post-2.0.0 | C |
| 46c070d | docs(plan-022): P2 complete; native-trigger smoke steps | C |
| 0c8da06 | feat(shortcuts): native modifier-hold + double-tap triggers (plan 022 P2) | A |
| 23474d9 | docs(plan-022): record P1 done + precise remaining P2/P3 steps | C |
| e582a4e | refactor(shortcuts): extract dispatch_action for engine reuse (P2 prep) | C |
| 24491db | feat(keytrigger): native cross-platform key-trigger engine crate (P1) | A |
| a523803 | docs: native-engine roadmap entry + changelog/smoke for overlay errors & connection count | C |
| 111eca2 | fix(remote): real active-connection count in network sharing | A |
| 840ada3 | fix(transcription): short, actionable overlay error messages | A |
| 0c726e7 | docs: changelog + smoke for single-key shortcuts on any action | C |
| f96743d | feat(shortcuts): allow single-key shortcuts on any action (safe keys, max 5) | A |
| 33edef5 | docs: changelog + smoke + roadmap for acceleration choice and shortcuts wave | C |
| 26bc20e | feat(shortcuts): add 'Toggle AI formatting' bindable action | A |
| 451d396 | fix(shortcuts): surface single-key push-to-talk + clarify recording screens | B |
| 6358305 | feat(transcription): surface Windows GPU/CPU acceleration choice | A |
| 78c8125 | docs: broaden 2.0.0 changelog with major V2 features | C |
| 4c821be | docs: changelog entries for V2 feature track (W1-W5) | C |
| 6b1f6f5 | feat: actionable error remediation in pill feedback (plan 026, Wave 5) | A |
| 665784f | feat: CLI --json consistency + structured output (plan 025, Wave 4) | A |
| 59cfb04 | feat: rich filterable transcription history (plan 024, Wave 3) | A |
| 3e6d7be | feat: cloud speaker diarization for uploads (plan 023, Wave 2) | A |
| 217236c | feat: add source + word-level fields to TranscriptionResult (plan 021, W0) | A |
| 82039e8 | feat: save uploaded transcript to .txt/.md (plan 022, Wave 1) | A |
| 676773f | docs: add 021 V2 feature-track roadmap + carryover | C |
| ee996fc | fix: address PR #79 review findings | B |
| 0a13073 | fix(macos): compile native deps at deployment target 14.0 | C |
| 3153441 | fix(parakeet): isolate stdout-redirect closure to MainActor (Swift 6.0) | C |
| 3ae8ffc | fix(build): surface full swift sidecar build error + exit status | C |
| f256513 | ci: select Xcode 16 (Swift 6) on macOS runners for Parakeet sidecar | C |
| 0642cd9 | Merge origin/main (v1.13.0) into V2; resolve V2-base-of-truth, bump to 2.0.0 | C |
| 0b27e82 | test(port): lock watchdog re-arm, silence routing, media default, system specs | C |
| 9bd46ca | fix(models): verify downloads by exact size + pinned-revision SHA256 | A |
| 52408b6 | docs(smoke): add manual-smoke checklist for the main hotfix-line ports | C |
| a22d65c | perf(whisper): faster Windows/CPU transcription (openmp + CPU fast decode profile) | A |
| c35ada9 | fix(models): warm the Windows Vulkan sidecar on manual model preload | B |
| 139f1be | fix(models): keep model management responsive during downloads; dedupe download errors | B |
| 6315022 | feat(recording): non-destructive silence warnings + sustained-voice gating | A |
| de3cb9f | fix(recorder): recover stuck/finished recorder workers; never-lose-speech on recorder errors | A |
| f0b36f9 | fix(hotkeys): prevent repeated toggle-stop on a held / auto-repeating key | B |
| 274a2fb | fix(media): pause under-reporting players, single GSMTC enumeration, default pause off | B |
| 5f74f73 | fix(parakeet): redirect native sidecar stdout to protect the JSON line protocol | B |
| 2b6728a | feat(bug-report): include full system specs (OS/CPU/RAM/GPU) | A |
| d4b39d2 | fix(whisper): don't poison GPU-acceleration status when cancel/timeout aborts Vulkan sidecar | B |
| 54b7c50 | docs(plans): add failure-preservation smoke checklist (FP-S1..S4) | C |
| b93a739 | feat(transcription): preserve recording + retryable history row on failure | A |
| b71e4ca | docs(plans): mark plan 020 Stage 2 NEEDS-SMOKE + add hot-path smoke checklist | C |
| 6ac9b00 | feat(transcription): route desktop local/cloud recording through the shared executor | A |
| ff67bed | docs(plans): add plan 020 (transcription contract Stage 2) + claim board row | C |
| 0e436bd | feat(history): mark translation-failed rows as untranslated | A |
| 3873450 | fix(writing): fail truthfully when translation required and AI cleanup fails | B |
| dc2731a | fix(transcription): address Stage 1 review findings | C |
| 0943306 | docs(014): mark Stage 1 DONE, archive plan | C |
| fac6977 | feat(transcription): shared contract Stage 1 — DTOs + delegating executor | C |
| 4df8c64 | docs(014): plan + claim shared transcription contract Stage 1 | C |
| e018daa | test: clear all cargo clippy --all-targets lints in test code | C |
| 0b638a3 | test(stt): wiremock coverage for cloud_stt reliability bar (plan 019) | C |
| 93c0ab6 | chore(ai): drop Groq + OpenRouter from AI-polish catalog; retire plan 018 | C |
| 9dfef2e | chore(ai): drop xAI/DeepSeek/Cohere from AI-polish catalog (per user) | C |
| 423fd5a | feat(ai): generated provider catalog + searchable breadth UI + experimental graduation | A |
| 20165d6 | docs(plans): claim 017 + 018 (IN PROGRESS) | C |
| 382b5eb | feat(stt): curated cloud STT providers via data-driven cloud_stt seam (plan 019) | A |
| a2a29b0 | docs(plans): record AI-polish vs cloud-STT domain contract + 019 pre-merge bar | C |
| ef27b9e | docs(plans): archive closed plans, add concurrency protocol + consolidated smoke checklist | C |
| fb09a61 | fix(ai): resolve triple-check audit findings post-sidecar-removal | B |
| 057bc6d | refactor(ai): remove Pi formatting sidecar — Rust-native polish is the only runtime | C |
| 8308082 | docs(plans): mark 016 in progress — steps 1-6 done, step 7 gated on manual smoke | C |
| 66344b8 | feat(ai): rust-native polish cutover — validation, executor, migration, fallback | A |
| 3986b45 | feat(ai): VoiceTypr AI provider contract + Rust runtime spike (plan 016 steps 1-2) | C |
| 0ccd733 | docs(plans): split AI polish into 016 rust cutover, 017 catalog breadth, 018 provider graduation | C |
| c9c57a1 | fix: harden AI polish pipeline | B |
| b1a66bf | fix: cut start latency, bound transcription waits, never drop speech | B |
| 080663b | style: refine dashboard and pill feedback | B |
| 9868fdc | fix: harden transcription runtime and quality gates | B |
| 41351bd | fix: harden v2 readiness gaps | B |
| 7b654fa | feat: add flexible shortcut actions | A |
| f294858 | ci: fail fast when windows bundle inputs are missing | C |
| 1342f00 | feat: make voice commands user-editable | A |
| c7ec4be | feat: add parakeet custom vocabulary via ctc rescore | A |
| df8db3f | chore: upgrade fluidaudio to 0.15.2 | C |
| b0927be | feat: advertise remote capabilities and forward transcription context | A |
| 29580a3 | style: apply rustfmt to remote and writing tests | C |
| 04aa41e | feat: add provider capability matrix | A |
| abb22e2 | docs: add v2 deferred items execution plan | C |
| fc28741 | fix: close v2 functional readiness gaps | B |
| 0491bc5 | fix: resolve v2 verification findings | B |
| 0eaccff | feat: add remote model control | A |
| b6bc892 | feat: add app formatting rules | A |
| 39a6253 | feat: add dictation mode foundation | A |
| f3fce7e | feat: send personal context to Soniox | B |
| f39ef67 | chore: lock whisper-vulkan sidecar dependencies | C |
| 9cda82a | chore: leave launch assets untracked | C |
| 71529cf | ci: single windows installer with bundled runtimes | C |
| 6641a16 | refactor: extract windows vulkan into sidecar with cpu fallback | A |
| cc63593 | feat: add whisper-vulkan sidecar crate and gpu process manager | A |
| 93543c5 | chore: revert dirty split-installer patch | C |
| 62b20c0 | fix: require explicit parakeet model version | C |
| 6061225 | fix: sanitize compiled whisper context | B |
| a9713a2 | fix: remap final guard spans deterministically | C |
| 5d632e5 | fix: remap final guard spans after voice commands | B |
| c793365 | fix: tighten deterministic writing safeguards | B |
| c3ea0f0 | feat: add provenance-based final guard | C |
| bb8268b | feat: add deterministic voice commands | A |
| cf303d9 | feat: pass vocabulary context to whisper | A |
| 774001c | refactor: compile writing context by target | C |
| cb394d8 | refactor: track library rule provenance | C |
| 526591d | feat: add mechanical transcript cleanup | A |
| 8acb2ae | refactor: stage writing pipeline | C |
| 9eeedb1 | test: characterize writing pipeline boundaries | C |
| cff5b4c | chore: upgrade whisper bindings | C |
| 4cd1559 | feat: add parakeet download repair and diarization | A |
| 09f4754 | feat: upgrade fluidaudio sidecar | B |
| 36e99fc | feat: streamline retranscription actions | A |
| 8cf9450 | fix: address onboarding and remote transcription feedback | B |
| cf8276b | feat: refresh V2 app experience | A |
| 4d666c0 | fix: keep Anthropic provider available | B |
| 54ffea4 | chore: track V2 UI support modules | C |
| a0b3521 | feat: add V2 formatting sidecar foundation | C |
| 588f86f | ci: pin windows runner and opt into node24 actions | C |
| bc583c4 | ci: install pnpm without action-setup | C |
| bd98734 | ci: upgrade GitHub action runtimes | C |
| c4901a3 | ci: remove Claude review and fix strict clippy | C |
| ad23e99 | feat: implement VoiceTypr V2 foundations | A |
| c2dbe00 | docs: remove superseded v2 planning notes | C |
| 3565a65 | feat: integrate network sharing into v2 roadmap | C |
| 07cc1a0 | fix(remote): close merge blockers for remote transcription | B |
| e2f12d6 | fix(ui): align onboarding and source readiness truth | B |
| a29a032 | fix(remote): align restore state and source routing | B |
| 178f9b2 | fix(remote): restore truthful readiness and license gating | B |
| 875bccf | fix: resolve all 22 P1/P2 issues from network sharing PR review | B |
| da4ea99 | Merge remote-tracking branch 'origin/main' into feature/network-sharing-remote-transcription | C |
| aaec64e | fix: remote transcription UX and error handling improvements | B |
| 61ef76a | feat: preserve recordings on remote transcription failure | A |
| 65819ea | fix: improve model selection UI responsiveness | B |
| f7a2180 | docs: update network sharing screenshot with new binding UI | C |
| 5349882 | fix: hide localhost from network sharing UI | B |
| 0153d3c | Merge fix/intel-mac-lan-connectivity: Intel Mac LAN fixes and graceful binding | C |
| 60c874b | feat: add graceful IP binding with UI status feedback | A |
| 5047b1e | fix: bind server to individual IPs instead of 0.0.0.0 on Intel Macs | B |
| b7cc5d7 | fix: resolve LAN connectivity issues on Intel Macs | B |
| 5c96b66 | docs: update save-recordings plan with implementation status | C |
| 96839c9 | docs: update API documentation to match implementation | C |
| 65f325d | docs: update remote transcription plan with completed features | C |
| 3874116 | fix: support Parakeet engine for remote transcription | B |
| 3ad8c6f | docs: add screenshots for network sharing and re-transcribe features | C |
| 987534b | fix: re-transcribe dropdown and model ordering consistency | B |
| 88382ff | fix: resolve unused variable warnings in test files | C |
| 76cd1ac | fix: hide offline remote servers from tray menu model selector | B |
| 713e459 | feat: add in-place re-transcription for history items | A |
| bc236fe | feat: add re-transcription with audio playback for history items | A |
| 46d43f6 | refactor: simplify Save Recordings UI to single dropdown | B |
| b2bba0d | test: add real audio integration tests with GPU transcription | C |
| 569fa03 | fix: only show ESC cancel hint in toggle mode | B |
| c86c7ab | refactor: move ESC cancel notice inline with recording mode description | B |
| cbe8669 | feat: add save recordings toggle for re-transcription support (closes #11) | A |
| 7c5004f | Revert "chore: add .beads/ to gitignore" | C |
| bf3b2c9 | fix: hide 'When enabled...' text when remote server is active (closes #10) | B |
| 3f5454c | chore: add .beads/ to gitignore | C |
| 7b5155f | test: fix parallel request test script and audio file | C |
| 8f16c00 | feat: add re-transcribe button and dropdown to history (closes #9) | A |
| 105194d | revert: remove .beads and bv-site from gitignore | C |
| aa1f9e0 | chore: add .beads/ and bv-site/ to gitignore | C |
| efe9c88 | test: add parallel request test script and move test audio | C |
| e85b587 | feat: support file upload transcription via remote server (closes #8) | A |
| dbebfc3 | test: add test audio file for manual testing | C |
| 27fb801 | fix: show true transcription count beyond 500 (closes #7) | B |
| e2c86a7 | docs: add Windows build requirements documentation | C |
| 475b22d | fix: make integration tests cross-platform (macOS + Windows) | C |
| 8022e3a | fix: Dashboard Settings live-updates when tray menu changes sharing status (closes #6) | B |
| a7f44bd | test: add concurrent local + remote transcription tests (refs #3) | C |
| 6922cd3 | fix: correct private module import in tray_menu_tests | C |
| fb884a2 | test: add concurrent transcription tests (closes #3) | C |
| 19f0162 | feat: implement Windows firewall detection (closes #5) | A |
| 20460cb | test: add comprehensive rapid sequential requests tests (refs #2) | C |
| 2c9adac | test: add comprehensive tests for RemoteServerCard component (closes #20) | C |
| abddd56 | test: add AddServerModal component tests (closes #19) | C |
| ba20db9 | test: add comprehensive GeneralSettings component tests (closes #23) | C |
| 8f69615 | test: add comprehensive NetworkSharingCard component tests (closes #18) | C |
| 9aa3c77 | test: add tray menu remote server integration tests (closes #24) | C |
| ba1f12e | fix: remove overflowing literal in port validation test | C |
| 128081c | test: add audio recording tests (closes #21) | C |
| 89b3d40 | test: add comprehensive settings commands tests (closes #22) | C |
| cd98b24 | test: add comprehensive remote commands tests (closes #17) | C |
| c6e6f7f | docs: add mandatory sync step before starting each new task | C |
| 917be4d | test: add comprehensive HTTP server tests (closes #13) | C |
| 374dbe0 | fix: add Windows test runner script to fix TaskDialogIndirect error (closes #25) | C |
| a4fe526 | test: add comprehensive remote transcription module tests (closes #16) | C |
| 003a3d5 | docs: clarify worktree creation with origin/ prefix | C |
| 654e153 | docs: improve multi-agent coordination documentation | C |
| 43e1690 | test: add comprehensive remote server lifecycle tests (closes #14) | C |
| 0861a11 | docs: add git pull instructions to multi-agent protocol | C |
| 10391a3 | test: add comprehensive remote settings tests (closes #15) | C |
| bb68f84 | test: add comprehensive edge case tests for remote client module (closes #12) | C |
| 0047a2b | docs: update agent protocol to commit and close issues autonomously | C |
| 032ab26 | docs: remove hardcoded branch references from agent instructions | C |
| 0374a35 | docs: simplify multi-agent coordination with .agent-counter file | C |
| b7fefca | docs: add multi-agent coordination protocol | C |
| 872f91d | chore: exclude worktrees from vitest and clean up gitignore | C |
| e0a901f | feat: add accessibility permission polling from custom-build | A |
| fca7fe8 | chore: migrate from beads to GitHub Issues | C |
| 5050439 | chore(beads): sync issues and add .gitignore for local files | C |
| e934a32 | bd sync: 2026-01-15 18:25:11 | C |
| 00f8e34 | docs: add updated AGENTS.md with beads workflow guidance | C |
| 6af8d81 | feat: Network Sharing and Remote Transcription | A |
| b1b8552 | feat(pill): add recreate_pill_widget command for recovery | A |
| b75201a | fix: add macOS code signing script for network sharing | C |
| 1988951 | fix(ui): consistent selection styling between local models and remote servers | B |
| 7dcbaf9 | feat: add remote servers to tray menu | A |
| a75f371 | feat: implement remote transcription in recording flow | A |
| e6191ba | fix: allow adding remote server without successful connection test | B |
| 2521b04 | fix: platform-specific audio stream cleanup and Vite watch exclusions | C |
| a0146f0 | fix(beads): watch scripts now detect content changes, not just line counts | C |
| 72d4a47 | feat(ui): improve Network Sharing visual grouping and update beads | B |
| cd9c9fe | fix(beads): use BOM-less UTF-8 encoding in PowerShell watch script | C |
| 3e369a5 | docs: add beads watch daemon scripts and multi-agent instructions | C |
| 6888386 | docs: add beads issue creation guidelines to CLAUDE.md | C |
| f6f24ff | docs: add multi-agent collaboration guide and save-recordings design | C |
| 673e0a0 | test(remote): add Level 3 integration tests for remote transcription | C |
| 63fa776 | chore: add .worktrees to gitignore for parallel development | C |
| 04f8734 | feat(frontend): add remote server management to Models section | A |
| b85faf6 | feat(frontend): add Network Sharing card to Settings | A |
| 2324c4d | feat(remote): add backend for remote transcription feature | A |
| e58fb0a | fix(windows): add manifest embedding for test binaries | C |
| e2cd164 | feat(remote): add remote transcription module with TDD tests | A |
| dd1f2f9 | docs: add remote transcription feature design | C |

### Totals by classification

- **A — User-facing features: 65 commits**
- **B — QoL / polish / reliability: 56 commits**
- **C — Internal (arch, refactor, CI, build, test, docs, chore): 118 commits**
- **Total: 239 of 239 commits accounted for**

### Commits that required judgment calls (not straightforwardly feature or internal)

- `9bd46ca fix(models): verify downloads by exact size + pinned-revision SHA256` — classified A because exact-size/checksum verification is a user-facing durability guarantee, not merely an internal fix.
- `de3cb9f fix(recorder): recover stuck/finished recorder workers; never-lose-speech on recorder errors` — classified A because it exposes a qualitatively new behavior (transcribing audio that survived a recorder device error) rather than fixing an existing path.
- `111eca2 fix(remote): real active-connection count in network sharing` — classified A because it replaces always-0 with an accurate count, which is meaningful UI information.
- `840ada3 fix(transcription): short, actionable overlay error messages` — classified A because actionable error messages are a distinct user-facing capability (part of plan 026).
- `3565a65 feat: integrate network sharing into v2 roadmap` — classified C; despite the `feat` prefix it is a docs/planning commit with no code change.
- `fac6977 feat(transcription): shared contract Stage 1 — DTOs + delegating executor` — classified C; this is an architectural foundation commit (internal DTOs) with no user-visible behavior change on its own.
- `3986b45 feat(ai): VoiceTypr AI provider contract + Rust runtime spike` — classified C; it's a spike/architecture commit, with user-visible behavior delivered by the subsequent cutover commit (`66344b8`).
- `a0b3521 feat: add V2 formatting sidecar foundation` — classified C; the sidecar was later removed in `057bc6d` and never shipped to users.
- `09f4754 feat: upgrade fluidaudio sidecar` — classified B (dependency update with runtime reliability improvement for Parakeet users rather than a new capability).

No commits could not be classified with reasonable confidence given available commit messages, file lists, and recon reports.
