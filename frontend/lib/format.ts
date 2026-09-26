import type { Cents, ReasonCategory } from "./api-types";

const money = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" });

/** 120000 → "$1,200.00" */
export function formatCents(cents: Cents | null | undefined): string {
  return cents === null || cents === undefined ? "—" : money.format(cents / 100);
}

/** Dollars typed in a form → integer cents, or null when not a valid amount. */
export function dollarsToCents(input: string): number | null {
  const trimmed = input.trim().replace(/[$,\s]/g, "");
  if (!/^\d+(\.\d{1,2})?$/.test(trimmed)) return null;
  return Math.round(Number(trimmed) * 100);
}

export function centsToDollarsInput(cents: number): string {
  return (cents / 100).toFixed(2).replace(/\.00$/, "");
}

const dateFmt = new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric", year: "numeric" });
const shortDateFmt = new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric" });
const timeFmt = new Intl.DateTimeFormat("en-GB", { hour: "2-digit", minute: "2-digit", hour12: false });

/** "Sep 22, 2026" */
export function formatDate(iso: string): string {
  return dateFmt.format(new Date(iso));
}

/** "Sep 22" */
export function formatShortDate(iso: string): string {
  return shortDateFmt.format(new Date(iso));
}

/** "10:31" (24-hour clock) */
export function formatTime(iso: string): string {
  return timeFmt.format(new Date(iso));
}

function startOfDay(d: Date): number {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

/** "Today 10:31", "Yesterday 18:02" or "Sep 22 15:48". */
export function formatRelativeDayTime(iso: string, now: Date = new Date()): string {
  const d = new Date(iso);
  const days = Math.round((startOfDay(now) - startOfDay(d)) / 86_400_000);
  const day = days === 0 ? "Today" : days === 1 ? "Yesterday" : formatShortDate(iso);
  return `${day} ${formatTime(iso)}`;
}

/** "Sep 23, 2026 · 11:40" */
export function formatDateTime(iso: string): string {
  return `${formatDate(iso)} · ${formatTime(iso)}`;
}

/** "2026-09-14 09:22 UTC", used in the policy history. */
export function formatUtc(iso: string): string {
  const d = new Date(iso);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())} UTC`;
}

/** Local midnight today, as the `since` bound for "today" counts. */
export function startOfToday(now: Date = new Date()): Date {
  return new Date(now.getFullYear(), now.getMonth(), now.getDate());
}

/** "20 h", "2 h 37 m", "34 m", "1 m" */
export function formatAge(fromIso: string, now: Date = new Date()): string {
  const minutes = Math.max(1, Math.floor((now.getTime() - new Date(fromIso).getTime()) / 60_000));
  if (minutes < 60) return `${minutes} m`;
  const hours = Math.floor(minutes / 60);
  if (hours >= 10) return hours >= 48 ? `${Math.floor(hours / 24)} d` : `${hours} h`;
  const rest = minutes % 60;
  return rest ? `${hours} h ${rest} m` : `${hours} h`;
}

/** "2.41 s" */
export function formatLatency(ms: number | null | undefined): string {
  return ms === null || ms === undefined ? "—" : `${(ms / 1000).toFixed(2)} s`;
}

const int = new Intl.NumberFormat("en-US");
export function formatInt(n: number): string {
  return int.format(n);
}

/** "m:ss" for countdowns and elapsed timers. */
export function formatClock(totalSeconds: number): string {
  const s = Math.max(0, Math.ceil(totalSeconds));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

export const REASON_LABELS: Record<ReasonCategory, string> = {
  damaged: "Damaged",
  wrong_item: "Wrong item",
  not_received: "Not received or not provided",
  changed_mind: "Change of mind",
  not_as_described: "Not as described",
  other: "Other",
};

export function firstName(name: string): string {
  return name.trim().split(/\s+/)[0] ?? name;
}

export function plural(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`;
}
