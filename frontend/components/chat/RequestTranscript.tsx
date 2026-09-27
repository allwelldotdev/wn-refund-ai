"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useId, useRef, useState, type ReactNode } from "react";

import { StatusBadge, stateLabel } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Textarea } from "@/components/ui/Field";
import { Icon } from "@/components/ui/Icon";
import { Skeleton } from "@/components/ui/Surface";
import type { ConversationDetail, ConversationSummary, Message, Order, RequestSummary } from "@/lib/api-types";
import { api, isApiError } from "@/lib/bff";
import { cn } from "@/lib/cn";
import { orderAvailability } from "@/lib/customer";
import { formatCents, formatDate } from "@/lib/format";

/** Same cache entry as the live thread, so opening either reuses the other. */
function useConversation(id: string) {
  return useQuery({
    queryKey: ["conversation", id],
    queryFn: () => api<ConversationDetail>(`conversations/${id}`),
  });
}

/**
 * The verdict as it was given: a disputed request was denied first, and one a
 * person decided otherwise was escalated first.
 */
function givenVerdict(r: RequestSummary): "approved" | "denied" | "escalated" {
  if (r.disputed_at) return "denied";
  if (r.state === "resolved_approved" || r.state === "resolved_denied") return "escalated";
  return r.state;
}

const VERDICT_BOX: Record<"approved" | "denied" | "escalated", string> = {
  approved: "border-approved-border [&>div]:bg-approved-bg [&>div]:text-approved-fg",
  denied: "border-denied-border [&>div]:bg-denied-bg [&>div]:text-denied-fg",
  escalated: "border-escalated-border [&>div]:bg-escalated-bg [&>div]:text-escalated-fg",
};

function Entry({ m, request }: { m: Message; request: RequestSummary }) {
  if (m.role === "customer") {
    return (
      <li className="max-w-[85%] self-end rounded-[12px_12px_4px_12px] bg-primary px-3 py-2 text-meta whitespace-pre-wrap text-white [overflow-wrap:anywhere]">
        <span className="sr-only">You: </span>
        {m.body}
      </li>
    );
  }
  if (m.role === "system") {
    return (
      <li className="rounded-md border border-dashed border-border-control px-3 py-2 text-center text-caption text-ink-muted [overflow-wrap:anywhere]">
        {m.body}
      </li>
    );
  }
  if (m.assistant_kind === "verdict") {
    const given = givenVerdict(request);
    return (
      <li className={cn("overflow-hidden rounded-lg border", VERDICT_BOX[given])}>
        <div className="flex items-center justify-between gap-2 border-b border-inherit px-3 py-2">
          <span className="text-meta font-semibold">{stateLabel(given)}</span>
          <span className="font-mono text-caption tabular">Ref {request.ref}</span>
        </div>
        <p className="px-3 py-2.5 text-meta whitespace-pre-wrap [overflow-wrap:anywhere]">{m.body}</p>
      </li>
    );
  }
  return (
    <li className="max-w-[88%] self-start rounded-[12px_12px_12px_4px] bg-muted px-3 py-2 text-meta whitespace-pre-wrap [overflow-wrap:anywhere]">
      <span className="sr-only">Assistant: </span>
      {m.body}
    </li>
  );
}

function InfoLine({ icon, children }: { icon: "info-circle" | "lock"; children: ReactNode }) {
  return (
    <p
      className={cn(
        "flex items-start gap-2 rounded-md border border-border px-3 py-2.5 text-meta text-ink-muted",
        icon === "info-circle" && "bg-canvas",
      )}
    >
      <Icon name={icon} size={icon === "lock" ? 16 : 14} className="mt-0.5 shrink-0" />
      <span>{children}</span>
    </p>
  );
}

type RequestDetailViewProps = {
  conversationId: string;
  request: RequestSummary;
  orders: Order[] | undefined;
  conversations: ConversationSummary[] | undefined;
  onBack: () => void;
  onOpenChat: (conversationId: string) => void;
};

