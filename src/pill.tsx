import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./pill.css";

type BackendRecordingState =
  | "idle"
  | "starting"
  | "recording"
  | "stopping"
  | "transcribing"
  | "error";

type PillState = "idle" | "listening" | "transcribing" | "formatting";
type VisibleState = PillState | "error";
type PillIndicatorMode = "never" | "always" | "when_recording";

interface SettingsPayload {
  pill_indicator_mode?: PillIndicatorMode;
}

interface RecordingStatePayload {
  state: BackendRecordingState;
  error: string | null;
}

interface TauriEvent<T> {
  payload: T;
}

type UnlistenFn = () => void;
type ListenFn = <T>(event: string, handler: (event: TauriEvent<T>) => void) => Promise<UnlistenFn>;
type InvokeFn = <T = unknown>(command: string) => Promise<T>;

export interface RecordingPillController {
  destroy: () => void;
}

interface RecordingPillDeps {
  invoke?: InvokeFn;
  listen?: ListenFn;
}

interface PillDom {
  root: HTMLDivElement;
  surface: HTMLDivElement;
  idle: HTMLDivElement;
  bars: HTMLDivElement;
  barSpans: HTMLSpanElement[];
  listening: HTMLDivElement;
  timer: HTMLSpanElement;
  cancel: HTMLButtonElement;
  transcribing: HTMLDivElement;
  transcribingPrimary: HTMLSpanElement;
  transcribingSecondary: HTMLSpanElement;
  formatting: HTMLDivElement;
  formattingPrimary: HTMLSpanElement;
  formattingSecondary: HTMLSpanElement;
  error: HTMLDivElement;
  errorPrimary: HTMLSpanElement;
  errorSecondary: HTMLSpanElement;
}

const ERROR_FLASH_MS = 1500;
const LEVEL_BAR_COUNT = 9;
const LEVEL_ENVELOPE = Array.from({ length: LEVEL_BAR_COUNT }, (_, index) => {
  const center = (LEVEL_BAR_COUNT - 1) / 2;
  const distance = Math.abs(index - center) / center;
  return 0.42 + (1 - distance * distance) * 0.58;
});

function normalizeMode(mode: SettingsPayload["pill_indicator_mode"]): PillIndicatorMode {
  if (mode === "never" || mode === "always" || mode === "when_recording") {
    return mode;
  }
  return "when_recording";
}

function formatElapsed(seconds: number) {
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  return `${minutes}:${remainingSeconds.toString().padStart(2, "0")}`;
}

function stateFromBackend(state: BackendRecordingState): PillState {
  if (state === "recording" || state === "starting") return "listening";
  if (state === "stopping" || state === "transcribing") return "transcribing";
  return "idle";
}

function createEl<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
): HTMLElementTagNameMap[K] {
  const element = document.createElement(tag);
  if (className) element.className = className;
  return element;
}

function createTextPair(primaryText: string) {
  const wrapper = createEl("div", "pill-text-pair");
  const primary = createEl("span", "pill-text-primary");
  const secondary = createEl("span", "pill-text-secondary");
  primary.textContent = primaryText;
  secondary.textContent = "";
  wrapper.append(primary, secondary);
  return { wrapper, primary, secondary };
}

function createSpinner() {
  const spinner = createEl("span", "pill-spinner");
  spinner.setAttribute("aria-hidden", "true");
  return spinner;
}

