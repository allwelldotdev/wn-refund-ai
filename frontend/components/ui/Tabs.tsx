"use client";

import { useRef, type KeyboardEvent, type ReactNode } from "react";

import { cn } from "@/lib/cn";

export type TabItem<T extends string> = { value: T; label: ReactNode; count?: number };

type TabsProps<T extends string> = {
  label: string;
  items: TabItem<T>[];
  value: T;
  onChange: (value: T) => void;
  /** Prefix for tab/panel ids: tabs are `${idBase}-tab-${value}`, panels `${idBase}-panel-${value}`. */
  idBase: string;
  className?: string;
};

export function tabPanelProps(idBase: string, value: string) {
  return {
    id: `${idBase}-panel-${value}`,
    role: "tabpanel" as const,
    "aria-labelledby": `${idBase}-tab-${value}`,
    tabIndex: 0,
  };
}

/**
 * Underline tabs with a roving tabindex: arrows move and wrap, Home/End jump,
 * and only the active tab is in the Tab order.
 */
export function Tabs<T extends string>({ label, items, value, onChange, idBase, className }: TabsProps<T>) {
  const refs = useRef<Array<HTMLButtonElement | null>>([]);

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    const i = items.findIndex((t) => t.value === value);
    let next = -1;
    if (e.key === "ArrowRight") next = (i + 1) % items.length;
    else if (e.key === "ArrowLeft") next = (i - 1 + items.length) % items.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = items.length - 1;
    if (next < 0) return;
    e.preventDefault();
    onChange(items[next].value);
    refs.current[next]?.focus();
  }

  return (
    <div
      role="tablist"
      aria-label={label}
      onKeyDown={onKeyDown}
      className={cn("flex gap-1 border-b border-border", className)}
    >
      {items.map((t, i) => {
        const active = t.value === value;
        return (
          <button
            key={t.value}
            ref={(el) => {
              refs.current[i] = el;
            }}
            id={`${idBase}-tab-${t.value}`}
            type="button"
            role="tab"
            aria-selected={active}
            aria-controls={`${idBase}-panel-${t.value}`}
            tabIndex={active ? 0 : -1}
            onClick={() => onChange(t.value)}
            className={cn(
              "-mb-px inline-flex h-11 items-center gap-2 border-b-2 px-3 text-body-sm transition-colors duration-120 ease-standard",
              active ? "border-ink font-semibold text-ink" : "border-transparent font-medium text-ink-muted hover:text-ink",
            )}
          >
            {t.label}
            {t.count !== undefined ? <CountPill active={active}>{t.count}</CountPill> : null}
          </button>
        );
      })}
    </div>
  );
}

export function CountPill({ active, children }: { active?: boolean; children: ReactNode }) {
  return (
    <span
      className={cn(
        "inline-flex h-5 min-w-5 items-center justify-center rounded-full px-1.5 text-caption leading-5 font-medium tabular",
        active ? "bg-ink text-white" : "bg-muted text-ink-muted",
      )}
    >
      {children}
    </span>
  );
}
