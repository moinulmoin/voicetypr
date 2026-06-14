import { listen } from "@tauri-apps/api/event";
import { Info, TriangleAlert } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

export type PillToastAction = "show" | "clear";
export type PillToastVariant = "info" | "warning";

export interface PillToastPayload {
  id: number;
  action: PillToastAction;
  message: string;
  duration_ms: number;
  variant: PillToastVariant;
  persistent: boolean;
}

export interface VisibleToast {
  id: number;
  message: string;
  variant: PillToastVariant;
}

type IncomingToastPayload = Partial<PillToastPayload> & {
  id: number;
  message?: string;
  duration_ms?: number;
};

export function FeedbackToast() {
  const [visibleToast, setVisibleToast] = useState<VisibleToast | null>(null);
  const timerRef = useRef<number | null>(null);
  const latestIdRef = useRef<number>(0);

  const clearScheduledHide = useCallback(() => {
    if (timerRef.current !== null) {
      window.clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const applyPayload = useCallback(
    (raw: IncomingToastPayload) => {
      const payload: PillToastPayload = {
        id: raw.id,
        action: raw.action ?? "show",
        message: raw.message ?? "",
        duration_ms: raw.duration_ms ?? 0,
        variant: raw.variant ?? "info",
        persistent: raw.persistent ?? false,
      };

      if (payload.action === "clear") {
        if (payload.id !== latestIdRef.current) {
          return;
        }
        clearScheduledHide();
        setVisibleToast(null);
        return;
      }

      if (payload.id < latestIdRef.current) {
        return;
      }

      latestIdRef.current = payload.id;
      clearScheduledHide();

      if (!payload.message) {
        setVisibleToast(null);
        return;
      }

      setVisibleToast({
        id: payload.id,
        message: payload.message,
        variant: payload.variant,
      });

      if (!payload.persistent && payload.duration_ms > 0) {
        const toastId = payload.id;
        timerRef.current = window.setTimeout(() => {
          if (latestIdRef.current === toastId) {
            setVisibleToast(null);
          }
          timerRef.current = null;
        }, payload.duration_ms);
      }
    },
    [clearScheduledHide],
  );

  useEffect(() => {
    let isMounted = true;
    let unlistenFn: (() => void) | undefined;

    listen<IncomingToastPayload>("toast", (evt) => {
      if (!isMounted) return;
      applyPayload(evt.payload);
    }).then((unlisten) => {
      if (!isMounted) {
        unlisten();
        return;
      }
      unlistenFn = unlisten;
    });

    return () => {
      isMounted = false;
      if (unlistenFn) unlistenFn();
      clearScheduledHide();
    };
  }, [applyPayload, clearScheduledHide]);

  if (!visibleToast) {
    return null;
  }

  const isWarning = visibleToast.variant === "warning";

  return (
    <div className="fixed inset-0 flex items-center justify-center pointer-events-none">
      <div
        role="status"
        aria-live="polite"
        className={
          isWarning
            ? "text-sm px-4 py-2 rounded-lg shadow-lg ring-1 flex items-start gap-2 min-w-[200px] max-w-[400px] bg-amber-950 text-amber-50 ring-amber-400/40"
            : "bg-black text-white text-sm px-4 py-2 rounded-lg shadow-lg ring-1 ring-white/30 flex items-start gap-2 min-w-[200px] max-w-[400px]"
        }
      >
        {isWarning ? (
          <TriangleAlert
            className="h-4 w-4 text-amber-300 flex-shrink-0 mt-0.5"
            aria-hidden
          />
        ) : (
          <Info
            className="h-4 w-4 text-white/80 flex-shrink-0 mt-0.5"
            aria-hidden
          />
        )}
        <span
          className={
            isWarning ? "text-amber-400/50 flex-shrink-0" : "text-white/30 flex-shrink-0"
          }
        >
          |
        </span>
        <span className="break-words whitespace-pre-wrap">{visibleToast.message}</span>
      </div>
    </div>
  );
}