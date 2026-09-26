import { describe, expect, it } from "vitest";

import { dollarsToCents, formatAge, formatCents, formatClock, formatRelativeDayTime } from "@/lib/format";

describe("format", () => {
  it("formats cents as the design's money", () => {
    expect(formatCents(120000)).toBe("$1,200.00");
    expect(formatCents(null)).toBe("—");
  });

  it("parses dollar input strictly", () => {
    expect(dollarsToCents("$1,200.50")).toBe(120050);
    expect(dollarsToCents("600")).toBe(60000);
    expect(dollarsToCents("6.005")).toBeNull();
    expect(dollarsToCents("abc")).toBeNull();
  });

  it("formats waiting ages like the escalation cards", () => {
    const now = new Date("2026-09-24T10:32:00");
    expect(formatAge("2026-09-24T10:31:00", now)).toBe("1 m");
    expect(formatAge("2026-09-24T09:58:00", now)).toBe("34 m");
    expect(formatAge("2026-09-24T07:55:00", now)).toBe("2 h 37 m");
    expect(formatAge("2026-09-23T14:32:00", now)).toBe("20 h");
  });

  it("formats relative day and time", () => {
    const now = new Date("2026-09-24T12:00:00");
    expect(formatRelativeDayTime("2026-09-24T10:31:00", now)).toBe("Today 10:31");
    expect(formatRelativeDayTime("2026-09-23T18:02:00", now)).toBe("Yesterday 18:02");
    expect(formatRelativeDayTime("2026-09-22T15:48:00", now)).toBe("Sep 22 15:48");
  });

  it("formats countdowns as m:ss", () => {
    expect(formatClock(272)).toBe("4:32");
    expect(formatClock(0.2)).toBe("0:01");
  });
});
