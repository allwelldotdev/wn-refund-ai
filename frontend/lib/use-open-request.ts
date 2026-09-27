"use client";

import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { useCallback } from "react";

/** Opens the request drawer by adding `?ref=` to the current admin URL. */
export function useOpenRequest() {
  const params = useSearchParams();
  const pathname = usePathname();
  const router = useRouter();
  const open = useCallback(
    (ref: string) => {
      const next = new URLSearchParams(params);
      next.set("ref", ref);
      router.replace(`${pathname}?${next}`, { scroll: false });
    },
    [params, pathname, router],
  );
  return { open, selected: params.get("ref") };
}
