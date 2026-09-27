"use client";

import { useEffect, useRef, useState } from "react";

import { cn } from "@/lib/cn";
import { addDays, clampDay, formatDay, formatMonth, formatShortDay, isoDay, parseDay, today } from "@/lib/dates";

import { Icon } from "./Icon";

type DatePickerProps = {
  id: string;
  label: string;
  /** "YYYY-MM-DD" */
  value: string;
  min: string;
  max: string;
  onChange: (day: string) => void;
  help?: string;
  error?: string | null;
  /** Accessible name of the calendar popover, e.g. "Choose order date". */
  dialogLabel: string;
};

const WEEKDAYS = [
  ["Mo", "Monday"],
  ["Tu", "Tuesday"],
  ["We", "Wednesday"],
  ["Th", "Thursday"],
  ["Fr", "Friday"],
  ["Sa", "Saturday"],
  ["Su", "Sunday"],
] as const;

function firstOfMonth(day: string): string {
  return `${day.slice(0, 8)}01`;
}

function addMonths(first: string, n: number): string {
  const d = parseDay(first);
  d.setMonth(d.getMonth() + n, 1);
  return isoDay(d);
}

/** Monday-first weeks covering the month that starts on `first`. */
function weeksOf(first: string): string[][] {
  const start = addDays(first, -((parseDay(first).getDay() + 6) % 7));
  const nextMonth = addMonths(first, 1);
  const weeks: string[][] = [];
  for (let day = start; day < nextMonth || weeks.length === 0; ) {
    const week = Array.from({ length: 7 }, (_, i) => addDays(day, i));
    weeks.push(week);
    day = addDays(day, 7);
  }
  return weeks;
}

/**
 * A date field with a calendar popover: arrows move by a day or a week within
 * the allowed range, Enter or Space picks, Escape closes and returns focus to
 * the field (without closing any dialog around it).
 */
export function DatePicker({ id, label, value, min, max, onChange, help, error, dialogLabel }: DatePickerProps) {
  const [open, setOpen] = useState(false);
  const [focused, setFocused] = useState(value);
  const trigger = useRef<HTMLButtonElement>(null);
  const popover = useRef<HTMLDivElement>(null);
  const grid = useRef<HTMLDivElement>(null);
  const labelId = `${id}-label`;
  const helpId = `${id}-help`;
  const now = today();
  const month = firstOfMonth(focused);

  function show() {
    setFocused(clampDay(value, min, max));
    setOpen(true);
  }

  function close() {
    setOpen(false);
    trigger.current?.focus();
  }

  function pick(day: string) {
    onChange(day);
    close();
  }

  // Escape handled natively on the popover so it stops before a surrounding modal sees it.
  useEffect(() => {
    const el = popover.current;
    if (!open || !el) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      setOpen(false);
      trigger.current?.focus();
    };
    el.addEventListener("keydown", onKey);
    return () => el.removeEventListener("keydown", onKey);
  }, [open]);

  useEffect(() => {
    if (open) grid.current?.querySelector<HTMLButtonElement>(`[data-day="${focused}"]`)?.focus();
  }, [open, focused]);

  function onGridKey(e: React.KeyboardEvent) {
    const step = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 }[e.key];
    if (step === undefined) return;
    e.preventDefault();
    setFocused((d) => clampDay(addDays(d, step), min, max));
  }

  return (
    <div className="relative flex flex-col gap-1.5">
      <span id={labelId} className="text-body-sm font-medium text-ink">
        {label}
      </span>
      <button
        ref={trigger}
        type="button"
        id={id}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-labelledby={`${labelId} ${id}`}
        aria-describedby={helpId}
        onClick={() => (open ? close() : show())}
        className={cn(
          "inline-flex h-10 w-full items-center gap-2 rounded-md bg-surface px-3 text-left text-ink",
          error ? "border-[1.5px] border-denied-icon" : "border border-border-strong",
        )}
      >
        <Icon name="calendar" size={16} className="shrink-0 text-ink-muted" />
        <span className="flex-grow font-mono text-mono tabular">{formatDay(value)}</span>
        <Icon name="chevron-down" size={14} className="shrink-0 text-ink-muted" />
      </button>
      <p id={helpId} className={cn("text-caption", error ? "text-denied-fg" : "text-ink-subtle")}>
        {error ?? help}
      </p>
      {open ? (
        <div
          ref={popover}
          role="dialog"
          aria-modal="false"
          aria-label={dialogLabel}
          className="absolute top-[70px] left-0 z-20 flex w-[300px] max-w-[calc(100vw-48px)] flex-col gap-2 rounded-lg border border-border bg-surface p-3 shadow-lg"
        >
          <div className="flex items-center justify-between">
            <button type="button" aria-label="Previous month" disabled={addDays(month, -1) < min}
              onClick={() => setFocused(clampDay(addMonths(month, -1), min, max))}
              className="inline-flex size-9 items-center justify-center rounded-md text-ink hover:bg-muted disabled:text-ink-disabled">
              <Icon name="chevron-left" size={16} />
            </button>
            <span aria-live="polite" className="text-body-sm font-semibold">
              {formatMonth(month)}
            </span>
            <button type="button" aria-label="Next month" disabled={addMonths(month, 1) > max}
              onClick={() => setFocused(clampDay(addMonths(month, 1), min, max))}
              className="inline-flex size-9 items-center justify-center rounded-md text-ink hover:bg-muted disabled:text-ink-disabled">
              <Icon name="chevron-left" size={16} className="rotate-180" />
            </button>
          </div>
          <div ref={grid} role="grid" aria-label={formatMonth(month)} onKeyDown={onGridKey} className="flex flex-col gap-0.5">
            <div role="row" className="grid grid-cols-7">
              {WEEKDAYS.map(([s, l]) => (
                <span key={s} role="columnheader" aria-label={l} className="py-1 text-center text-caption font-medium text-ink-subtle">
                  {s}
                </span>
              ))}
            </div>
            {weeksOf(month).map((week) => (
              <div key={week[0]} role="row" className="grid grid-cols-7">
                {week.map((day) => {
                  const out = day < min || day > max;
                  const selected = day === value;
                  return (
                    <span key={day} role="gridcell" aria-selected={selected} className="flex justify-center">
                      <button
                        type="button"
                        data-day={day}
                        disabled={out}
                        tabIndex={day === focused ? 0 : -1}
                        aria-label={formatDay(day)}
                        aria-current={day === now ? "date" : undefined}
                        onClick={() => pick(day)}
                        className={cn(
                          "size-[34px] rounded-md font-mono text-mono tabular",
                          selected
                            ? "bg-primary font-semibold text-white"
                            : out
                              ? "text-ink-disabled line-through"
                              : day.slice(0, 7) !== month.slice(0, 7)
                                ? "text-ink-subtle hover:bg-muted"
                                : "text-ink hover:bg-muted",
                          day === now && !selected && "border border-border-strong font-semibold",
                        )}
                      >
                        {Number(day.slice(8))}
                      </button>
                    </span>
                  );
                })}
              </div>
            ))}
          </div>
          <div className="flex items-center justify-between gap-2 border-t border-muted pt-2 text-caption">
            <button type="button" className="link min-h-8 font-medium" disabled={now < min || now > max} onClick={() => pick(now)}>
              Today
            </button>
            <span className="text-ink-subtle">
              {formatShortDay(min)} – {formatShortDay(max)}
            </span>
            <button type="button" onClick={close} className="min-h-8 rounded-md px-2 font-medium text-ink hover:bg-muted">
              Close
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
