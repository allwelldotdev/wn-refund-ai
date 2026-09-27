"use client";

import Link from "next/link";
import { useState } from "react";

import { buttonClasses } from "@/components/ui/Button";
import { Icon, type IconName } from "@/components/ui/Icon";
import { EmptyState, LoadingRegion, Skeleton } from "@/components/ui/Surface";
import { useRequests, useStats } from "@/lib/admin";
import { cn } from "@/lib/cn";
import { formatAge, startOfToday } from "@/lib/format";
import { useNow } from "@/lib/use-now";
import { useOpenRequest } from "@/lib/use-open-request";

import { RequestTable } from "./RequestTable";
import { SectionError, TableSkeleton } from "./common";

const dayFmt = new Intl.DateTimeFormat("en-US", { weekday: "short", month: "short", day: "numeric" });

export function OverviewSection() {
  const [since] = useState(() => startOfToday().toISOString());
  const stats = useStats(since);
  const recent = useRequests({ limit: 8, offset: 0 });
  const { open, selected } = useOpenRequest();
  const now = useNow();

  if (stats.isError && !stats.data) {
    return <SectionError what="the overview" error={stats.error} onRetry={() => void stats.refetch()} />;
  }

  const s = stats.data;
  const c = s?.created;
  const tiles: Array<{ label: string; value: number | undefined; sub: string; icon?: IconName; tone?: string; action?: boolean }> = [
    { label: "Requests today", value: c?.total, sub: "Since 00:00" },
    {
      label: "Approved",
      value: c ? c.approved + c.resolved_approved : undefined,
      sub: c?.resolved_approved ? `Incl. ${c.resolved_approved} after review` : "Automatically or after review",
      icon: "check-circle",
      tone: "text-approved-icon",
    },
    {
      label: "Denied",
      value: c ? c.denied + c.resolved_denied : undefined,
      sub: "Customer told the rule",
      icon: "x-circle",
      tone: "text-denied-icon",
    },
    { label: "Escalated", value: c?.escalated, sub: "Sent to a person today", icon: "warning", tone: "text-escalated-icon" },
    {
      label: "Open escalations",
      value: s?.open_escalations,
      sub: s?.oldest_open_escalation_at ? `Oldest waiting ${formatAge(s.oldest_open_escalation_at, now)}` : "Queue is clear",
      action: true,
    },
  ];

  return (
    <div className="flex flex-col gap-6">
      <p className="text-meta text-ink-subtle">Today · {dayFmt.format(new Date(since))}</p>

      {stats.isPending ? (
        <LoadingRegion label="Loading Overview" className="grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-5">
          {tiles.map((t) => (
            <Skeleton key={t.label} className="h-[116px] rounded-lg" />
          ))}
        </LoadingRegion>
      ) : (
        <ul className="grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-5">
          {tiles.map((t) => (
            <li key={t.label} className="flex flex-col gap-1 rounded-lg border border-border bg-surface p-4 shadow-xs">
              <span className="flex items-center gap-1.5 text-meta font-medium text-ink-muted">
                {t.icon ? <Icon name={t.icon} size={14} strokeWidth={2.25} className={t.tone} /> : null}
                {t.label}
              </span>
              <span className="font-mono text-metric font-medium tabular">{t.value ?? "—"}</span>
              <span className="flex flex-wrap items-center justify-between gap-2 text-caption text-ink-subtle">
                {t.sub}
                {t.action && s?.open_escalations ? (
                  <Link href="/admin/escalations" className="link text-meta font-medium">
                    Review
                  </Link>
                ) : null}
              </span>
            </li>
          ))}
        </ul>
      )}

      <section className="flex flex-col gap-3">
        <div className="flex items-center justify-between gap-3">
          <h2 className="text-title-sm font-semibold">Recent requests</h2>
          <Link href="/admin/requests" className="link text-body-sm font-medium">
            View all requests
          </Link>
        </div>
        {recent.isPending ? (
          <TableSkeleton label="Loading recent requests" />
        ) : recent.isError ? (
          <SectionError what="recent requests" error={recent.error} onRetry={() => void recent.refetch()} />
        ) : recent.data.items.length === 0 ? (
          <EmptyState
            icon="inbox"
            title="No requests yet today"
            action={<Link href="/admin/requests" className={buttonClasses("secondary")}>View all requests</Link>}
          >
            When customers ask for refunds, today&apos;s numbers and the latest requests show up here.
          </EmptyState>
        ) : (
          <div className={cn(recent.isFetching && "transition-opacity")}>
            <RequestTable variant="overview" caption="Most recent refund requests" items={recent.data.items} selected={selected} onOpen={open} />
          </div>
        )}
      </section>
    </div>
  );
}
