"use client";

import { useQuery } from "@tanstack/react-query";
import { useState, type KeyboardEvent } from "react";

import { PolicyProse } from "@/components/policy/PolicyProse";
import { Button, IconButton } from "@/components/ui/Button";
import { BrandMark } from "@/components/ui/Icon";
import { Dialog } from "@/components/ui/Overlay";
import { CountPill } from "@/components/ui/Tabs";
import type { ConversationSummary, Order, PublicPolicy } from "@/lib/api-types";
import { api } from "@/lib/bff";
import { cn } from "@/lib/cn";
import { customerRequests, initials } from "@/lib/customer";

import { ChatThreadView } from "./ChatThreadView";
import { useConversation } from "./RequestTranscript";
import { RequestsView } from "./RequestsView";

/** Which thread the widget shows: an existing conversation, or a new one (optionally for an order). */
export type ThreadTarget = { key: number; conversationId: string | null; orderId: string | null };

type ChatWidgetProps = {
  customerName: string;
  layout: "panel" | "sheet";
  target: ThreadTarget;
  view: "chat" | "requests";
  focusRef: string | null;
  detailRef: string | null;
  orders: { data: Order[] | undefined; isError: boolean };
  conversations: { data: ConversationSummary[] | undefined; isPending: boolean; isError: boolean; refetch: () => void };
  onView: (view: "chat" | "requests") => void;
  /** A request's read-only detail, or the list when `null`. */
  onShowRequest: (ref: string | null) => void;
  /** Back from a detail to the list, focusing that row. */
  onBackToList: (ref: string) => void;
  onOpenThread: (target: Omit<ThreadTarget, "key">) => void;
  onConversationCreated: (id: string) => void;
  onClose: () => void;
};

/**
 * The help chat: a 400×600 panel on desktop, a full-screen sheet below 640px.
 * Escape closes it; the page returns focus to whatever opened it.
 */
