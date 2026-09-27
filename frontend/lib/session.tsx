"use client";

import { useQuery } from "@tanstack/react-query";
import { useRouter } from "next/navigation";
import { useEffect, type ReactNode } from "react";

import { Spinner } from "@/components/ui/Icon";

import type { Principal, Role } from "./api-types";
import { api } from "./bff";

export const HOME: Record<Role, string> = { customer: "/support", admin: "/admin/overview" };

export function useSession() {
  return useQuery({
    queryKey: ["me"],
    queryFn: () => api<{ principal: Principal }>("auth/me").then((r) => r.principal),
    staleTime: Infinity,
  });
}

/**
 * Client-side route guard. Page navigations carry no tab id, so the server
 * cannot see this tab's session; the Rust API still enforces every role.
 */
export function RequireRole({ role, children }: { role: Role; children: (p: Principal) => ReactNode }) {
  const router = useRouter();
  const session = useSession();
  const principal = session.data;
  const wrongRole = principal && principal.kind !== role;

  useEffect(() => {
    if (wrongRole) router.replace(HOME[principal.kind]);
  }, [wrongRole, principal, router]);

  if (principal && !wrongRole) return <>{children(principal)}</>;
  return (
    <div aria-busy="true" className="flex min-h-dvh items-center justify-center text-ink-muted">
      <Spinner size={20} />
      <span className="sr-only">Loading</span>
    </div>
  );
}
