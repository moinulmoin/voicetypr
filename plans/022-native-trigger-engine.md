# Plan 022 — Native key-trigger engine (cross-platform, replaces `global_shortcut`)

**Status:** DESIGN v2 — oracle-reviewed (`agent://OracleDesignReview`), BLOCKER/SHOULD-FIX deltas folded in. Ready to implement P1. Post-2.0.0, owner-confirmed 2026-06-17.
**Supersedes:** the deferred W6 hotkey-model merge (plans/021:70) and the `global_shortcut` exclusivity/consume limits.

---

## 1. Why

Tauri `global_shortcut` has three root limits we cannot work around inside it:

1. **No bare modifier-only / double-tap triggers** ("hold Right-Option to talk", "double-tap Cmd"). *Primary need.*
2. **Binding any key hijacks it from typing** (it registers system-wide and *consumes*). The 2.0.0 single-key allowlist+cap (`f96743d`) is a stopgap around exactly this.
3. **Exclusive registration** fails when a hotkey is taken. Owner wants permissive: never block, observe + fire, overlaps are the user's to resolve.

**End-state:** one engine owns all triggers; `global_shortcut` removed; the legacy primary-hotkey model and per-action `shortcut_bindings` unify into one binding model (completes W6). Phased rollout; single-system replacement is the target.

---

## 2. Goals / Non-goals (v1)

**Goals**
- Cross-platform (macOS + Windows) detection of: **modifier-only hold** (left/right side), **double-tap** (key or modifier), **chord** (modifier(s)+key), **single key**.
- **Observation-only / non-exclusive**: never consume; never fail to register; overlaps all fire.
- Robust long-running: survive macOS tap-timeout disable; survive session transitions (sleep/wake, lock, fast-user-switch); start only after Accessibility granted; never leave a hold stuck "on".
- Drop into the existing dispatch via an **engine adapter** (not the tauri-`Shortcut`-shaped struct); no change to the **14** `ShortcutAction`s.
- Standalone, reusable, MIT, open-sourceable crate (`keytrigger`) with a platform-independent, exhaustively unit-tested matcher.

**Non-goals (v1)**
- **Consuming/suppressing** keys (needs an active tap) — opt-in per binding, P3+.
- Mouse, gestures, leader/sequence keys.
- Linux (backend trait leaves room; no Linux backend ships).
- Per-app/context triggers; macOS Globe/Fn as a trigger; multi-keyboard device-ID routing.

---

## 3. Architectural ground truth (do NOT re-derive)