/** One request and its chat, read-only. Only an escalated request can still take details, in the chat. */
export function RequestDetailView({ conversationId, request: r, orders, conversations, onBack, onOpenChat }: RequestDetailViewProps) {
  const detail = useConversation(conversationId);
  const order = orders?.find((o) => o.ref === r.order_ref);
  const rest = order ? orderAvailability(order, conversations).free : [];
  const byPerson = r.state === "resolved_approved" || r.state === "resolved_denied";
  return (
    <div className="flex min-h-0 flex-grow flex-col gap-3 overflow-y-auto px-4 pt-3 pb-4 [&>*]:shrink-0">
      <button type="button" onClick={onBack}
        className="-ml-1 inline-flex min-h-9 items-center gap-1 self-start rounded-md pr-2 pl-1 text-meta font-medium text-ink">
        <Icon name="chevron-left" size={16} />
        All requests
      </button>
      <div className="flex flex-col gap-1">
        <div className="flex items-start justify-between gap-2">
          <h3 className="text-lead font-semibold">{r.item_name ?? "Refund request"}</h3>
          <StatusBadge state={r.state} size="sm" />
        </div>
        <span className="font-mono text-caption text-ink-muted tabular">
          {[r.ref, r.order_ref, r.amount_cents !== null ? formatCents(r.amount_cents) : null, `Requested ${formatDate(r.created_at)}`]
            .filter(Boolean)
            .join(" · ")}
        </span>
      </div>
      <h4 className="mt-1 text-caption font-semibold tracking-[0.06em] text-ink-muted uppercase">Conversation · read-only</h4>
      {detail.isPending ? (
        <div aria-busy="true" className="flex flex-col gap-2">
          <span className="sr-only">Loading the conversation</span>
          <Skeleton className="ml-auto h-10 w-[62%]" />
          <Skeleton className="h-16" />
        </div>
      ) : detail.isError ? (
        <div role="alert" className="flex flex-col items-start gap-2 text-meta text-denied-fg">
          Couldn&apos;t load this conversation.
          <Button size="sm" onClick={() => detail.refetch()}>
            Try again
          </Button>
        </div>
      ) : (
        <ol role="log" aria-label={`Conversation for ${r.ref}`} className="flex flex-col gap-2.5">
          {detail.data.messages.map((m) => (
            <Entry key={m.id} m={m} request={r} />
          ))}
        </ol>
      )}
      {order && rest.length ? (
        <InfoLine icon="info-circle">
          The rest of {order.ref} ({rest.map((i) => i.name).join(", ")}) can still get its own refund request from the chat.
        </InfoLine>
      ) : null}
      {r.state === "escalated" ? (
        <div className="flex flex-col items-start gap-2 rounded-lg border border-border bg-canvas px-3.5 py-3">
          <p className="text-meta text-ink-muted">A support specialist is reviewing this request. You can add details for them in the chat.</p>
          <Button size="sm" onClick={() => onOpenChat(conversationId)}>
            Add details in chat
          </Button>
        </div>
      ) : byPerson ? (
        <InfoLine icon="lock">A support specialist reviewed this request. This decision is final.</InfoLine>
      ) : r.can_dispute ? (
        <DisputeCard conversationId={conversationId} request={r} />
      ) : r.state === "denied" ? (
        <InfoLine icon="lock">This decision is final.</InfoLine>
      ) : null}
    </div>
  );
}

const MAX_REASON = 500;

