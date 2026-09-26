import { type NextRequest, NextResponse } from "next/server";

import { BACKEND_URL, COOKIE_OPTIONS, sessionCookieName, tabIdFrom } from "@/lib/backend";

export const dynamic = "force-dynamic";

/** Ends the tab's session upstream and always clears its cookie. */
export async function POST(req: NextRequest) {
  const tabId = tabIdFrom(req.headers);
  const res = NextResponse.json({});
  if (!tabId) return res;
  const name = sessionCookieName(tabId);
  const token = req.cookies.get(name)?.value;
  if (token) {
    await fetch(`${BACKEND_URL}/api/auth/logout`, {
      method: "POST",
      headers: { authorization: `Bearer ${token}` },
      cache: "no-store",
    }).catch(() => undefined);
  }
  res.cookies.set(name, "", { ...COOKIE_OPTIONS, maxAge: 0 });
  return res;
}
