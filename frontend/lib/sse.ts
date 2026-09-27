import type { SseEvent } from "./api-types";
import { bffFetch, toApiError } from "./bff";

/**
 * Incremental parser for `text/event-stream` frames: feed it chunks, get back
 * complete events. Comment lines (keep-alives) and unknown fields are ignored.
 */
export class SseParser {
  private buffer = "";

  push(chunk: string): SseEvent[] {
    this.buffer += chunk.replace(/\r\n?/g, "\n");
    const events: SseEvent[] = [];
    let end: number;
    while ((end = this.buffer.indexOf("\n\n")) >= 0) {
      const frame = this.buffer.slice(0, end);
      this.buffer = this.buffer.slice(end + 2);
      const parsed = parseFrame(frame);
      if (parsed) events.push(parsed);
    }
    return events;
  }
}

function parseFrame(frame: string): SseEvent | null {
  let event = "message";
  const data: string[] = [];
  for (const line of frame.split("\n")) {
    if (line === "" || line.startsWith(":")) continue;
    const colon = line.indexOf(":");
    const field = colon < 0 ? line : line.slice(0, colon);
    const value = colon < 0 ? "" : line.slice(colon + 1).replace(/^ /, "");
    if (field === "event") event = value;
    else if (field === "data") data.push(value);
  }
  if (data.length === 0) return null;
  try {
    return { event, data: JSON.parse(data.join("\n")) } as SseEvent;
  } catch {
    return null;
  }
}

/**
 * POSTs a customer message and streams the reply. HTTP-level failures (401,
 * 409, 422, 429, …) arrive as JSON instead of a stream and are thrown as
 * `ApiError` before any event is delivered.
 */
export async function postMessage(
  conversationId: string,
  body: { client_msg_id: string; body: string; order_id?: string | null },
  onEvent: (e: SseEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const res = await bffFetch(`conversations/${conversationId}/messages`, {
    method: "POST",
    headers: { Accept: "text/event-stream" },
    body: JSON.stringify(body),
    signal,
  });
  if (!res.ok || !res.headers.get("content-type")?.includes("text/event-stream")) {
    throw await toApiError(res);
  }
  const reader = res.body!.pipeThrough(new TextDecoderStream()).getReader();
  const parser = new SseParser();
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    for (const e of parser.push(value)) onEvent(e);
  }
}
