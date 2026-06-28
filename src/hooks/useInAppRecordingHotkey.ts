import { useEffect, useRef } from "react";
import { useSetting } from "@/contexts/SettingsContext";
import { useRecording } from "@/hooks/useRecording";
import { eventMatchesShortcut } from "@/lib/shortcut-event-match";
import { createLogger } from "@/lib/logger";

const log = createLogger("in-app-hotkey");

/**
 * Minimum gap between two in-app toggles. Guards against duplicate keydowns
 * (e.g. WebView2/IME quirks) and the rare case where the OS-level hook also
 * fires for the same press.
 */
const TOGGLE_DEBOUNCE_MS = 300;

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

/**
 * In-app fallback for the recording hotkey.
 *
 * The OS-level `keytrigger` engine starts/stops recording globally, but when
 * focus is inside one of our own WebView2 text fields the browser/IME can
 * swallow the combo (most notably Ctrl+Space, a Windows input-language toggle)
 * before the low-level hook sees it. There the global hotkey silently does
 * nothing. Because the keydown still reaches the DOM, this hook re-derives the
 * configured shortcut from the event and toggles recording directly.
 *
 * Mount once in the main window (it has the text inputs); the pill window has
 * none.
 */
export function useInAppRecordingHotkey(): void {
  const hotkey = useSetting("hotkey");
  const recording = useRecording();
  const lastToggleRef = useRef(0);

  // Keep the latest hotkey + recording controls in a ref so the listener is
  // bound once and never re-attaches on state changes. Updated in an effect
  // (not during render) so reads always see the committed values.
  const latest = useRef({ hotkey, recording });
  useEffect(() => {
    latest.current = { hotkey, recording };
  });

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.repeat) return;
      // Skip IME composition events (Chromium reports keyCode 229 / isComposing
      // mid-composition), which would otherwise spuriously match the hotkey.
      if (event.isComposing) return;
      if (!isEditableTarget(event.target)) return;

      // Only handle Ctrl/Cmd-class combos — the class the OS hook misses via
      // IME inside our own text fields (e.g. Ctrl+Space). Bare keys and
      // Shift/Alt-only combos can produce typed characters (onboarding allows a
      // one-key shortcut), so leave them to the focused field's normal input.
      if (!event.ctrlKey && !event.metaKey) return;

      const { hotkey: currentHotkey, recording: currentRecording } = latest.current;
      if (!currentHotkey || !eventMatchesShortcut(event, currentHotkey)) return;

      const now = Date.now();
      if (now - lastToggleRef.current < TOGGLE_DEBOUNCE_MS) return;
      lastToggleRef.current = now;

      // Stop the combo from also acting on the focused field (e.g. inserting a
      // space) now that we are handling it as the recording hotkey.
      event.preventDefault();

      if (currentRecording.isActive) {
        log.debug("In-app hotkey matched in editable field — stopping recording");
        void currentRecording.stopRecording();
      } else {
        log.debug("In-app hotkey matched in editable field — starting recording");
        void currentRecording.startRecording();
      }
    };

    // Capture phase: run before the focused field's own handlers.
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, []);
}
