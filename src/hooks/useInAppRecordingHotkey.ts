import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef } from "react";
import { useSetting } from "@/contexts/SettingsContext";
import { useRecording } from "@/hooks/useRecording";
import { createLogger } from "@/lib/logger";
import { isMacOS } from "@/lib/platform";
import { findActivePrimaryBinding } from "@/lib/shortcut-display";
import { eventMatchesShortcut } from "@/lib/shortcut-event-match";
import type {
  ModifierSpec,
  ShortcutBinding,
  ShortcutSettings,
} from "@/types/shortcuts";

const log = createLogger("in-app-hotkey");

/**
 * Minimum gap between two in-app toggles. Guards against duplicate keydowns
 * (e.g. WebView2/IME quirks) and the rare case where the OS-level hook also
 * fires for the same press.
 */
const TOGGLE_DEBOUNCE_MS = 300;

/** DOM `KeyboardEvent.code` prefix for each bare-modifier kind. */
const MODIFIER_CODE_PREFIX: Record<ModifierSpec["modifier"], string> = {
  control: "Control",
  alt: "Alt",
  meta: "Meta",
  shift: "Shift",
};

/**
 * True when focus is inside an editable element (text input / textarea /
 * contenteditable) — the ONLY context where the OS-level keytrigger hook can be
 * bypassed by the browser/IME (e.g. Ctrl+Space in our own bug-report box).
 * Restricting the fallback to this case avoids double-triggering everywhere the
 * native engine already handles the hotkey.
 */
function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA";
}

/** True when the event's physical key is the configured bare modifier + side. */
function eventMatchesBareModifier(event: KeyboardEvent, spec: ModifierSpec): boolean {
  const code = event.code || "";
  if (!code.startsWith(MODIFIER_CODE_PREFIX[spec.modifier])) return false;
  if (spec.side === "left") return code.endsWith("Left");
  if (spec.side === "right") return code.endsWith("Right");
  return true; // "either"
}

/** True when a modifier OTHER than the configured one is currently held. */
function hasOtherModifierHeld(event: KeyboardEvent, spec: ModifierSpec): boolean {
  return (
    (spec.modifier !== "control" && event.ctrlKey) ||
    (spec.modifier !== "alt" && event.altKey) ||
    (spec.modifier !== "shift" && event.shiftKey) ||
    (spec.modifier !== "meta" && event.metaKey)
  );
}

/**
 * Resolve a bare-modifier primary we can emulate with a DOM tap: it must be an
 * isolated-tap toggle binding (push-to-talk "hold" has different keydown-start /
 * keyup-stop semantics and is intentionally out of scope here).
 */
function tapToggleModifier(binding: ShortcutBinding | null): ModifierSpec | null {
  if (!binding?.modifier) return null;
  if (!binding.enabled) return null;
  if (binding.action !== "toggle_recording") return null;
  if (binding.trigger_kind !== "isolated_tap") return null;
  return binding.modifier;
}

/**
 * In-app fallback for the recording hotkey.
 *
 * The OS-level `keytrigger` engine starts/stops recording globally, but when
 * focus is inside one of our own WebView2 text fields the browser/IME can
 * swallow the trigger before the low-level hook sees it, so the global hotkey
 * silently does nothing. Because the keys still reach the DOM, this hook
 * re-handles them when an editable element is focused:
 *   - combos (e.g. Ctrl+Space) — matched on keydown against `settings.hotkey`;
 *   - bare-modifier isolated-tap (e.g. Control alone) — a clean modifier
 *     keydown→keyup with no other key, matched against the native primary
 *     binding (which leaves `settings.hotkey` empty).
 *
 * Editable-focus gating is mandatory: a tap fires on key-UP while the native
 * hook starts on key-DOWN, so emulating it where the native hook works would
 * start-then-stop. Mount once in the main window (it has the text inputs).
 */
