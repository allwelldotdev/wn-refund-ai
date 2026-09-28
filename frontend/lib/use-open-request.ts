"use client";

import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { useCallback } from "react";

/** Opens the request drawer by adding `?ref=` to the current admin URL; `reply` focuses its message box. */
export function useOpenRequest() {
  const params = useSearchParams();
  const pathname = usePathname();
  const router = useRouter();
  const open = useCallback(
    (ref: string, opts: { reply?: boolean } = {}) => {
      const next = new URLSearchParams(params);
      next.set("ref", ref);
      if (opts.reply) next.set("reply", "1");
      else next.delete("reply");
      router.replace(`${pathname}?${next}`, { scroll: false });
    },
    [params, pathname, router],
  );
  return { open, selected: params.get("ref") };
}
