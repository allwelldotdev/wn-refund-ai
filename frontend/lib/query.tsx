"use client";

import { QueryCache, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useState, type ReactNode } from "react";

import { isApiError } from "./bff";

/** Sends the tab back to sign-in, remembering where it was. */
export function redirectToLogin(reason: "expired" | null = "expired") {
  const next = `${window.location.pathname}${window.location.search}`;
  const params = new URLSearchParams();
  if (reason) params.set("reason", reason);
  if (!next.startsWith("/login")) params.set("next", next);
  window.location.replace(`/login?${params}`);
}

export function QueryProvider({ children }: { children: ReactNode }) {
  const [client] = useState(
    () =>
      new QueryClient({
        queryCache: new QueryCache({
          // A 401 on any background read means this tab's session is gone.
          onError: (e) => {
            if (isApiError(e, 401) && !window.location.pathname.startsWith("/login")) redirectToLogin();
          },
        }),
        defaultOptions: {
          queries: {
            staleTime: 10_000,
            refetchOnWindowFocus: false,
            retry: (count, e) => !(isApiError(e) && e.status < 500) && count < 2,
          },
        },
      }),
  );
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}
