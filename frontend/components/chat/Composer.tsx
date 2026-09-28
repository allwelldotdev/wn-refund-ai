"use client";

import { forwardRef, useId, type KeyboardEvent } from "react";

import { Icon } from "@/components/ui/Icon";
import { cn } from "@/lib/cn";

export const SOFT_LIMIT = 500;

type ComposerProps = {
  value: string;
  onChange: (value: string) => void;
  onSend: () => void;
  placeholder: string;
  /** Busy or paused: typing still works, sending does not. */
  blocked: boolean;
  hint: string;
  /** 16px text in the mobile sheet so iOS doesn't zoom. */
  large: boolean;
  /** Screen-reader label for the text box. */
  label?: string;
};

export const Composer = forwardRef<HTMLTextAreaElement, ComposerProps>(function Composer(
  { value, onChange, onSend, placeholder, blocked, hint, large, label = "Describe the problem" },
  ref,
) {
  const uid = useId();
  const n = value.length;
  const over = n > SOFT_LIMIT;
  const canSend = !blocked && !over && value.trim().length > 0;

  function onKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      if (canSend) onSend();
    }
  }

  return (
    <form
      className="flex flex-col gap-1.5 border-t border-border bg-surface px-3 pt-3 pb-[calc(12px+env(safe-area-inset-bottom))] sm:pb-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (canSend) onSend();
      }}
    >
      <label htmlFor={`${uid}-input`} className="sr-only">
        {label}
      </label>
      <div className="flex items-end gap-2">
        <textarea
          ref={ref}
          id={`${uid}-input`}
          rows={2}
          value={value}
          placeholder={placeholder}
          aria-invalid={over || undefined}
          aria-describedby={`${uid}-count ${uid}-hint`}
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={onKeyDown}
          className={cn(
            "max-h-[120px] min-h-11 flex-grow resize-none rounded-md px-3 py-2.5 text-ink placeholder:text-ink-subtle",
            large ? "text-body" : "text-body-sm",
            over ? "border-[1.5px] border-danger" : "border border-border-strong",
            blocked ? "bg-muted" : "bg-surface",
          )}
        />
        <button
          type="submit"
          aria-label="Send message"
          disabled={!canSend}
          className="inline-flex size-11 shrink-0 items-center justify-center rounded-md bg-primary text-white hover:bg-primary-hover disabled:cursor-not-allowed disabled:bg-muted disabled:text-ink-disabled"
        >
          <Icon name="arrow-up" size={18} strokeWidth={2.25} />
        </button>
      </div>
      <div className="flex items-center justify-between gap-3 text-caption">
        <span id={`${uid}-hint`} className="text-ink-subtle">
          {hint}
        </span>
        <span
          id={`${uid}-count`}
          className={cn("tabular", over ? "font-semibold text-denied-fg" : n >= 450 ? "font-semibold text-ink" : "text-ink-subtle")}
        >
          {n} / {SOFT_LIMIT}
          {over ? ` · ${n - SOFT_LIMIT} over` : n >= 450 ? ` · ${SOFT_LIMIT - n} left` : ""}
        </span>
      </div>
    </form>
  );
});
