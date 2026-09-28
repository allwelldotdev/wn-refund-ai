"use client";

import { useQueries } from "@tanstack/react-query";
import Link from "next/link";
import { useEffect, useRef, useState } from "react";

import { StatusBadge } from "@/components/ui/Badge";
import { Button, buttonClasses } from "@/components/ui/Button";
import { Icon } from "@/components/ui/Icon";
import { EmptyState, LoadingRegion, Skeleton } from "@/components/ui/Surface";
import { useToast } from "@/components/ui/Toast";
import { FLAG_META, detailQuery, distinctFlags, useRequests } from "@/lib/admin";
import type { AdminListItem, RequestDetail } from "@/lib/api-types";
import { cn } from "@/lib/cn";
import { REASON_LABELS, formatAge, formatCents, formatDateTime, formatRelativeDayTime, plural } from "@/lib/format";
import { useNow } from "@/lib/use-now";
import { useOpenRequest } from "@/lib/use-open-request";

import { FlagChips, ResolveDialog, ReviewDraft, SectionError, type ResolveTarget } from "./common";

const OLD_AFTER_MS = 24 * 3600 * 1000;

/** Why the engine sent it to a person, in words: the escalating rules, then the flags. */
function whyEscalated(d: RequestDetail | undefined): string | null {
  if (d?.request.disputed_at) {
    const denial = d.audit?.rule_trace.find((f) => f.verdict === "denied")?.explanation;
    return `The assistant denied this automatically${denial ? ` (${denial.replace(/\.$/, "")})` : ""}. The customer disputed it from Your requests, so a person needs to decide.`;
  }
  if (!d?.audit) return d ? "Seeded history: escalated before this system existed." : null;
  const rules = d.audit.rule_trace.filter((f) => f.verdict === "escalated" && f.kind !== "fail_closed").map((f) => f.explanation);
  const flags = distinctFlags(d.audit.flags).map((f) => FLAG_META[f].tip.replace(/^[^:]+:\s*/, ""));
  return [...rules, ...flags].join(" ") || "No policy rule applied, so a person decides.";
}

/** "10:34" today, else "Yesterday 18:02" / "Sep 22 15:48". */
function replyTime(iso: string): string {
  return formatRelativeDayTime(iso).replace(/^Today /, "");
}

/**
 * Toasts what changed in the open queue since the last poll: a new
 * escalation or dispute, or a customer message on a request not open in the drawer.
 */
function useQueueToasts(items: AdminListItem[] | undefined, openRef: string | null) {
  const toast = useToast();
  const seen = useRef<Map<string, number> | null>(null);
  useEffect(() => {
    if (!items) return;
    const before = seen.current;
    seen.current = new Map(items.map((r) => [r.ref, r.unread_from_customer]));
    if (!before) return;
    for (const r of items) {
      const had = before.get(r.ref);
      if (had === undefined) {
        toast({ tone: "info", title: `New ${r.disputed_at ? "dispute" : "escalation"} ${r.ref} from ${r.customer_name}.` });
      } else if (r.unread_from_customer > had && r.ref !== openRef) {
        toast({ tone: "info", title: `New message from ${r.customer_name} on ${r.ref}.` });
      }
    }
  }, [items, openRef, toast]);
}

