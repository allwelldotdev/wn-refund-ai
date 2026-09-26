import "server-only";

/** Where the BFF reaches the Rust API (compose: http://backend:8080). */
export const BACKEND_URL = (process.env.BACKEND_URL ?? "http://localhost:8080").replace(/\/$/, "");

const TAB_ID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

/** The tab id header, if it is a lowercase UUID. */
export function tabIdFrom(headers: Headers): string | null {
  const id = headers.get("x-tab-id");
  return id && TAB_ID.test(id) ? id : null;
}

export function sessionCookieName(tabId: string): string {
  return `sid_${tabId}`;
}

export const COOKIE_OPTIONS = {
  httpOnly: true,
  sameSite: "strict" as const,
  path: "/api/bff",
  maxAge: 60 * 60 * 24,
  // The demo is served over plain http; set true behind TLS.
  secure: false,
};

export function jsonError(status: number, code: string, message: string): Response {
  return Response.json({ error: { code, message } }, { status });
}

/** Response headers worth passing from the API to the browser. */
export function passThroughHeaders(upstream: Response): Headers {
  const headers = new Headers();
  for (const name of ["content-type", "retry-after"]) {
    const v = upstream.headers.get(name);
    if (v) headers.set(name, v);
  }
  if (upstream.headers.get("content-type")?.includes("text/event-stream")) {
    headers.set("cache-control", "no-cache");
    headers.set("x-accel-buffering", "no");
  } else {
    headers.set("cache-control", "no-store");
  }
  return headers;
}