/** Asks a person to review an automatic denial: once, after a confirm step, with an optional reason. */
function DisputeCard({ conversationId, request }: { conversationId: string; request: RequestSummary }) {
  const [asking, setAsking] = useState(false);
  const [reason, setReason] = useState("");
  const yes = useRef<HTMLButtonElement>(null);
  const reasonId = useId();
  const queryClient = useQueryClient();
  useEffect(() => {
    if (asking) yes.current?.focus();
  }, [asking]);
  const dispute = useMutation({
    mutationFn: () =>
      api<RequestSummary>(`conversations/${conversationId}/dispute`, { method: "POST", json: { reason: reason.trim() || null } }),
    onSuccess: () =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: ["conversations"] }),
        queryClient.invalidateQueries({ queryKey: ["conversation", conversationId] }),
      ]),
  });
  const error = dispute.error
    ? isApiError(dispute.error)
      ? dispute.error.message
      : "Your dispute didn't reach us. Please try again."
    : null;
  return (
    <div className="flex flex-col gap-2 rounded-lg border border-border bg-canvas px-3.5 py-3">
      <p className="text-body-sm font-semibold">Think we got this wrong?</p>
      <p className="text-meta text-ink-muted">This was decided automatically. A support specialist can review it and reply here.</p>
      {!asking ? (
        <Button className="self-start" onClick={() => setAsking(true)}>
          Dispute this decision
        </Button>
      ) : (
        <div role="group" aria-label="Confirm dispute" className="flex flex-col gap-2 border-t border-border pt-2">
          <p className="text-meta font-medium">Send {request.ref} to a specialist for review?</p>
          <label htmlFor={reasonId} className="text-caption text-ink-muted">
            Why do you think it&apos;s wrong? (optional)
          </label>
          <Textarea id={reasonId} rows={2} maxLength={MAX_REASON} value={reason} onChange={(e) => setReason(e.target.value)}
            disabled={dispute.isPending} />
          {error ? (
            <p role="alert" className="text-meta text-denied-fg">
              {error}
            </p>
          ) : null}
          <div className="flex gap-2">
            <Button variant="ghost" disabled={dispute.isPending} onClick={() => setAsking(false)}>
              Cancel
            </Button>
            <Button ref={yes} variant="primary" loading={dispute.isPending} onClick={() => dispute.mutate()}>
              Yes, send for review
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

type EarlierRequestProps = {
  conversationId: string;
  request: RequestSummary;
  order: Order;
  free: Order["items"];
  onShowRequest: (ref: string) => void;
};

/** Picking an order with an earlier request on one of its items: that request's chat, and what is still open. */
export function EarlierRequest({ conversationId, request: r, order, free, onShowRequest }: EarlierRequestProps) {
  const detail = useConversation(conversationId);
  const names = free.map((i) => i.name).join(", ");
  // The decision and the message that led to it; the chat may hold other turns.
  const messages = detail.data?.messages ?? [];
  const at = messages.findIndex((m) => m.assistant_kind === "verdict");
  const asked = at > 0 ? messages.slice(0, at).findLast((m) => m.role === "customer") : undefined;
  const excerpt = at >= 0 ? [asked, messages[at]].filter((m) => m !== undefined) : messages.slice(-2);
  return (
    <section aria-label={`Earlier request ${r.ref} on this order`}
      className="flex animate-wn-in flex-col gap-2 rounded-lg border border-border bg-canvas p-3">
      <div className="flex items-center justify-between gap-2">
        <span className="text-caption font-semibold tracking-[0.06em] text-ink-muted uppercase">Earlier request · {formatDate(r.created_at)}</span>
        <button type="button" onClick={() => onShowRequest(r.ref)} className="link min-h-8 px-0.5 font-mono text-caption">
          {r.ref}
        </button>
      </div>
      {excerpt.length ? (
        <ol className="flex flex-col gap-1.5">
          {excerpt.map((m) =>
            m.role === "customer" ? (
              <li key={m.id} className="max-w-[85%] self-end rounded-[10px_10px_4px_10px] bg-ink-muted px-2.5 py-1.5 text-meta text-white [overflow-wrap:anywhere]">
                <span className="sr-only">You: </span>
                {m.body}
              </li>
            ) : (
              <li key={m.id} className="max-w-[88%] self-start rounded-[10px_10px_10px_4px] border border-border bg-surface px-2.5 py-1.5 text-meta [overflow-wrap:anywhere]">
                <span className="sr-only">Assistant: </span>
                {m.body}
              </li>
            ),
          )}
        </ol>
      ) : null}
      <div className="flex flex-wrap items-center gap-2 rounded-md border border-border bg-surface px-2.5 py-2">
        <StatusBadge state={r.state} size="sm" />
        <span className="text-meta">{r.item_name}</span>
      </div>
      <p className="flex items-start gap-1.5 text-meta text-ink-muted">
        <Icon name="info-circle" size={14} className="mt-0.5 shrink-0" />
        <span>
          This chat is still open for the rest of {order.ref}: {names}. If something went wrong with {free.length === 1 ? "it" : "those items"} too,
          you can ask for a refund below.
        </span>
      </p>
    </section>
  );
}
