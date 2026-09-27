"use client";

import { useEffect, useRef } from "react";

import { StatusBadge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { EmptyState, Skeleton } from "@/components/ui/Surface";
import type { ConversationSummary, Order, RequestSummary } from "@/lib/api-types";
import { cn } from "@/lib/cn";
import { customerRequests } from "@/lib/customer";
import { formatCents, formatDate } from "@/lib/format";

import { StartNewFooter } from "./parts";
import { RequestDetailView } from "./RequestTranscript";

type RequestsViewProps = {
  conversations: ConversationSummary[] | undefined;
  orders: Order[] | undefined;
  loading: boolean;
  error: boolean;
  onRetry: () => void;
  /** Row to focus and highlight (the one the customer just came back from). */
  focusRef: string | null;
  /** Request shown read-only instead of the list. */
  detailRef: string | null;
  onOpenDetail: (ref: string) => void;
  onBack: (ref: string) => void;
  onOpenChat: (conversationId: string) => void;
  onNewRequest: () => void;
};

function note(r: RequestSummary) {
  if (r.state === "escalated") return "with a specialist";
  return `decided ${formatDate(r.resolved_at ?? r.created_at)}`;
}

/** The customer's refund requests, newest first. Each opens its chat, read-only. */
export function RequestsView(props: RequestsViewProps) {
  const { conversations, loading, error, onRetry, focusRef, detailRef, onOpenDetail, onNewRequest } = props;
  const rows = customerRequests(conversations);
  const detail = detailRef ? rows.find((r) => r.request.ref === detailRef) : undefined;
  const focused = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    focused.current?.focus();
    focused.current?.scrollIntoView({ block: "nearest" });
  }, [focusRef, detailRef, rows.length]);

  return (
    <>
      {detail ? (
        <RequestDetailView
          key={detail.request.ref}
          conversationId={detail.conversationId}
          request={detail.request}
          orders={props.orders}
          conversations={conversations}
          onBack={() => props.onBack(detail.request.ref)}
          onOpenChat={props.onOpenChat}
        />
      ) : (
        <div className="flex min-h-0 flex-grow flex-col gap-2.5 overflow-y-auto px-4 py-4 [&>*]:shrink-0">
          <p className="text-meta text-ink-muted">Your refund requests, newest first. Open one to see the full chat.</p>
          {loading ? (
            <div aria-busy="true" className="flex flex-col gap-2">
              <span className="sr-only">Loading your requests</span>
              {[0, 1, 2].map((i) => (
                <Skeleton key={i} className="h-[84px] rounded-lg" />
              ))}
            </div>
          ) : error ? (
            <div role="alert" className="flex flex-col items-start gap-2 text-meta text-denied-fg">
              Couldn&apos;t load your requests.
              <Button size="sm" onClick={onRetry}>
                Try again
              </Button>
            </div>
          ) : rows.length === 0 ? (
            <EmptyState variant="dashed" icon="inbox" title="No requests yet">
              Requests you make in chat show up here with their status.
            </EmptyState>
          ) : (
            <ul className="flex flex-col gap-2.5">
              {rows.map(({ request: r }) => {
                const isFocused = focusRef === r.ref;
                return (
                  <li key={r.id}>
                    <button
                      ref={isFocused ? focused : undefined}
                      type="button"
                      onClick={() => onOpenDetail(r.ref)}
                      aria-label={`${r.item_name ?? "Request"} ${r.ref}. View the chat`}
                      className={cn(
                        "flex w-full flex-col gap-1 rounded-lg border px-3.5 py-3 text-left hover:border-border-strong",
                        isFocused ? "border-[1.5px] border-ink bg-selected" : "border-border bg-surface",
                      )}
                    >
                      <span className="flex items-center justify-between gap-2">
                        <span className="text-body-sm font-medium">{r.item_name ?? "Refund request"}</span>
                        <StatusBadge state={r.state} size="sm" />
                      </span>
                      <span className="font-mono text-caption text-ink-muted tabular">
                        {[r.ref, r.order_ref, r.amount_cents !== null ? formatCents(r.amount_cents) : null].filter(Boolean).join(" · ")}
                      </span>
                      <span className="flex justify-between gap-2 text-caption text-ink-subtle">
                        <span>
                          Requested {formatDate(r.created_at)} · {note(r)}
                        </span>
                        <span aria-hidden="true" className="font-medium text-focus">
                          View chat ›
                        </span>
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      )}
      <StartNewFooter onStart={onNewRequest} />
    </>
  );
}
