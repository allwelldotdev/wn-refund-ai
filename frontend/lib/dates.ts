/**
 * Calendar days as "YYYY-MM-DD" strings in the viewer's time zone. Strings in
 * this form sort and compare correctly as plain strings.
 */

export function isoDay(d: Date): string {
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${d.getFullYear()}-${m}-${day}`;
}

export function parseDay(day: string): Date {
  const [y, m, d] = day.split("-").map(Number);
  return new Date(y, m - 1, d);
}

export function today(now: Date = new Date()): string {
  return isoDay(now);
}

export function addDays(day: string, n: number): string {
  const d = parseDay(day);
  d.setDate(d.getDate() + n);
  return isoDay(d);
}

export function clampDay(day: string, min: string, max: string): string {
  return day < min ? min : day > max ? max : day;
}

/** Whole days from `a` to `b`. */
export function daysBetween(a: string, b: string): number {
  return Math.round((parseDay(b).getTime() - parseDay(a).getTime()) / 86_400_000);
}

const long = new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric", year: "numeric" });
const short = new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric" });
const month = new Intl.DateTimeFormat("en-US", { month: "long", year: "numeric" });

/** "Sep 19, 2026" */
export function formatDay(day: string): string {
  return long.format(parseDay(day));
}

/** "Sep 19" */
export function formatShortDay(day: string): string {
  return short.format(parseDay(day));
}

/** "September 2026" for the month containing `day`. */
export function formatMonth(day: string): string {
  return month.format(parseDay(day));
}