function createPillDom(rootElement: HTMLElement): PillDom {
  const root = createEl("div", "pill-root");
  const surface = createEl("div", "pill-surface");

  const idle = createEl("div", "pill-dots");
  idle.setAttribute("aria-label", "Recording idle");
  idle.dataset.testid = "pill-dots";
  idle.append(createEl("span"), createEl("span"), createEl("span"));

  const bars = createEl("div", "pill-bars");
  bars.dataset.testid = "pill-bars";
  const barSpans = LEVEL_ENVELOPE.map(() => {
    const bar = createEl("span", "pill-bar");
    bars.append(bar);
    return bar;
  });

  const listening = createEl("div", "pill-status pill-status-listening");
  const timer = createEl("span", "pill-timer");
  timer.setAttribute("aria-label", "Recording elapsed time");
  const cancel = createEl("button", "pill-cancel");
  cancel.type = "button";
  cancel.setAttribute("aria-label", "Cancel recording");
  cancel.textContent = "×";
  listening.append(bars, timer, cancel);

  const transcribing = createEl("div", "pill-status pill-status-label");
  transcribing.setAttribute("role", "status");
  const transcribingText = createTextPair("Transcribing…");
  transcribing.append(createSpinner(), transcribingText.wrapper);

  const formatting = createEl("div", "pill-status pill-status-label");
  formatting.setAttribute("role", "status");
  const formattingText = createTextPair("Polishing…");
  formatting.append(createSpinner(), formattingText.wrapper);

  const error = createEl("div", "pill-status pill-status-error");
  error.setAttribute("role", "status");
  const errorText = createTextPair("");
  error.append(errorText.wrapper);

  surface.append(idle, listening, transcribing, formatting, error);
  root.append(surface);
  rootElement.replaceChildren(root);

  return {
    root,
    surface,
    idle,
    bars,
    barSpans,
    listening,
    timer,
    cancel,
    transcribing,
    transcribingPrimary: transcribingText.primary,
    transcribingSecondary: transcribingText.secondary,
    formatting,
    formattingPrimary: formattingText.primary,
    formattingSecondary: formattingText.secondary,
    error,
    errorPrimary: errorText.primary,
    errorSecondary: errorText.secondary,
  };
}

function setHidden(element: HTMLElement, hidden: boolean) {
  element.hidden = hidden;
}

function setBars(dom: PillDom, level: number, state: PillState) {
  const clampedLevel = Math.max(0, Math.min(1, level));
  dom.bars.dataset.state = state;

  dom.barSpans.forEach((bar, index) => {
    const envelope = LEVEL_ENVELOPE[index] ?? 0.42;
    const scale =
      state === "listening"
        ? 0.22 + clampedLevel * envelope * 0.78
        : 0.2 + envelope * 0.12;
    bar.style.transform = `scaleY(${scale.toFixed(3)})`;
  });
}

