import type { NextRequest } from "next/server";

import { BACKEND_URL, jsonError, passThroughHeaders, sessionCookieName, tabIdFrom } from "@/lib/backend";

export const dynamic = "force-dynamic";

const SEGMENT = /^[A-Za-z0-9_.-]+$/;
/** Sign-in and sign-out have their own handlers; never proxy them raw (the login body holds the token). */
const RESERVED = new Set(["auth/login", "auth/logout"]);

/**
 * Proxies `/api/bff/<path>` to `${BACKEND_URL}/api/<path>`, adding the tab's
 * bearer token. Bodies and responses are streamed, so SSE passes straight through.
 */
async function proxy(req: NextRequest, ctx: { params: Promise<{ path: string[] }> }) {
  const { path } = await ctx.params;
  if (path.some((s) => !SEGMENT.test(s) || s === "." || s === "..")) {
    return jsonError(404, "not_found", "Not found.");
  }
  const joined = path.join("/");
  if (RESERVED.has(joined.toLowerCase())) return jsonError(404, "not_found", "Not found.");

  const headers = new Headers();
  for (const name of ["content-type", "accept"]) {
    const v = req.headers.get(name);
    if (v) headers.set(name, v);
  }
  const tabId = tabIdFrom(req.headers);
  const token = tabId ? req.cookies.get(sessionCookieName(tabId))?.value : undefined;
  if (token) headers.set("authorization", `Bearer ${token}`);

  const hasBody = req.method !== "GET" && req.method !== "HEAD";
  let upstream: Response;
  try {
    upstream = await fetch(`${BACKEND_URL}/api/${joined}${req.nextUrl.search}`, {
      method: req.method,
      headers,
      body: hasBody ? req.body : undefined,
      // Required by Node's fetch when streaming a request body.
      ...(hasBody ? { duplex: "half" } : {}),
      cache: "no-store",
      signal: req.signal,
    } as RequestInit);
  } catch {
    return jsonError(502, "backend_unavailable", "The support service is not reachable. Try again shortly.");
  }
  return new Response(upstream.body, { status: upstream.status, headers: passThroughHeaders(upstream) });
}

export { proxy as GET, proxy as POST, proxy as PUT, proxy as PATCH, proxy as DELETE };
