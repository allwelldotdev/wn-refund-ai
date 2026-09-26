"use client";

import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { Suspense, type ReactNode } from "react";

import { AdminShell } from "@/components/admin/AdminShell";
import { RequestDrawer } from "@/components/admin/RequestDrawer";
import { RequireRole } from "@/lib/session";

/** Any admin section can open a request's case file with `?ref=RR-…`. */
function DrawerFromUrl() {
  const params = useSearchParams();
  const pathname = usePathname();
  const router = useRouter();
  const ref = params.get("ref");
  return (
    <RequestDrawer
      requestRef={ref}
      onClose={() => {
        const next = new URLSearchParams(params);
        next.delete("ref");
        router.replace(next.size ? `${pathname}?${next}` : pathname, { scroll: false });
      }}
    />
  );
}

export default function AdminLayout({ children }: { children: ReactNode }) {
  return (
    <RequireRole role="admin">
      {(principal) => (
        <AdminShell principal={principal}>
          <Suspense>
            {children}
            <DrawerFromUrl />
          </Suspense>
        </AdminShell>
      )}
    </RequireRole>
  );
}
