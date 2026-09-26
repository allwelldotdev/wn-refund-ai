"use client";

import { StatusBadge } from "@/components/ui/Badge";
import { TD, TH, THead, TR, Table } from "@/components/ui/Table";
import type { AdminListItem } from "@/lib/api-types";
import { cn } from "@/lib/cn";
import { REASON_LABELS, formatCents, formatRelativeDayTime } from "@/lib/format";

import { FlagChips } from "./common";

type Props = {
  items: AdminListItem[];
  caption: string;
  selected: string | null;
  onOpen: (ref: string) => void;
  /** The queue shows order and reason columns (desktop) and short flag chips with tooltips. */
  variant: "overview" | "queue";
};

/**
 * Requests as a table from 768px (order and reason columns from 1280px), and
 * as a card list below. Rows are clickable; the reference is the real button.
 */
export function RequestTable({ items, caption, selected, onOpen, variant }: Props) {
  const queue = variant === "queue";
  return (
    <>
      <div className="hidden overflow-hidden rounded-lg border border-border bg-surface shadow-xs md:block">
        <Table caption={caption}>
          <THead>
            <TH>Reference</TH>
            <TH>Customer</TH>
            {queue ? <TH className="hidden xl:table-cell">Order</TH> : null}
            <TH numeric>Amount</TH>
            {queue ? <TH className="hidden xl:table-cell">Reason</TH> : null}
            <TH>Verdict</TH>
            <TH>Flags</TH>
            <TH numeric sorted={queue ? "descending" : undefined}>
              Time
            </TH>
          </THead>
          <tbody>
            {items.map((r) => (
              <TR key={r.ref} selected={selected === r.ref} onActivate={() => onOpen(r.ref)}>
                <TD className="py-1.5">
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      onOpen(r.ref);
                    }}
                    className="min-h-10 font-mono text-mono font-medium text-ink underline decoration-border-control underline-offset-3"
                  >
                    {r.ref}
                  </button>
                </TD>
                <TD className="py-1.5">
                  <span className="flex flex-col">
                    <span className="font-medium">{r.customer_name}</span>
                    {queue ? <span className="text-caption text-ink-subtle">{r.customer_email}</span> : null}
                  </span>
                </TD>
                {queue ? <TD className="hidden py-1.5 font-mono text-mono xl:table-cell">{r.order_ref ?? "—"}</TD> : null}
                <TD numeric className="py-1.5">
                  {formatCents(r.amount_cents)}
                </TD>
                {queue ? (
                  <TD className="hidden py-1.5 text-ink-muted xl:table-cell">
                    {r.reason_category ? REASON_LABELS[r.reason_category] : "—"}
                  </TD>
                ) : null}
                <TD className="py-1.5">
                  <StatusBadge state={r.state} />
                </TD>
                <TD className="py-1.5">
                  <FlagChips flags={r.flags} full={!queue} />
                </TD>
                <TD numeric className="py-1.5 whitespace-nowrap">
                  {formatRelativeDayTime(r.created_at)}
                </TD>
              </TR>
            ))}
          </tbody>
        </Table>
      </div>

      <ul className="flex flex-col gap-2 md:hidden" aria-label={caption}>
        {items.map((r) => (
          <li key={r.ref}>
            <button
              type="button"
              onClick={() => onOpen(r.ref)}
              aria-current={selected === r.ref ? "true" : undefined}
              className={cn(
                "flex w-full flex-col gap-1.5 rounded-lg border bg-surface p-3.5 text-left",
                selected === r.ref ? "border-[1.5px] border-ink bg-selected" : "border-border",
              )}
            >
              <span className="flex items-center justify-between gap-2 text-lead">
                <span className="font-medium">{r.customer_name}</span>
                <span className="font-mono text-mono tabular">{formatCents(r.amount_cents)}</span>
              </span>
              {queue ? (
                <span className="text-meta text-ink-muted">
                  {[r.reason_category ? REASON_LABELS[r.reason_category] : null, r.order_ref].filter(Boolean).join(" · ")}
                </span>
              ) : null}
              <span className="flex flex-wrap items-center gap-1.5">
                <StatusBadge state={r.state} size="sm" />
                <FlagChips flags={r.flags} />
              </span>
              <span className="font-mono text-caption text-ink-subtle">
                {r.ref} · {formatRelativeDayTime(r.created_at)}
              </span>
            </button>
          </li>
        ))}
      </ul>
    </>
  );
}
