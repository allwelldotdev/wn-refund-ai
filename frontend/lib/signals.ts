import type { SignalView } from "./api-types";

export type Segment = { text: string; marked: boolean };

/**
 * Splits message text on signal spans. Offsets count Unicode code points (as
 * the API does), so text is indexed with `Array.from`, not UTF-16 units.
 * Overlapping spans merge; out-of-range offsets are clamped.
 */
export function splitSignals(text: string, signals: Pick<SignalView, "start" | "end">[]): Segment[] {
  const chars = Array.from(text);
  const marked = new Array<boolean>(chars.length).fill(false);
  for (const s of signals) {
    for (let i = Math.max(0, s.start); i < Math.min(chars.length, s.end); i++) marked[i] = true;
  }
  const out: Segment[] = [];
  let i = 0;
  while (i < chars.length) {
    const on = marked[i];
    let j = i;
    while (j < chars.length && marked[j] === on) j++;
    out.push({ text: chars.slice(i, j).join(""), marked: on });
    i = j;
  }
  return out;
}