export function ChatWidget(props: ChatWidgetProps) {
  const { layout, target, view, orders, conversations, onView, onOpenThread, onClose } = props;
  const [policyOpen, setPolicyOpen] = useState(false);
  const [dismissed, setDismissed] = useState<ReadonlySet<string>>(new Set());
  const policy = useQuery({ queryKey: ["policy"], queryFn: () => api<PublicPolicy>("policy"), enabled: policyOpen });
  const requests = customerRequests(conversations.data);
  const requestCount = requests.length;
  const freshCount = (conversations.data ?? []).filter((c) => c.unread_count > 0).length;

  // A specialist's newest reply on a request the customer isn't looking at.
  const onScreen =
    view === "chat" ? target.conversationId : (requests.find((r) => r.request.ref === props.detailRef)?.conversationId ?? null);
  const replyKey = (c: ConversationSummary) => `${c.id}@${c.last_reply_at}`;
  const unseen = (conversations.data ?? [])
    .filter((c) => c.request && c.unread_count > 0 && c.id !== onScreen && !dismissed.has(replyKey(c)))
    .sort((a, b) => (b.last_reply_at ?? "").localeCompare(a.last_reply_at ?? ""))[0];

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    if (e.key === "Escape" && !policyOpen) {
      e.stopPropagation();
      onClose();
    }
  }

  return (
    <div
      id="wn-chat"
      role="dialog"
      aria-label="Order help chat"
      onKeyDown={onKeyDown}
      className={cn(
        "flex flex-col overflow-hidden bg-surface",
        layout === "panel"
          ? "h-[600px] max-h-[calc(100dvh-3rem)] w-[400px] rounded-xl border border-border shadow-lg"
          : "h-dvh w-full pt-[env(safe-area-inset-top)] pr-[env(safe-area-inset-right)] pl-[env(safe-area-inset-left)]",
      )}
    >
      <div className="flex items-center gap-3 border-b border-border px-4 pt-3 pb-0">
        <div className="flex flex-grow flex-col">
          <div className="flex items-center gap-3">
            <BrandMark size={32} radius={8} />
            <div className="flex flex-grow flex-col">
              <h2 className="text-title-sm font-semibold">Order help</h2>
              <p className="text-caption text-ink-subtle">Refunds and order problems</p>
            </div>
            <IconButton icon={layout === "panel" ? "close" : "chevron-down"} label="Close chat" size={44} onClick={onClose} />
          </div>
          <nav aria-label="Chat views" className="mt-2 flex gap-1">
            {(
              [
                ["chat", "Chat"],
                ["requests", "Your requests"],
              ] as const
            ).map(([v, label]) => (
              <button
                key={v}
                type="button"
                aria-current={view === v ? "page" : undefined}
                onClick={() => onView(v)}
                className={cn(
                  "-mb-px inline-flex h-10 items-center gap-1.5 border-b-2 px-2.5 text-body-sm",
                  view === v ? "border-ink font-semibold text-ink" : "border-transparent font-medium text-ink-muted hover:text-ink",
                )}
              >
                {label}
                {v === "requests" && freshCount > 0 ? (
                  <span className="h-5 min-w-5 rounded-full bg-info-fg px-1.5 text-center text-caption leading-5 font-semibold text-white">
                    {freshCount} new
                  </span>
                ) : null}
                {v === "requests" ? <CountPill active={view === v}>{requestCount}</CountPill> : null}
              </button>
            ))}
          </nav>
        </div>
      </div>

      {unseen ? (
        <ReplyBanner
          conversation={unseen}
          onView={() => props.onShowRequest(unseen.request!.ref)}
          onDismiss={() => setDismissed((d) => new Set(d).add(replyKey(unseen)))}
        />
      ) : null}

      {/* Hidden, not unmounted, on Your requests: an in-flight reply, a picked order and the scroll position survive. */}
      <div className={cn("flex min-h-0 flex-grow flex-col", view !== "chat" && "hidden")}>
        <ChatThreadView
          key={target.key}
          customerName={props.customerName}
          orders={orders.data}
          ordersError={orders.isError}
          conversations={conversations.data}
          conversationId={target.conversationId}
          preselectOrderId={target.orderId}
          large={layout === "sheet"}
          visible={view === "chat"}
          onConversationCreated={props.onConversationCreated}
          onNewThread={(orderId) => onOpenThread({ conversationId: null, orderId })}
          onShowRequest={props.onShowRequest}
          onShowPolicy={() => setPolicyOpen(true)}
        />
      </div>
      {view === "requests" ? (
        <RequestsView
          conversations={conversations.data}
          orders={orders.data}
          loading={conversations.isPending}
          error={conversations.isError}
          onRetry={conversations.refetch}
          focusRef={props.focusRef}
          detailRef={props.detailRef}
          onOpenDetail={props.onShowRequest}
          onBack={props.onBackToList}
          onNewRequest={() => onOpenThread({ conversationId: null, orderId: null })}
          large={layout === "sheet"}
        />
      ) : null}

      <Dialog
        open={policyOpen}
        onClose={() => setPolicyOpen(false)}
        title="Refund policy"
        description={policy.data ? `Version ${policy.data.version}. Generated from the rules our assistant applies.` : undefined}
        actions={
          <Button variant="primary" onClick={() => setPolicyOpen(false)}>
            Close
          </Button>
        }
      >
        <div className="max-h-[50dvh] overflow-y-auto">
          {policy.isPending ? (
            <p className="text-body-sm text-ink-muted">Loading the policy…</p>
          ) : policy.isError ? (
            <p role="alert" className="text-body-sm text-denied-fg">
              Couldn&apos;t load the refund policy.
            </p>
          ) : (
            <PolicyProse prose={policy.data.prose} compact />
          )}
        </div>
      </Dialog>
    </div>
  );
}

function ReplyBanner({ conversation: c, onView, onDismiss }: { conversation: ConversationSummary; onView: () => void; onDismiss: () => void }) {
  const detail = useConversation(c.id);
  const r = c.request!;
  const by = c.last_reply_by ?? "Support team";
  const title =
    r.state === "resolved_approved"
      ? `${by} approved ${r.ref}`
      : r.state === "resolved_denied"
        ? `${by} denied ${r.ref}`
        : `New message from ${by} · ${r.ref}`;
  const latest = detail.data?.messages.findLast((m) => m.role === "admin");
  return (
    <div role="status" className="flex shrink-0 items-start gap-2.5 border-b border-info-border bg-info-bg py-2.5 pr-2 pl-3.5">
      <span aria-hidden="true" className="inline-flex size-6 shrink-0 items-center justify-center rounded-full bg-info-fg text-[10px] font-semibold text-white">
        {initials(by)}
      </span>
      <div className="flex min-w-0 flex-grow flex-col gap-0.5">
        <span className="text-[13px] leading-[18px] font-semibold text-ink">{title}</span>
        <span className="truncate text-caption text-ink-muted">{latest?.body ?? ""}</span>
      </div>
      <button type="button" onClick={onView}
        className="h-8 shrink-0 rounded-md border border-info-fg bg-surface px-2.5 text-[13px] font-medium text-info-fg hover:bg-info-bg">
        View
      </button>
      <IconButton icon="close" label="Dismiss notification" size={32} onClick={onDismiss} />
    </div>
  );
}
