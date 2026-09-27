import { describe, expect, it } from "vitest";

import { SseParser } from "@/lib/sse";

import { designVerdictStream } from "./fixtures/design";

describe("SseParser", () => {
  it("rebuilds events across chunk boundaries, CRLF and keep-alive comments", () => {
    const parser = new SseParser();
    const events = designVerdictStream.flatMap((chunk) => parser.push(chunk));
    expect(events.map((e) => e.event)).toEqual([
      "message_saved",
      "reply_start",
      "reply_token",
      "reply_token",
      "reply_done",
      "request_updated",
      "done",
    ]);
    const text = events.flatMap((e) => (e.event === "reply_token" ? [e.data.text] : [])).join("");
    const done = events.find((e) => e.event === "reply_done");
    expect(done?.event === "reply_done" && done.data.body).toBe(text);
  });

  it("holds a frame until its blank line arrives", () => {
    const parser = new SseParser();
    expect(parser.push('event: done\ndata: {}')).toEqual([]);
    expect(parser.push("\n\n")).toEqual([{ event: "done", data: {} }]);
  });

  it("skips frames without data or with malformed JSON", () => {
    const parser = new SseParser();
    expect(parser.push("event: reply_start\n\nevent: error\ndata: {oops\n\n")).toEqual([]);
  });
});
