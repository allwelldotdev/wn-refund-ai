"use client";

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";

import { cn } from "@/lib/cn";

import { IconButton } from "./Button";
import { Icon, type IconName } from "./Icon";

type ToastTone = "success" | "info" | "warning" | "error";
type ToastInput = { tone?: ToastTone; title: string; body?: string };
type ToastItem = ToastInput & { id: number };

const TONES: Record<ToastTone, { icon: IconName; color: string }> = {
  success: { icon: "check-circle", color: "text-approved-icon" },
  info: { icon: "info-circle", color: "text-info-icon" },
  warning: { icon: "warning", color: "text-escalated-icon" },
  error: { icon: "alert-circle", color: "text-denied-icon" },
};

const ToastContext = createContext<(t: ToastInput) => void>(() => {});

export function useToast() {
  return useContext(ToastContext);
}

/**
 * Bottom-right stack, newest on top, at most three. Success/info/warning
 * dismiss after 6 s (paused while hovered or focused); errors stay.
 */
export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const next = useRef(1);

  const push = useCallback((t: ToastInput) => {
    const id = next.current++;
    setToasts((all) => [{ ...t, id }, ...all].slice(0, 3));
  }, []);
  const dismiss = useCallback((id: number) => setToasts((all) => all.filter((t) => t.id !== id)), []);

  const value = useMemo(() => push, [push]);
  return (
    <ToastContext.Provider value={value}>
      {children}
      <div
        className={cn(
          "pointer-events-none fixed z-60 flex flex-col gap-3",
          "right-4 bottom-[calc(76px+env(safe-area-inset-bottom))] left-4 md:bottom-6 md:left-auto md:right-6 md:w-[420px]",
        )}
      >
        {toasts.map((t) => (
          <ToastView key={t.id} toast={t} onDismiss={() => dismiss(t.id)} />
        ))}
      </div>
    </ToastContext.Provider>
  );
}

function ToastView({ toast, onDismiss }: { toast: ToastItem; onDismiss: () => void }) {
  const tone = toast.tone ?? "success";
  const [paused, setPaused] = useState(false);
  useEffect(() => {
    if (tone === "error" || paused) return;
    const timer = window.setTimeout(onDismiss, 6000);
    return () => window.clearTimeout(timer);
  }, [tone, paused, onDismiss]);
  const t = TONES[tone];
  return (
    <div
      role={tone === "error" ? "alert" : "status"}
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
      onFocus={() => setPaused(true)}
      onBlur={() => setPaused(false)}
      className="pointer-events-auto flex animate-wn-in items-start gap-3 rounded-lg border border-border bg-surface py-3 pr-2 pl-4 shadow-md"
    >
      <Icon name={t.icon} size={20} strokeWidth={2.25} className={cn("mt-px shrink-0", t.color)} />
      <div className="flex min-w-0 flex-grow flex-col gap-0.5 py-0.5">
        <p className="text-body-sm font-semibold">{toast.title}</p>
        {toast.body ? <p className="text-meta text-ink-muted">{toast.body}</p> : null}
      </div>
      <IconButton icon="close" label="Dismiss notification" size={32} onClick={onDismiss} className="text-ink-subtle" />
    </div>
  );
}
