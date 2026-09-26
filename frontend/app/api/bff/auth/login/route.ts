import { type NextRequest, NextResponse } from "next/server";

import {
  BACKEND_URL,
  COOKIE_OPTIONS,
  jsonError,
  passThroughHeaders,
  sessionCookieName,
  tabIdFrom,
} from "@/lib/backend";

export const dynamic = "force-dynamic";

/**
 * Signs this tab in. The API's bearer token goes into an httpOnly cookie
 * scoped to the tab (`sid_<tabId>`); the browser only ever sees the principal.
 */
export async function POST(req: NextRequest) {
  const tabId = tabIdFrom(req.headers);
  if (!tabId) return jsonError(400, "invalid_tab", "Missing or malformed X-Tab-Id header.");

  let upstream: Response;
  try {
    upstream = await fetch(`${BACKEND_URL}/api/auth/login`, {
      method: "POST",
      headers: { "content-type": req.headers.get("content-type") ?? "application/json" },
      body: await req.text(),
      cache: "no-store",
    });
  } catch {
    return jsonError(502, "backend_unavailable", "The support service is not reachable. Try again shortly.");
  }
  if (!upstream.ok) {
    return new Response(upstream.body, { status: upstream.status, headers: passThroughHeaders(upstream) });
  }

  const { token, principal } = (await upstream.json()) as { token: string; principal: unknown };
  const res = NextResponse.json({ principal });
  res.cookies.set(sessionCookieName(tabId), token, COOKIE_OPTIONS);
  return res;
}
