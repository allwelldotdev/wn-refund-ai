"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import type { ReactNode } from "react";

import { BrandMark, Icon, type IconName } from "@/components/ui/Icon";
import { Avatar } from "@/components/ui/Surface";
import { useCurrentPolicy, useStats } from "@/lib/admin";
import type { Principal } from "@/lib/api-types";
import { api } from "@/lib/bff";
import { cn } from "@/lib/cn";
import { formatShortDate, startOfToday } from "@/lib/format";

export const SECTIONS: Array<{ key: string; href: string; label: string; icon: IconName }> = [
  { key: "overview", href: "/admin/overview", label: "Overview", icon: "grid" },
  { key: "requests", href: "/admin/requests", label: "Requests", icon: "list" },
  { key: "escalations", href: "/admin/escalations", label: "Escalations", icon: "warning" },
  { key: "policy", href: "/admin/policy", label: "Policy", icon: "policy" },
  { key: "settings", href: "/admin/settings", label: "Settings", icon: "settings" },
];

/**
 * Admin frame: a 248px side nav from 1280px, an 84px icon rail from 768px,
 * and a bottom tab bar below that. The page title comes from the section.
 */
export function AdminShell({ principal, children }: { principal: Principal; children: ReactNode }) {
  const pathname = usePathname();
  const current = SECTIONS.find((s) => pathname.startsWith(s.href)) ?? SECTIONS[0];
  const stats = useStats(startOfToday().toISOString());
  const policy = useCurrentPolicy();
  const open = stats.data?.open_escalations ?? 0;

  async function signOut() {
    try {
      await api("auth/logout", { method: "POST" });
    } finally {
      window.location.replace("/login");
    }
  }

  return (
    <div className="grid min-h-dvh grid-cols-1 md:h-dvh md:grid-cols-[84px_minmax(0,1fr)] xl:grid-cols-[248px_minmax(0,1fr)]">
      <nav aria-label="Admin sections" className="hidden flex-col border-r border-border bg-surface md:flex">
        <div className="flex h-16 items-center gap-2.5 px-5 max-xl:justify-center max-xl:px-0">
          <BrandMark />
          <span className="hidden flex-col xl:flex">
            <span className="text-lead font-semibold">Worknoon Support</span>
            <span className="text-caption text-ink-subtle">Admin</span>
          </span>
        </div>
        <ul className="flex flex-col gap-1 px-3 max-xl:px-2">
          {SECTIONS.map((s) => {
            const on = s === current;
            const count = s.key === "escalations" ? open : 0;
            return (
              <li key={s.key}>
                <Link
                  href={s.href}
                  aria-current={on ? "page" : undefined}
                  aria-label={count ? `${s.label}, ${count} open` : undefined}
                  className={cn(
                    "relative flex min-h-11 items-center gap-3 rounded-md px-3 text-body-sm",
                    "max-xl:flex-col max-xl:justify-center max-xl:gap-1 max-xl:px-1 max-xl:py-2 max-xl:text-micro",
                    on ? "bg-muted font-semibold text-ink" : "font-medium text-ink-muted hover:bg-canvas hover:text-ink",
                  )}
                >
                  <Icon name={s.icon} size={18} />
                  <span className="xl:flex-grow">{s.label}</span>
                  {count ? (
                    <span
                      aria-hidden="true"
                      className={cn(
                        "inline-flex items-center justify-center rounded-full font-semibold tabular",
                        "xl:h-5 xl:min-w-6 xl:border xl:border-escalated-border xl:bg-escalated-bg xl:px-1.5 xl:text-caption xl:text-escalated-fg",
                        "max-xl:absolute max-xl:top-1 max-xl:right-3 max-xl:h-[18px] max-xl:min-w-[18px] max-xl:bg-escalated-fg max-xl:px-1 max-xl:text-micro max-xl:text-white",
                      )}
                    >
                      {count}
                    </span>
                  ) : null}
                </Link>
              </li>
            );
          })}
        </ul>
        <div className="mt-auto hidden p-3 xl:block">
          <Link href="/admin/policy" className="flex flex-col gap-0.5 rounded-lg border border-border bg-canvas px-3 py-2.5 hover:border-border-strong">
            <span className="text-caption font-medium text-ink-subtle">Active policy</span>
            <span className="text-body-sm font-medium text-ink">
              {policy.data
                ? `Version ${policy.data.version.version} · ${formatShortDate(policy.data.version.created_at)}`
                : "Loading…"}
            </span>
          </Link>
        </div>
      </nav>

      <div className="flex min-w-0 flex-col md:overflow-hidden">
        <header className="flex h-14 shrink-0 items-center justify-between gap-3 border-b border-border bg-surface px-4 md:h-16 md:px-6 xl:px-8">
          <div className="flex items-center gap-2.5">
            <span className="md:hidden">
              <BrandMark />
            </span>
            <h1 className="text-title-md font-semibold md:text-page">{current.label}</h1>
          </div>
          <div className="flex items-center gap-3">
            <Avatar name={principal.name} />
            <span className="hidden flex-col md:flex">
              <span className="text-body-sm font-medium">
                <span className="sr-only">Signed in as </span>
                {principal.name}
              </span>
              <span className="text-caption text-ink-subtle">Admin</span>
            </span>
            <button type="button" onClick={signOut} className="link min-h-11 px-1 text-body-sm font-medium">
              Sign out
            </button>
          </div>
        </header>
        <main className="flex-grow px-4 pt-4 pb-[calc(96px+env(safe-area-inset-bottom))] md:overflow-y-auto md:px-6 md:pt-6 md:pb-12 xl:px-8 xl:pt-8">
          {children}
        </main>
      </div>

      <nav
        aria-label="Admin sections"
        className="fixed inset-x-0 bottom-0 z-20 grid grid-cols-5 border-t border-border bg-surface pb-[env(safe-area-inset-bottom)] md:hidden"
      >
        {SECTIONS.map((s) => {
          const on = s === current;
          const count = s.key === "escalations" ? open : 0;
          return (
            <Link
              key={s.key}
              href={s.href}
              aria-current={on ? "page" : undefined}
              aria-label={count ? `${s.label}, ${count} open` : undefined}
              className={cn(
                "relative flex min-h-14 flex-col items-center justify-center gap-0.5 text-micro",
                on ? "font-semibold text-ink" : "font-medium text-ink-muted",
              )}
            >
              <Icon name={s.icon} size={20} />
              {s.label}
              {count ? (
                <span aria-hidden="true" className="absolute top-1.5 left-[calc(50%+6px)] inline-flex h-[18px] min-w-[18px] items-center justify-center rounded-full bg-escalated-fg px-1 text-micro font-semibold text-white">
                  {count}
                </span>
              ) : null}
            </Link>
          );
        })}
      </nav>
    </div>
  );
}