export function createRecordingPill(
  rootElement: HTMLElement,
  deps: RecordingPillDeps = {},
): RecordingPillController {
  const tauriInvoke = deps.invoke ?? invoke;
  const tauriListen = deps.listen ?? listen;
  const dom = createPillDom(rootElement);

  let mode: PillIndicatorMode = "when_recording";
  let pillState: PillState = "idle";
  let isFormatting = false;
  let audioLevel = 0;
  let elapsedSeconds = 0;
  let errorMessage: string | null = null;
  let isCancelling = false;
  let isDestroyed = false;
  let errorTimeout: ReturnType<typeof setTimeout> | undefined;
  let timerInterval: ReturnType<typeof setInterval> | undefined;
  let audioUnlisten: UnlistenFn | undefined;
  const unlisteners: UnlistenFn[] = [];

  const visibleState = (): VisibleState =>
    errorMessage ? "error" : isFormatting ? "formatting" : pillState;

  const isVisible = (state: VisibleState) =>
    errorMessage !== null || mode === "always" || (mode === "when_recording" && state !== "idle");

  const render = () => {
    if (isDestroyed) return;

    const state = visibleState();
    const visible = isVisible(state);
    dom.root.dataset.state = state;
    dom.root.dataset.visible = String(visible);
    setHidden(dom.surface, !visible);

    setHidden(dom.idle, state !== "idle");
    setHidden(dom.listening, state !== "listening");
    setHidden(dom.transcribing, state !== "transcribing");
    setHidden(dom.formatting, state !== "formatting");
    setHidden(dom.error, state !== "error");

    dom.timer.textContent = formatElapsed(elapsedSeconds);
    dom.cancel.disabled = isCancelling;
    dom.errorPrimary.textContent = errorMessage ?? "";
    setBars(dom, state === "listening" ? audioLevel : 0, state === "listening" ? "listening" : "idle");
  };

  const stopAudioListener = () => {
    audioUnlisten?.();
    audioUnlisten = undefined;
  };

  const startAudioListener = () => {
    if (audioUnlisten) return;

    void tauriListen<number>("audio-level", (event) => {
      if (isDestroyed || visibleState() !== "listening") return;
      audioLevel = event.payload;
      render();
    }).then((unlisten) => {
      if (isDestroyed || visibleState() !== "listening") {
        unlisten();
      } else {
        audioUnlisten = unlisten;
      }
    });
  };

  const stopTimer = () => {
    if (timerInterval) {
      clearInterval(timerInterval);
      timerInterval = undefined;
    }
  };

  const startTimer = () => {
    if (timerInterval) return;

    timerInterval = setInterval(() => {
      elapsedSeconds += 1;
      render();
    }, 1000);
  };

  const syncListeningEffects = () => {
    if (visibleState() === "listening") {
      startAudioListener();
      startTimer();
      return;
    }

    stopAudioListener();
    stopTimer();
  };

  const resetActiveState = () => {
    isFormatting = false;
    isCancelling = false;
    audioLevel = 0;
    elapsedSeconds = 0;
  };

  const setPillState = (nextState: PillState) => {
    const wasListening = visibleState() === "listening";
    pillState = nextState;
    const nowListening = visibleState() === "listening";
    if (wasListening !== nowListening) syncListeningEffects();
    render();
  };

  const flashError = (message: string) => {
    if (errorTimeout) clearTimeout(errorTimeout);
    errorMessage = message;
    syncListeningEffects();
    render();

    errorTimeout = setTimeout(() => {
      errorMessage = null;
      errorTimeout = undefined;
      syncListeningEffects();
      render();
    }, ERROR_FLASH_MS);
  };

  const readSettings = async () => {
    try {
      const settings = await tauriInvoke<SettingsPayload>("get_settings");
      if (isDestroyed) return;
      mode = normalizeMode(settings.pill_indicator_mode);
    } catch {
      if (isDestroyed) return;
      mode = "when_recording";
    }
    render();
  };

  const subscribe = <T,>(event: string, handler: (event: TauriEvent<T>) => void) => {
    void tauriListen<T>(event, handler).then((unlisten) => {
      if (isDestroyed) {
        unlisten();
      } else {
        unlisteners.push(unlisten);
      }
    });
  };

  dom.cancel.addEventListener("click", () => {
    if (isCancelling) return;
    isCancelling = true;
    render();
    void tauriInvoke("cancel_recording").catch(() => {
      if (isDestroyed) return;
      isCancelling = false;
      render();
    });
  });

  subscribe("settings-changed", () => {
    void readSettings();
  });

  subscribe<RecordingStatePayload>("recording-state-changed", (event) => {
    if (isDestroyed) return;
    resetActiveState();
    setPillState(stateFromBackend(event.payload.state));

    if (event.payload.state === "error") {
      flashError(event.payload.error || "Recording failed");
    }
  });

  subscribe("recording-started", () => {
    if (isDestroyed) return;
    resetActiveState();
    setPillState("listening");
  });

  subscribe("transcription-started", () => {
    if (isDestroyed) return;
    resetActiveState();
    setPillState("transcribing");
  });

  subscribe("enhancing-started", () => {
    if (isDestroyed) return;
    isFormatting = true;
    syncListeningEffects();
    render();
  });

  subscribe("enhancing-completed", () => {
    if (isDestroyed) return;
    isFormatting = false;
    syncListeningEffects();
    render();
  });

  subscribe("enhancing-failed", () => {
    if (isDestroyed) return;
    isFormatting = false;
    syncListeningEffects();
    render();
  });

  subscribe<string>("recording-too-short", (event) => {
    if (isDestroyed) return;
    resetActiveState();
    setPillState("idle");
    flashError(event.payload || "Recording too short");
  });

  render();
  void Promise.resolve().then(readSettings);

  return {
    destroy: () => {
      isDestroyed = true;
      if (errorTimeout) clearTimeout(errorTimeout);
      stopTimer();
      stopAudioListener();
      unlisteners.forEach((unlisten) => unlisten());
      rootElement.replaceChildren();
    },
  };
}

const root = document.getElementById("root");

if (root) {
  createRecordingPill(root);
}
