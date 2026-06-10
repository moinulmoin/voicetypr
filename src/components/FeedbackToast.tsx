import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";

interface PillToastPayload {
  id: number;
  message: string;
  duration_ms: number;
}

type ToastSeverity = "info" | "success" | "error";

interface ActiveToast {
  id: number;
  message: string;
  severity: ToastSeverity;
}

const SEVERITY_TREATMENT: Record<ToastSeverity, string> = {
  error: "before:bg-rose-500/80",
  info: "before:bg-sky-500/65",
  success: "before:bg-emerald-500/75",
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

export function FeedbackToast() {
  const [toast, setToast] = useState<ActiveToast | null>(null);
  const latestIdRef = useRef(Number.NEGATIVE_INFINITY);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearTimer = useCallback(() => {
    if (!timerRef.current) return;
    clearTimeout(timerRef.current);
    timerRef.current = null;
  }, []);

  const showMessage = useCallback(
    ({ duration_ms, id, message }: PillToastPayload) => {
      if (id < latestIdRef.current) return;

      latestIdRef.current = id;
      clearTimer();
      setToast({ id, message, severity: inferSeverity(message) });

      timerRef.current = setTimeout(() => {
        if (latestIdRef.current !== id) return;
        setToast(null);
        timerRef.current = null;
      }, duration_ms);
    },
    [clearTimer]
  );

  useEffect(() => {
    let isMounted = true;
    let unlistenFn: (() => void) | undefined;

    void listen<PillToastPayload>("toast", (evt) => {
      if (isMounted) showMessage(evt.payload);
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
  }, [clearTimer, showMessage]);

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
      </div>
    </div>
  );
}