export function EscalationsSection() {
  const open = useRequests({ state: "escalated", limit: 200, offset: 0 });
  const approved = useRequests({ state: "resolved_approved", limit: 10, offset: 0 });
  const denied = useRequests({ state: "resolved_denied", limit: 10, offset: 0 });
  const { open: openDrawer, selected } = useOpenRequest();
  const [resolve, setResolve] = useState<ResolveTarget | null>(null);
  const now = useNow();
  useQueueToasts(open.data?.items, selected);

  // New customer replies first (latest first), then oldest first.
  const oldestFirst = [...(open.data?.items ?? [])].reverse();
  const queue = [
    ...oldestFirst.filter((r) => r.customer_replied_at).sort((a, b) => b.customer_replied_at!.localeCompare(a.customer_replied_at!)),
    ...oldestFirst.filter((r) => !r.customer_replied_at),
  ];
  const decided = [...(approved.data?.items ?? []), ...(denied.data?.items ?? [])]
    .sort((a, b) => (b.resolved_at ?? "").localeCompare(a.resolved_at ?? ""))
    .slice(0, 10);
  const details = useQueries({ queries: [...queue, ...decided].map((r) => detailQuery(r.ref)) });
  const detailOf = (ref: string) => details.find((q) => q.data?.request.ref === ref)?.data;

  if (open.isPending) {
    return (
      <LoadingRegion label="Loading Escalations" className="flex flex-col gap-3">
        {[0, 1, 2].map((i) => (
          <div key={i} className="flex flex-col gap-3 rounded-lg border border-border bg-surface p-5">
            <Skeleton className="h-4 w-1/3" />
            <Skeleton className="h-3 w-1/2" />
            <div className="flex gap-2 pt-2">
              <Skeleton className="h-10 w-24 rounded-md" />
              <Skeleton className="h-10 w-32 rounded-md" />
            </div>
          </div>
        ))}
      </LoadingRegion>
    );
  }
  if (open.isError && !open.data) {
    return <SectionError what="the escalation queue" error={open.error} onRetry={() => void open.refetch()} />;
  }
  if (queue.length === 0 && decided.length === 0) {
    return (
      <EmptyState
        icon="inbox"
        title="No open escalations"
        action={<Link href="/admin/requests" className={buttonClasses("secondary")}>View recent requests</Link>}
      >
        Nothing needs a person right now. Requests the assistant can&apos;t decide will appear here.
      </EmptyState>
    );
  }

  const target = (r: AdminListItem, resolution: "approved" | "denied"): ResolveTarget => ({
    ref: r.ref,
    customer: r.customer_name,
    orderRef: r.order_ref,
    amountCents: r.amount_cents,
    resolution,
  });

  return (
    <div className="flex flex-col gap-6">
      <p className="text-body-sm text-ink-muted">
        {queue.length
          ? `${queue.length} open. New customer replies first, then oldest first. Approving or denying needs a note and a confirmation; the customer is told in chat.`
          : "All escalations are decided."}
      </p>

      {queue.length === 0 ? (
        <EmptyState icon="check-circle" title="Queue is clear">
          New escalations will appear here.
        </EmptyState>
      ) : (
        <ul className="flex flex-col gap-3">
          {queue.map((r) => {
            const d = detailOf(r.ref);
            const old = now.getTime() - new Date(r.created_at).getTime() > OLD_AFTER_MS;
            return (
              <li key={r.ref}
                className={cn(
                  "flex flex-col gap-3 rounded-lg bg-surface p-5 shadow-xs",
                  r.customer_replied_at ? "border-[1.5px] border-info-fg" : "border border-border",
                )}>
                {r.customer_replied_at ? (
                  <span className="inline-flex h-6 items-center gap-1.5 self-start rounded-full border border-info-border bg-info-bg pr-2.5 pl-2 text-caption font-semibold text-info-fg">
                    <Icon name="chat" size={14} strokeWidth={2.25} />
                    Customer replied {replyTime(r.customer_replied_at)} · awaiting your reply
                  </span>
                ) : null}
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="flex flex-col gap-0.5">
                    <h2 className="text-title-sm font-semibold">
                      {r.customer_name} ·{" "}
                      {r.amount_cents !== null ? (
                        <span className="font-mono tabular">{formatCents(r.amount_cents)}</span>
                      ) : (
                        <span className="font-normal text-ink-muted">no order identified</span>
                      )}
                    </h2>
                    <p className="font-mono text-caption text-ink-subtle">
                      {[r.ref, r.order_ref, r.reason_category ? REASON_LABELS[r.reason_category] : null].filter(Boolean).join(" · ")}
                    </p>
                  </div>
                  <span className={cn("inline-flex items-center gap-1.5 text-meta", old ? "font-medium text-escalated-fg" : "text-ink-muted")}>
                    <Icon name="clock" size={14} />
                    Waiting {formatAge(r.created_at, now)}
                  </span>
                </div>
                <p className="text-body-sm">
                  <span className="font-semibold">Why escalated: </span>
                  {whyEscalated(d) ?? <Skeleton className="inline-block h-3 w-1/2 align-middle" />}
                </p>
                {d ? <ReviewDraft review={d.review} createdAt={d.request.created_at} /> : <Skeleton className="h-16 rounded-lg" />}
                <div className="flex flex-wrap items-end justify-between gap-3 border-t border-muted pt-3">
                  <FlagChips flags={r.flags} disputed={r.disputed_at !== null} full />
                  <div role="group" aria-label="Your decision" className="ml-auto flex flex-wrap items-center gap-2">
                    <span className="text-caption font-semibold tracking-[0.05em] text-ink-subtle uppercase">Your decision</span>
                    {r.unread_from_customer > 0 ? (
                      <span className="rounded-full border border-info-border bg-info-bg px-2 py-0.5 text-caption font-semibold text-info-fg">
                        {plural(r.unread_from_customer, "new message")}
                      </span>
                    ) : null}
                    <Button variant="ghost" onClick={() => openDrawer(r.ref)}>
                      View details
                    </Button>
                    <Button icon="chat" onClick={() => openDrawer(r.ref, { reply: true })}>
                      Message customer
                    </Button>
                    <Button variant="danger-outline" onClick={() => setResolve(target(r, "denied"))}>
                      Deny
                    </Button>
                    <Button variant="primary" onClick={() => setResolve(target(r, "approved"))}>
                      {r.amount_cents !== null ? `Approve ${formatCents(r.amount_cents)}` : "Approve"}
                    </Button>
                  </div>
                </div>
              </li>
            );
          })}
        </ul>
      )}

      {decided.length ? (
        <section className="flex flex-col gap-3">
          <h2 className="text-caption font-semibold tracking-[0.06em] text-ink-muted uppercase">Recently decided</h2>
          <ul className="flex flex-col gap-2">
            {decided.map((r) => {
              const d = detailOf(r.ref);
              const ev = d?.timeline.find((e) => e.kind === "resolved");
              const draft = d?.review;
              return (
                <li key={r.ref} className="flex flex-col gap-1.5 rounded-lg border border-border bg-surface p-4">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <button type="button" onClick={() => openDrawer(r.ref)} className="text-left text-body-sm">
                      <span className="font-medium">{r.customer_name}</span> · <span className="font-mono tabular">{formatCents(r.amount_cents)}</span>{" "}
                      <span className="font-mono text-ink-muted underline decoration-border-control underline-offset-3">{r.ref}</span>
                    </button>
                    <StatusBadge state={r.state} size="sm" />
                  </div>
                  <p className="font-mono text-caption text-ink-subtle">
                    {r.state === "resolved_approved" ? "Approved" : "Denied"} by{" "}
                    <span className="font-semibold text-ink">{ev?.actor_name ?? draft?.resolved_by ?? "an admin"}</span>
                    {r.resolved_at ? ` · ${formatDateTime(r.resolved_at)}` : ""}
                  </p>
                  {ev ? <p className="text-body-sm">Note: {String(ev.payload.note ?? "")}</p> : null}
                  <p className="rounded-md bg-muted px-3 py-2 text-meta text-ink-muted">
                    AI review draft at the time (advisory):{" "}
                    {!draft
                      ? "None (decided before AI drafts)."
                      : draft.status === "drafted" && draft.draft
                        ? `Suggested ${draft.draft.suggested_resolution}. ${draft.draft.summary}`
                        : draft.status === "pending"
                          ? "Still drafting when decided."
                          : "Draft unavailable."}
                  </p>
                </li>
              );
            })}
          </ul>
        </section>
      ) : null}

      <ResolveDialog target={resolve} onClose={() => setResolve(null)} />
    </div>
  );
}
