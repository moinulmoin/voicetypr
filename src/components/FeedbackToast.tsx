import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";

export type PillToastAction = "show" | "clear";
export type PillToastVariant = "info" | "warning";

export interface PillToastPayload {
  id: number;
  message: string;
  duration_ms: number;
  action?: PillToastAction;
  variant?: PillToastVariant;
  persistent?: boolean;
  suggestion?: string;
}

type ToastSeverity = "info" | "success" | "error" | "warning";

interface ActiveToast {
  id: number;
  message: string;
  severity: ToastSeverity;
  suggestion?: string;
}

const SEVERITY_TREATMENT: Record<ToastSeverity, string> = {
  error: "before:bg-rose-500/80",
  info: "before:bg-sky-500/65",
  success: "before:bg-emerald-500/75",
  warning: "before:bg-amber-500/80",
};

function inferSeverity(message: string): ToastSeverity {
  const normalized = message.toLowerCase();
  if (
    normalized.includes("error") ||
    normalized.includes("fail") ||
    normalized.includes("unable") ||
    normalized.includes("couldn't")
  ) {
    return "error";
  }
  if (
    normalized.includes("complete") ||
    normalized.includes("copied") ||
    normalized.includes("ready") ||
    normalized.includes("saved")
  ) {
    return "success";
  }
  return "info";
}

function severityForPayload({ message, variant }: PillToastPayload): ToastSeverity {
  if (variant === "warning") return "warning";
  if (variant === "info") return "info";
  return inferSeverity(message);
}

export function FeedbackToast() {
  const [toast, setToast] = useState<ActiveToast | null>(null);
  const latestIdRef = useRef(Number.NEGATIVE_INFINITY);
  const timerRef = useRef<number | null>(null);

  const clearTimer = useCallback(() => {
    if (!timerRef.current) return;
    clearTimeout(timerRef.current);
    timerRef.current = null;
  }, []);

  const applyPayload = useCallback(
    (payload: PillToastPayload) => {
      const action = payload.action ?? "show";

      if (action === "clear") {
        if (payload.id !== latestIdRef.current) return;
        clearTimer();
        setToast(null);
        return;
      }

      if (payload.id < latestIdRef.current) return;

      latestIdRef.current = payload.id;
      clearTimer();
      setToast({
        id: payload.id,
        message: payload.message,
        severity: severityForPayload(payload),
        suggestion: payload.suggestion,
      });

      if (payload.persistent === true) return;

      timerRef.current = window.setTimeout(() => {
        if (latestIdRef.current !== payload.id) return;
        setToast(null);
        timerRef.current = null;
      }, payload.duration_ms);
    },
    [clearTimer]
  );

  useEffect(() => {
    let isMounted = true;
    let unlistenFn: (() => void) | undefined;

    void listen<PillToastPayload>("toast", (evt) => {
      if (isMounted) applyPayload(evt.payload);
    }).then((unlisten) => {
      if (!isMounted) {
        unlisten();
        return;
      }
      unlistenFn = unlisten;
    });

    return () => {
      isMounted = false;
      unlistenFn?.();
      clearTimer();
    };
  }, [applyPayload, clearTimer]);

  if (!toast) {
    return null;
  }

  return (
    <div className="pointer-events-none fixed inset-0 z-50 flex items-center justify-center px-4">
      <div
        aria-live="polite"
        className={`relative max-w-[min(420px,calc(100vw-2rem))] rounded-2xl border border-white/55 bg-white/90 px-3.5 py-2.5 pl-4 text-[13px] leading-snug text-neutral-800 shadow-[0_16px_45px_rgba(15,23,42,0.14)] ring-1 ring-neutral-950/10 backdrop-blur-xl before:absolute before:left-2 before:top-1/2 before:h-1.5 before:w-1.5 before:-translate-y-1/2 before:rounded-full dark:border-white/10 dark:bg-neutral-900/80 dark:text-neutral-100 dark:shadow-black/35 dark:ring-white/10 ${SEVERITY_TREATMENT[toast.severity]}`}
        role="status"
      >
        <span className="block overflow-hidden break-words [display:-webkit-box] [-webkit-box-orient:vertical] [-webkit-line-clamp:2]">
          {toast.message}
        </span>
        {toast.suggestion && (
          <span className="mt-0.5 block text-xs opacity-70 overflow-hidden break-words [display:-webkit-box] [-webkit-box-orient:vertical] [-webkit-line-clamp:2] text-neutral-500 dark:text-neutral-400">
            {toast.suggestion}
          </span>
        )}
      </div>
    </div>
  );
}