- Handler `lib.rs:235-242` registers `global_shortcut` → `recording/hotkeys.rs:18 handle_global_shortcut(app,&Shortcut,ShortcutState)` → `shortcuts::matching_custom_binding` (Shortcut equality over `AppState.custom_shortcut_bindings`, shortcuts.rs:291-305) → `dispatch_custom_shortcut(..)` (hotkeys.rs:309, **private**, matches all actions) → else primary/PTT gate → else `handle_non_recording_shortcut` (ESC).
- **Reusable pure fns (keep verbatim):** `shortcuts.rs:85 pressed_shortcut_should_run(&mut HashSet<String>, id, ShortcutState)->bool`; `shortcuts.rs:99 hold_shortcut_transition(..)->CustomHoldTransition{Start,Stop,Noop}`. ToggleRecording has a special Released path clearing `toggle_key_held` (hotkeys.rs:136-146, 298-307) — must be preserved.
- Models (shortcuts.rs:19-76): **`ShortcutAction` has 14 variants** {ToggleRecording, HoldToRecord, CancelRecording, CopyLastTranscription, PasteLastTranscription, CycleFormattingMode, **ToggleAiFormatting**, Set{PersonalDictation,CleanDictation,Writing,Notes,Message,Code}, OpenDashboard}; `ShortcutTrigger{Pressed,Hold}`; `ShortcutBinding{id,action,shortcut:String,trigger,enabled,allow_risky_combo}`; `ShortcutSettings{bindings:Vec<_>}` (single serde JSON blob → serde-default fields are migration-safe); `RegisteredShortcutBinding{id,action,trigger,shortcut: tauri Shortcut}` (legacy-only).
- Custom settings register every enabled binding with `global_shortcut` (shortcuts.rs:145-184, 204-289, 523-558) and validation **rejects modifier-only shortcuts** (shortcuts.rs:617-680) — P2 must skip engine trigger-kinds here.
- ESC: registered only while recording (`global_shortcut().register("Escape")`, audio.rs:3843-3864), unregistered on stop/cancel (audio.rs:3971-3980, 6165-6175); handler needs **double-ESC within 2s, ignores Released** (escape_handler.rs:6-24,41-61).
- Permissions: `commands/permissions.rs:5-103` `request/check_accessibility_permission`, `open_accessibility_settings`; emits `accessibility-granted`/`accessibility-denied`; frontend `useAccessibilityPermission.ts:15-80` + onboarding + `text.rs` paste gate. **Engine must subscribe backend-side** (frontend state alone insufficient). Windows: no accessibility model; LL hook needs none (UIPI: a non-elevated process can't hook elevated windows — same as today).
- Frontend capture `HotkeyInput.tsx:52-90` uses DOM `e.metaKey/ctrlKey/shiftKey/altKey` booleans + filters modifier names → **cannot express side-specific / bare-modifier / double-tap** → native triggers need a picker.

---

## 4. Foundation — raw OS taps, NOT `rdev` (source-verified)

`rdev 0.5.3` (dep at Cargo.toml:40, used only for `simulate`/paste) is unsuitable as a listener: blocks forever, single `static mut GLOBAL_CALLBACK`, no stop/restart, silently no-ops on macOS until Accessibility granted with no restart (breaks onboarding grant), and never re-enables a timeout-disabled tap. Keep it for `simulate` only; reference its MIT keycode tables.

**macOS — `core-graphics 0.24`, zero hand-declared FFI for v1** (verified `event.rs`):
- `CGEventTap::new(HID, kCGHeadInsertEventTap, CGEventTapOptions::ListenOnly, [KeyDown,KeyUp,FlagsChanged], cb)` (event.rs:477-505); `tap.mach_port.create_runloop_source(0)` + `run_current()` (event.rs:445-467); `tap.enable()` wraps `CGEventTapEnable` (event.rs:514-516); `CGEventType::TapDisabledByTimeout/ByUserInput` (event.rs:139-142); `KEYBOARD_EVENT_AUTOREPEAT` + `KEYBOARD_EVENT_KEYCODE` + `get_flags`/`get_integer_value_field` (event.rs:185-192, 635-663). `core-foundation 0.10` `CFRunLoop` is Send/Sync with `get_current/run_current/stop` (runloop.rs:28-30,40-59,79-82).
- Thread model: run the CFRunLoop on a **dedicated thread we own**, retain the `CFRunLoop` handle so `stop()` (another thread) calls `CFRunLoopStop`.
- **BLOCKER — re-enable pattern (callback gets `CGEventTapProxy`, NOT the `CFMachPort`):** `tap.enable()` inside its own callback is self-referential. Resolution: the tap callback closure **captures `Arc<OnceCell<SendableMachPort>>`**; after `CGEventTap::new` returns, we store the tap's mach-port into that cell. On a `TapDisabledByTimeout/ByUserInput` event the callback re-enables directly via the stored port (`CGEventTapEnable(port, true)` — cheap, thread-safe). Fallback if the wrapper hides the raw port: callback forwards a `Control::ReEnable` over the channel to the owner thread (holds the `CGEventTap`) which calls `tap.enable()`. Primary = captured-cell.

**Windows — `windows 0.62`, `WH_KEYBOARD_LL`:**
- `SetWindowsHookExW(WH_KEYBOARD_LL, proc, hinst, 0)`. The hook proc is delivered to the **installing thread's message loop**, so install + `GetMessageW` pump must be the **same thread**. Hook proc passes through always (`CallNextHookEx`) and only forwards a tiny `RawKeyEvent` over the channel.
- **BLOCKER — clean start/stop handshake:** the stop thread's `PostThreadMessageW(WM_QUIT)` is dropped if the hook thread hasn't created its message queue yet. Hook thread sequence: (1) `PeekMessageW(PM_NOREMOVE)` to force queue creation, (2) signal **ready** (a `mpsc`/`Event`), (3) `SetWindowsHookExW`, (4) `GetMessageW` loop. `stop()` posts `WM_QUIT` **only after ready**; on loop exit the hook thread calls `UnhookWindowsHookEx`, then `start()`/`stop()` joins it. All hook-lifetime state lives on the pump thread (no cross-thread races with an in-flight callback).
- **SHOULD-FIX — no timeout-disabled event:** Windows silently drops a LL hook whose callback exceeds `LowLevelHooksTimeout` (no notification on Win7+). The tiny-callback rule mitigates; add a health check (watchdog detects "no events for N s while expected" or thread death) → **restart the hook thread**. There is no macOS-style re-enable event here.

Net new third-party surface: **zero** (both crates already vendored).

---

## 5. The `keytrigger` crate

Workspace member `src-tauri/crates/keytrigger/` (path dep of `voicetypr`, no `tauri` dep, MIT). Deps: `core-graphics`+`core-foundation` (macOS), `windows` (Windows), `log`, `thiserror`. Channel: `std::sync::mpsc`.

### 5.1 Public API
```rust
pub struct TriggerEngine { /* control-channel sender + backend/dispatcher thread handles */ }
pub type TriggerId = String;                 // caller-supplied (= binding id)
pub enum Modifier { Alt, Control, Meta, Shift }     // Meta = Cmd(mac)/Win
pub enum Side { Left, Right, Either }
pub enum KeySpec { Named(NamedKey), Raw(u32) }      // physical; Raw round-trips unknown
pub enum Trigger {
    ModifierHold { modifier: Modifier, side: Side },          // PRIMARY
    DoubleTap    { key: TapKey, within: Duration },           // TapKey = Key(KeySpec) | Mod(Modifier, Side)
    Chord        { mods: ModSet, key: KeySpec },
    SingleKey    { key: KeySpec },                            // bare only (no modifiers held)
}
pub enum KeyPhase { Pressed, Released }
pub struct TriggerEvent { pub id: TriggerId, pub phase: KeyPhase }
impl TriggerEngine {
    pub fn new() -> Self;
    pub fn start(&self, on_event: impl Fn(TriggerEvent) + Send + 'static) -> Result<(), EngineError>;
    pub fn stop(&self);              // synthesizes Released for active triggers (see §6.6) then tears down
    pub fn is_running(&self) -> bool;
    pub fn set_bindings(&self, bindings: Vec<(TriggerId, Trigger)>);  // via control channel; diff + synth-release on dispatcher thread
}
```

### 5.2 Internals — two threads, no locks in the OS callback
```
OS tap/hook thread  --RawKeyEvent / Control over mpsc-->  dispatcher thread { Matcher + on_event(catch_unwind) }
```
- `trait KeyEventSource { fn run(self, tx: Sender<Msg>, ready: ReadySignal); fn request_stop(&self); }` impls `MacEventTap`, `WinKeyboardHook`; `MockSource` for tests.
- `enum Msg { Raw(RawKeyEvent), Control(Control) }`; `enum Control { ReEnable, SetBindings(Vec<(TriggerId,Trigger)>), Stop }`.
- `RawKeyEvent { key: KeySpec, side: Option<Side>, down: bool, is_repeat: bool, t: Instant }` — **normalized by the backend**; modifier *state* is tracked by the matcher, never trusted from an OS aggregate (Windows has none; macOS aggregate can't disambiguate sides).
- **Hot-swap concurrency:** `set_bindings` sends `Control::SetBindings`; the dispatcher applies it **in order with events** (computes removed/changed active bindings, emits synthetic `Released` for them, then swaps). No `Mutex` is ever taken on the OS callback path; the callback only forwards.
- Callback discipline (non-negotiable): OS callback does only event-normalize + channel send + (macOS) re-enable on disable. Matcher + `on_event` run on the dispatcher thread, `on_event` wrapped in `catch_unwind`.

---

## 6. Matcher semantics & edge cases (the bulk of correctness — exhaustively unit-tested)

Matcher state: `keys_down: HashSet<KeySpec>` (non-modifier physical keys), `mods_down: HashSet<(Modifier,Side)>` (**tracked per physical modifier**, the single source of truth), `last_tap: HashMap<TapKey,(Instant, bool /*saw_up*/)>`, `active: HashMap<TriggerId, bool>`.

### 6.1 Backend normalization → tracked modifier state
- **macOS:** KeyDown/KeyUp → `RawKeyEvent{down, is_repeat=KEYBOARD_EVENT_AUTOREPEAT}` with `KeySpec` from keycode. **FlagsChanged → per-keycode toggle**: the event carries the *keycode of the modifier that changed*; if that physical modifier is already in `mods_down` it's a **release**, else a **press** (this is the fix for the "both Shifts held, release one, aggregate flag stays set" case — never trust the aggregate flag for side, use the keycode). Right-Option = keycode 61 (physical, not a flag bit); map left/right Shift/Control/Option/Cmd from their keycodes.
- **Windows:** WM_(SYS)KEYDOWN/UP → VK + scancode. **No aggregate snapshot** → derive modifier press/release from VK_L/R{SHIFT,CONTROL,MENU,WIN} down/up and maintain `mods_down`. A KEYDOWN for a key already in `keys_down`/`mods_down` = **repeat** (Windows has no explicit flag).

### 6.2 ModifierHold{modifier, side}
On a modifier press transition matching (modifier, side|Either) → `Pressed`; on its release transition → `Released`. Does NOT require absence of other keys (holding Option while a chord also fires is allowed — non-exclusive). Powers hold-Right-Option-to-talk.

### 6.3 Chord{mods, key}
**Re-evaluated on EVERY raw event**: active iff `key ∈ keys_down` AND `mods ⊆ mods_down` (side-agnostic). Emit `Pressed` on false→true, `Released` on true→false. Re-evaluating every event reconciles a missed `key`-up (chord drops when key leaves down-set) and modifier changes. (Deep desync — a missed modifier-up while key still held — is covered by §6.7 reconciliation.)

### 6.4 SingleKey{key}
`key` (non-modifier) down with `mods_down` **empty** → `Pressed`; up → `Released`. **Shift+F8 does NOT fire an F8 SingleKey** (bare-only; modified variants are Chords). Observation-only: the key still performs its normal OS function.

### 6.5 DoubleTap{TapKey, within}
On a press of the TapKey (for `Mod`, a FlagsChanged/VK modifier press; for `Key`, a KeyDown), if a previous press of the *same* TapKey occurred within `within` AND a release was seen in between (`saw_up`) AND the current press is not a repeat → emit `Pressed` then `Released` **same-tick** (one-shot; no separate `Fired` phase — keeps the existing pressed/toggle state machine intact). Reset the tap record after firing. For `Mod`, side semantics follow `Side` (Left/Right/Either); Left-Cmd then Right-Cmd is NOT a double-tap unless `Either`.

### 6.6 Reset / set_bindings / stop ordering (BLOCKER fix)
Before clearing any `active`/`keys_down`/`mods_down` or swapping bindings, **synthesize `Released` for every currently-active `TriggerId` and dispatch it through the normal `on_event` path** so VoiceTypr's pressed/hold/toggle AppState clears (no stuck recording). `set_bindings` diff: for bindings removed or whose `Trigger` changed while active, emit synthetic `Released` first, then apply.

### 6.7 Session/desync reconciliation (SHOULD-FIX)
- On (re)start, tap re-enable, and focus/session events: reset `keys_down`/`mods_down`/`active` (after synthesizing Released per §6.6).
- **Active-trigger reconciliation:** while any trigger is active, periodically (and on each event) reconcile `mods_down` against OS truth — macOS `CGEventSourceFlagsState`, Windows `GetAsyncKeyState` — and release any hold/chord whose physical state cleared unseen. This bounds the "missed modifier-up while key held" window.
- **Sleep/wake, lock/unlock, fast-user-switch:** reset matcher AND **recreate the tap/hook** (not merely `enable()`) — a disabled-callback re-enable does not prove the mach port/source survived a session transition. macOS: subscribe to `NSWorkspace` sleep/wake (or recreate defensively on first event after a gap); Windows: restart hook thread on health-check failure.

### 6.8 Secure Input (macOS, SHOULD-FIX)
While any hold is active, poll `IsSecureEventInputEnabled`; if it flips true (a password field grabbed input — taps stop delivering), synthesize release-all so the mic never stays stuck. Document as the chosen mitigation (vs. accepting stuck-recording with only a manual cancel).

### 6.9 Determinism
Matcher is a pure function of (normalized event stream, bindings, injected clock). Tests feed `RawKeyEvent`/`Control` sequences with a mock clock and assert the exact `TriggerEvent` sequence for every case above.

---

## 7. Permission & lifecycle

- **macOS:** `start()` only when `check_accessibility_permission` is true (tap creation fails / silently no-ops otherwise). Boot: granted → start; else dormant. **Backend** subscribes to `accessibility-granted` → start (and on revocation cues, stop). Reuse `useAccessibilityPermission` UI; no new permission surface.
- **Windows:** `start()` at boot (no permission). UIPI limitation documented (= today).
- Engine = single `Arc<TriggerEngine>` in `AppState` (one per process, enforced). `set_bindings` called from `register_saved_shortcuts` and every `update_shortcut_settings`.

---

## 8. VoiceTypr integration

- New module `src-tauri/src/trigger/`: `engine_host.rs` (owns engine, lifecycle, accessibility subscription), `mapping.rs` (`ShortcutBinding`→`Trigger`), `dispatch.rs` (`on_event`→action).
- **Seam fix (BLOCKER):** `dispatch_custom_shortcut` is private + `RegisteredShortcutBinding` is tauri-`Shortcut`-shaped (no Shortcut for ModifierHold/DoubleTap). Introduce `EngineBinding{ id, action: ShortcutAction, trigger: ShortcutTrigger }` and a `pub(crate) fn dispatch_engine_event(app,&AppState,&EngineBinding, KeyPhase→ShortcutState)` that reuses `pressed_shortcut_should_run`/`hold_shortcut_transition` and the action match arms (refactor the match body out of `dispatch_custom_shortcut` into a shared `pub(crate)` `dispatch_action(app,&AppState,action,trigger,id,state)`; legacy path keeps its Shortcut lookup). Engine bindings live in their own `AppState` vec + active sets; legacy `Shortcut`-matching stays isolated.
- **Schema additions** (serde-default, blob-persisted): `#[serde(default)] trigger_kind: TriggerKind` (`Combo` default == today | `ModifierHold` | `DoubleTap` | `SingleKey`); `#[serde(default)] modifier: Option<ModifierSpec{modifier,side}>`. Default `Combo` keeps every existing binding byte-identical. Frontend `src/types/shortcuts.ts:19-30` mirrors it. Backend serde-default round-trip test.
- **ESC:** keep on `global_shortcut` through P2 (it's recording-scoped + double-ESC); migrate to engine `SingleKey{Escape}` in P3. Observation-only ESC reaches the focused app (see §10 tradeoff).

## 9. Recorder UX (frontend)

`ShortcutsSection` gains a **picker**: trigger-kind select (Combo / Hold a modifier / Double-tap) → ModifierHold shows modifier+side (Left/Right Option, Control, Command, Shift); DoubleTap shows key/modifier + interval; Combo/SingleKey keep `HotkeyInput`. Picker writes structured fields. No backend capture mode in v1 (picker is deterministic; "capture next" deferred). Available on macOS + Windows via `@/lib/platform`.

---

## 10. Phases & acceptance

### P1 — `keytrigger` crate
Scaffold + workspace wiring; full API; pure `Matcher` (§6); `MacEventTap` (ListenOnly, captured-port re-enable, CFRunLoop stop, reconciliation hooks); `WinKeyboardHook` (ready-handshake, unhook, restart-on-failure); `examples/print_triggers.rs`.
- **Acceptance:** builds on macOS, compiles for Windows in CI; matcher unit tests cover §6.1-6.7 (hold, chord-every-event, single-bare, double-tap key+modifier, autorepeat, reset/set_bindings synth-release ordering, reconciliation); macOS example prints Pressed/Released for hold-Right-Option and double-tap-Cmd and survives an induced tap-timeout (re-enables). No VoiceTypr behavior change.

### P2 — additive integration (ship the primary need) — **transitional**
`trigger/` host+mapping+dispatch; engine in AppState; backend accessibility-grant subscription; `EngineBinding` seam; schema additions + routing so **ModifierHold/DoubleTap never enter the `global_shortcut` registration vector and Combo/SingleKey never enter engine bindings** (routing tests required). Picker UI. New availability: HoldToRecord via ModifierHold (Right-Option), ToggleRecording via DoubleTap.
- **Transitional limitation (state explicitly):** Combo/SingleKey remain `global_shortcut`/exclusive until P3; P2 does NOT deliver full non-exclusive behavior for those kinds.
- **Acceptance (macOS smoke):** hold Right-Option → record starts on press / stops on release, Option still types; double-tap Cmd → toggles and does NOT stick; existing Combo/single-key unchanged; no double-fire; revoke+regrant accessibility → engine recovers; sleep/wake → no stuck recording. Windows: compiles in CI; manual smoke deferred. Gates + reviewer clean.

### P3 — unify & retire `global_shortcut`
Migrate Combo + SingleKey + primary `hotkey`/`ptt_hotkey` to the engine (one model, W6); remove `tauri-plugin-global-shortcut` (Cargo.toml, lib.rs plugin+handler, register/unregister sites, `Shortcut`/`ShortcutState`); migrate ESC to engine; settings migration folds legacy primary hotkey into a `shortcut_bindings` entry.
- Optional opt-in **consume** per binding — **only here**: the `core-graphics` wrapper maps callback `None`→pass-through (event.rs:437-442), so suppression needs `CGEventTapOptions::Default` + a raw callback returning NULL (the single place we add raw FFI; ListenOnly v1 needs none).
- Reconsider the single-key allowlist/cap (observation-only may relax it).
- **Acceptance:** `global_shortcut` grep-clean; all 14 actions work via the engine; legacy bindings auto-migrate; full SMOKE matrix (macOS) + Windows manual smoke; gates + reviewer clean.

---

## 11. Testing strategy
- **Matcher**: exhaustive pure unit tests (mock clock + scripted events), the primary correctness gate.
- **Backends**: thin; macOS example harness (real taps) + Windows manual smoke; keep platform code minimal.
- **Integration**: Rust tests for `mapping.rs` and `dispatch_action`/`dispatch_engine_event` (reuse the characterized hold/pressed fns, `tests/shortcut_bindings.rs`); **P2 routing tests** (trigger-kind never crosses systems); ToggleRecording-double-tap-doesn't-stick test; serde-default schema round-trip.
- No OS mocks; `MockSource` keeps matcher tests OS-free.

## 12. Risks & mitigations
- macOS tap timeout-disabled → captured-port re-enable in callback + matcher synth-Released on reset (R: stuck mic).
- Windows LL hook silently dropped on callback timeout (no event) → tiny callback + health watchdog → restart hook thread.
- Missed events / side desync → matcher tracks modifier state per physical key (never trusts aggregate) + §6.7 reconciliation via `CGEventSourceFlagsState`/`GetAsyncKeyState` + reset-on-(re)start.
- Callback latency / FFI panic → callback only forwards; matcher+`on_event` on dispatcher thread, `catch_unwind`.
- Secure Input (mac) → poll `IsSecureEventInputEnabled` while hold active → release-all.
- Session transitions → recreate tap/hook + reset matcher.
- P2 double-fire → strict trigger_kind routing + tests.
- Windows UIPI (no elevated-window hooking) → documented, = today.
- Single instance → one `TriggerEngine`/process, enforced in host.

## 13. Open questions — RESOLVED (oracle)
1. **macOS FFI** — RESOLVED: `core-graphics 0.24` exposes the full listen-only path, zero hand-declared FFI for v1; re-enable via captured mach-port cell (§4). Raw FFI only for P3 consume.
2. **ESC consume** — RESOLVED: observation-only for v1/P2 (matches non-exclusive principle; double-ESC + ignore-Released already guards accidental cancel). Document ESC also reaches the focused app; revisit consume in P3 only if double-ESC UX proves unacceptable.
3. **P2 vs single cutover** — RESOLVED: keep P2 dual-system, tightened with routing tests; single cutover would replace primary+PTT+custom+ESC+schema+backends at once (hardware bugs unisolable). P2 ships the primary need with smaller blast radius; mark Combo/SingleKey legacy/exclusive until P3.
4. **DoubleTap phase** — RESOLVED: one-shot `Pressed`+`Released` same-tick (no `Fired`); the existing pressed/toggle machine needs Released to clear `toggle_key_held`. Test it doesn't stick.
5. **Recorder** — RESOLVED: picker sufficient for v1 (DOM capture can't do side/bare-modifier); native "capture next" deferred.
6. **Schema** — RESOLVED: structured `trigger_kind` + serde-default fields (overloading the Shortcut string fights the validator that rejects modifier-only, shortcuts.rs:617-680); update `src/types/shortcuts.ts` + add serde-default tests.

## 14. Manual hardware matrix (P2/P3 smoke)
External + built-in keyboard; left vs right modifiers; sleep/wake; lock/unlock; secure-input/password field; accessibility revoke→regrant; (Windows) elevated target window; double-tap timing on slow/fast taps; autorepeat under long hold.

## 15. Documented policies / non-goals
Physical-keycode matching (display strings localized/best-effort; `Raw(u32)` round-trips unknown keys); multiple keyboards collapse to one physical-key state (no device IDs in v1); macOS Globe/Fn not a trigger unless added to `Modifier`/`KeySpec`; Windows uses `WH_KEYBOARD_LL` for v1 (Raw Input is a possible later hardening for monitor-only cases).