export function useInAppRecordingHotkey(): void {
  const hotkey = useSetting("hotkey");
  const recording = useRecording();
  const lastToggleRef = useRef(0);

  // Active bare-modifier isolated-tap primary. Loaded only on Windows and only
  // when no combo hotkey is set: macOS's native engine handles bare-modifier
  // taps in our own windows, where this keyup-based detector could stutter
  // against it; and a combo primary leaves the bare-modifier binding inactive.
  const bareModifierRef = useRef<ModifierSpec | null>(null);
  // Monotonic load token: only the most recent reload wins, so an in-flight
  // load whose result resolves late can't clobber a fresher binding (e.g. a
  // combo hotkey just set, or a second shortcut-settings-changed event).
  const bareModifierLoadTokenRef = useRef(0);

  /**
   * Re-resolve the active bare-modifier isolated-tap primary from the backend.
   * No-op on macOS and when a combo hotkey is set: macOS's native engine
   * handles bare-modifier taps in our own windows (where this keyup-based
   * detector could stutter against it), and a combo primary leaves the
   * bare-modifier binding inactive. Called on mount and whenever `hotkey`
   * changes, and again when the backend reports shortcut settings changed —
   * so an in-session binding save refreshes the cached spec without an app
   * restart (settings.hotkey alone may not change, since it can already be "").
   */
  const reloadBareModifier = useCallback((currentHotkey: string | undefined): void => {
    if (currentHotkey || isMacOS) {
      bareModifierRef.current = null;
      return;
    }
    const token = ++bareModifierLoadTokenRef.current;
    invoke<ShortcutSettings>("get_shortcut_settings")
      .then((settings) => {
        if (token === bareModifierLoadTokenRef.current) {
          bareModifierRef.current = tapToggleModifier(
            findActivePrimaryBinding(settings.bindings),
          );
        }
      })
      .catch(() => {
        if (token === bareModifierLoadTokenRef.current) {
          bareModifierRef.current = null;
        }
      });
  }, []);

  useEffect(() => {
    reloadBareModifier(hotkey);
  }, [hotkey, reloadBareModifier]);

  // Keep the latest hotkey + recording controls in a ref so the listeners are
  // bound once and never re-attach on state changes. Updated in an effect (not
  // during render) so reads always see the committed values.
  const latest = useRef({ hotkey, recording });
  useEffect(() => {
    latest.current = { hotkey, recording };
  });

  // Backend signals that shortcut settings were persisted (e.g. an in-session
  // bare-modifier binding change). Re-resolve so the fallback picks up the new
  // spec without a restart. Registered once; the handler reads the current
  // hotkey from `latest` so it never closes over a stale combo value.
  useEffect(() => {
    if (isMacOS) return; // native engine handles bare-modifier taps on macOS
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listen("shortcut-settings-changed", () => {
      reloadBareModifier(latest.current.hotkey);
    }).then((unlistenFn) => {
      // listen() resolved after unmount → unlisten right away to avoid a leak;
      // otherwise hold the unlisten fn for cleanup.
      if (cancelled) {
        unlistenFn();
      } else {
        unlisten = unlistenFn;
      }
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [reloadBareModifier]);

  useEffect(() => {
    // A pending lone-modifier tap: the code of the modifier pressed with no
    // other key + the recording state at keydown. Cleared on completion, on any
    // intervening key, or on focus/visibility loss.
    let pendingTap: { code: string; stateAtDown: string } | null = null;

    // Toggle recording, mirroring the native state machine (handle_toggle_mode):
    // act only on settled states, ignore transitional ones, and debounce.
    const performToggle = (reason: string): void => {
      const { recording: currentRecording } = latest.current;
      const now = Date.now();
      if (now - lastToggleRef.current < TOGGLE_DEBOUNCE_MS) return;
      lastToggleRef.current = now;
      const state = currentRecording.state;
      if (state === "idle" || state === "error") {
        log.debug(`In-app ${reason} in editable field — starting recording`);
        void currentRecording.startRecording();
      } else if (state === "recording") {
        log.debug(`In-app ${reason} in editable field — stopping recording`);
        void currentRecording.stopRecording();
      }
    };

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.isComposing) return;
      if (!isEditableTarget(event.target)) return;

      const currentHotkey = latest.current.hotkey;

      // ── Combo path (e.g. Ctrl+Space) ─────────────────────────────────────
      if (currentHotkey) {
        if (event.repeat) return;
        // Only Ctrl/Cmd-class combos (the IME-swallowed class); leave bare keys
        // and Shift/Alt-only combos to the field's normal typing.
        if (!event.ctrlKey && !event.metaKey) return;
        // AltGr is reported as Ctrl+Alt on Windows and produces typed chars.
        if (event.getModifierState?.("AltGraph")) return;
        if (!eventMatchesShortcut(event, currentHotkey)) return;
        event.preventDefault();
        performToggle("hotkey");
        return;
      }

      // ── Bare-modifier isolated-tap path (e.g. Control alone) ─────────────
      const bareModifier = bareModifierRef.current;
      if (!bareModifier) return;

      if (!eventMatchesBareModifier(event, bareModifier)) {
        // Any non-matching key while the modifier is held → it's a chord/combo,
        // not a clean tap (covers Ctrl+C/V and AltGr's synthesized second key).
        pendingTap = null;
        return;
      }
      if (event.repeat) return; // held modifier auto-repeats; keep the pending tap
      if (hasOtherModifierHeld(event, bareModifier)) {
        pendingTap = null;
        return;
      }
      pendingTap = { code: event.code, stateAtDown: latest.current.recording.state };
    };

    const onKeyUp = (event: KeyboardEvent) => {
      const pending = pendingTap;
      if (!pending || event.code !== pending.code) return;
      pendingTap = null;
      if (event.isComposing || event.getModifierState?.("AltGraph")) return;
      if (!isEditableTarget(event.target)) return;
      // Bail if the recording state moved since keydown (e.g. the native hook
      // did fire) — avoids a start→stop stutter.
      if (latest.current.recording.state !== pending.stateAtDown) return;
      performToggle("bare-modifier tap");
    };

    const clearPending = () => {
      pendingTap = null;
    };

    // Capture phase: run before the focused field's own handlers.
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    window.addEventListener("blur", clearPending, true);
    document.addEventListener("visibilitychange", clearPending);

    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
      window.removeEventListener("blur", clearPending, true);
      document.removeEventListener("visibilitychange", clearPending);
    };
  }, []);
}
