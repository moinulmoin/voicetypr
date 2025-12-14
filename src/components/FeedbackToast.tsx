import { listen } from "@tauri-apps/api/event";
import { useEffect, useState, useRef, useCallback } from "react";

interface PillToastPayload {
  id: number;
  message: string;
  duration_ms: number;
}

export function FeedbackToast() {
  const [message, setMessage] = useState<string>("");
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const showMessage = useCallback((text: string, duration: number) => {
    if (timerRef.current) clearTimeout(timerRef.current);

    setMessage(text);

    timerRef.current = setTimeout(() => {
      setMessage("");
      timerRef.current = null;
    }, duration);
  }, []);

  useEffect(() => {
    let isMounted = true;
    let unlistenFn: (() => void) | undefined;

    listen<PillToastPayload>("toast", (evt) => {
      if (!isMounted) return;
      const { message, duration_ms } = evt.payload;
      showMessage(message, duration_ms);
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
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [showMessage]);

  if (!message) {
    return null;
  }

  return (
    <div className="fixed inset-0 flex items-center justify-center">
      <div className="bg-black text-white text-sm px-4 py-2 rounded-lg shadow-lg flex items-start gap-2 min-w-[200px] max-w-[400px]">
        <img src="/AppIcon.png" alt="" className="w-4 h-4 rounded-sm flex-shrink-0 mt-0.5" />
        <span className="text-white/30 flex-shrink-0">|</span>
        <span className="break-words whitespace-pre-wrap">{message}</span>
      </div>
    </div>
  );
}
