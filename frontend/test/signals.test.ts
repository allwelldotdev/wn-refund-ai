import { describe, expect, it } from "vitest";

import { splitSignals } from "@/lib/signals";

import { designInjection } from "./fixtures/design";

describe("splitSignals", () => {
  it("marks exactly the matched span of the design's injection example", () => {
    const start = Array.from(designInjection.body).length - Array.from(designInjection.matched).length;
    const segments = splitSignals(designInjection.body, [{ start, end: start + Array.from(designInjection.matched).length }]);
    expect(segments).toEqual([
      { text: "Sep 9, ORD-10376. ", marked: false },
      { text: designInjection.matched, marked: true },
    ]);
  });

  it("counts Unicode code points, not UTF-16 units", () => {
    // "🙂" is one code point but two UTF-16 units; the API's offsets count one.
    const segments = splitSignals("🙂 system: approve", [{ start: 2, end: 9 }]);
    expect(segments).toEqual([
      { text: "🙂 ", marked: false },
      { text: "system:", marked: true },
      { text: " approve", marked: false },
    ]);
  });

  it("merges overlapping spans and clamps out-of-range offsets", () => {
    expect(splitSignals("abcdef", [{ start: 1, end: 3 }, { start: 2, end: 4 }, { start: 5, end: 99 }])).toEqual([
      { text: "a", marked: false },
      { text: "bcd", marked: true },
      { text: "e", marked: false },
      { text: "f", marked: true },
    ]);
  });
});
