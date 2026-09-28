"use client";

import { useEffect, useRef, useState, type ReactNode } from "react";

import { Button } from "@/components/ui/Button";
import { EmptyState, Skeleton } from "@/components/ui/Surface";
import type { ConversationSummary, Message, Order } from "@/lib/api-types";
import { alreadyStored, customerRequests, isClosed, orderAvailability } from "@/lib/customer";
import { firstName, formatCents } from "@/lib/format";

import { Composer } from "./Composer";
import { OrderChips } from "./OrderChips";
import {
  BotBubble,
  NoteLine,
  NoticeCard,
  OrderBubble,
  RateLimitNotice,
  ReviewingCard,
  SignInAgainLink,
  StaffBubble,
  StartNewFooter,
  UserBubble,
  VerdictCard,
} from "./parts";
import { EarlierRequest } from "./RequestTranscript";
import { useChatThread } from "./useChatThread";

type ChatThreadViewProps = {
  customerName: string;
  orders: Order[] | undefined;
  ordersError: boolean;
  conversations: ConversationSummary[] | undefined;
  conversationId: string | null;
  preselectOrderId: string | null;
  large: boolean;
  onConversationCreated: (id: string) => void;
  onNewThread: (orderId: string | null) => void;
  onShowRequest: (ref: string) => void;
  onShowPolicy: () => void;
};

function orderSummary(order: Order) {
  return { amount: formatCents(order.total_cents), items: order.items.map((i) => i.name).join(", ") };
}

export function ChatThreadView(props: ChatThreadViewProps) {
  const { customerName, orders, conversations, preselectOrderId, large, onNewThread, onShowRequest, onShowPolicy } = props;
  const thread = useChatThread(props.conversationId, props.onConversationCreated);
  const { detail, phase, outgoing, notice, setNotice, draft, setDraft } = thread;
  const [picked, setPicked] = useState<string | null>(preselectOrderId);
  const logRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  const messages = detail.data?.messages ?? [];
  const request = detail.data?.request ?? null;
  const orderById = new Map((orders ?? []).map((o) => [o.id, o]));
  const customerMessages = messages.filter((m) => m.role === "customer");
  const started = customerMessages.length > 0 || outgoing !== null;
  // The order the thread is about so far: the latest one sent with a message.
  const sentOrderId = outgoing?.orderId ?? customerMessages.findLast((m) => m.order_id !== null)?.order_id ?? null;
  const orderSent = sentOrderId !== null;
  const pickedOrder = picked ? (orderById.get(picked) ?? null) : null;
  const busy = phase !== "idle";
  const closed = isClosed(request);
  const paused = notice?.kind === "rate" || notice?.kind === "expired";

  // Lift the rate-limit pause when its countdown ends.
  useEffect(() => {
    if (notice?.kind !== "rate") return;
    const t = window.setTimeout(() => setNotice(null), Math.max(0, notice.until - Date.now()));
    return () => window.clearTimeout(t);
  }, [notice, setNotice]);

  // Keep the newest entry in view.
  const entryCount = messages.length + (outgoing ? 1 : 0) + (busy ? 1 : 0) + (notice ? 1 : 0);
  useEffect(() => {
    const el = logRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [entryCount, detail.isSuccess]);

  function pick(order: Order) {
    setPicked(order.id);
    inputRef.current?.focus();
  }

  function send() {
    const text = draft.trim();
    if (!text) return;
    const orderId = picked && picked !== sentOrderId ? picked : null;
    void thread.send({ text, clientMsgId: crypto.randomUUID(), orderId, saved: false });
  }

  // An order picked before the first message: earlier requests on its other items.
  const earlier = (order: Order) => {
    const { markers, free } = orderAvailability(order, conversations);
    const known = customerRequests(conversations);
    return markers.flatMap((m) => {
      const conversationId = known.find((k) => k.request.id === m.request?.id)?.conversationId;
      return m.request && conversationId
        ? [<EarlierRequest key={`earlier-${m.request.id}`} conversationId={conversationId} request={m.request} order={order} free={free} onShowRequest={onShowRequest} />]
        : [];
    });
  };

  const entries: ReactNode[] = [];
  const greeting =
    orders && orders.length === 0
      ? "Hi there. How can we help?"
      : `Hi ${firstName(customerName)}. Which order do you need help with? Pick one, or just tell me what happened.`;
  entries.push(<BotBubble key="greeting">{greeting}</BotBubble>);

  if (props.conversationId && detail.isPending) {
    entries.push(
      <div key="loading" aria-busy="true" className="flex flex-col gap-3">
        <span className="sr-only">Loading conversation</span>
        <Skeleton className="ml-auto h-10 w-[62%] rounded-[10px_10px_4px_10px]" />
        <Skeleton className="ml-8 h-14 w-[70%] rounded-[10px_10px_10px_4px]" />
      </div>,
    );
  } else if (detail.isError) {
    entries.push(
      <NoticeCard key="load-error" tone="error" icon="alert-circle" title="Couldn't load this conversation"
        action={<Button size="sm" onClick={() => detail.refetch()}>Try again</Button>}>
        Your messages are saved. Check your connection and try again.
      </NoticeCard>,
    );
  }

  if (!started && !props.conversationId) {
    if (orders && orders.length === 0) {
      entries.push(
        <EmptyState key="no-orders" variant="dashed" icon="box" title="No orders yet" className="ml-8">
          Orders you place show up here so you can ask about them. Have a question anyway? Type it below.
        </EmptyState>,
      );
    } else if (pickedOrder) {
      const s = orderSummary(pickedOrder);
      entries.push(<OrderBubble key="picked" orderRef={pickedOrder.ref} amount={s.amount} items={s.items} />);
      entries.push(...earlier(pickedOrder));
      entries.push(
        <BotBubble key="picked-prompt">What went wrong with {pickedOrder.ref}? A sentence or two is enough.</BotBubble>,
      );
    } else if (orders) {
      entries.push(
        <OrderChips key="chips" orders={orders} conversations={conversations} selectedId={picked}
          disabled={busy} onPick={pick} onShowRequest={onShowRequest} />,
      );
    } else if (props.ordersError) {
      entries.push(
        <BotBubble key="orders-error">I couldn&apos;t load your orders just now. Describe the problem and include the order ID.</BotBubble>,
      );
    }
  }

  const seenOrders = new Set<string>();
  const orderLead = (orderId: string | null, key: string) => {
    if (!orderId || seenOrders.has(orderId)) return;
    seenOrders.add(orderId);
    const order = orderById.get(orderId);
    if (order) {
      const s = orderSummary(order);
      entries.push(<OrderBubble key={key} orderRef={order.ref} amount={s.amount} items={s.items} />);
    }
  };
  messages.forEach((m: Message) => {
    if (m.role === "customer") {
      orderLead(m.order_id, `order-${m.id}`);
      entries.push(<UserBubble key={m.id}>{m.body}</UserBubble>);
    } else if (m.assistant_kind === "verdict" && request) {
      entries.push(
        <VerdictCard key={m.id} id={m.id} body={m.body} request={request} animate={thread.animateId === m.id}
          onShowPolicy={onShowPolicy} onShowRequest={onShowRequest} />,
      );
    } else if (m.role === "system") {
      entries.push(<NoteLine key={m.id}>{m.body}</NoteLine>);
    } else if (m.role === "admin") {
      entries.push(<StaffBubble key={m.id}>{m.body}</StaffBubble>);
    } else {
      entries.push(<BotBubble key={m.id}>{m.body}</BotBubble>);
    }
  });

  if (outgoing && !alreadyStored(outgoing.messageId, messages)) {
    orderLead(outgoing.orderId, "order-outgoing");
    entries.push(
      <UserBubble key="outgoing" pending={!outgoing.saved}>
        {outgoing.text}
      </UserBubble>,
    );
  }

  if (busy && thread.startedAt !== null) {
    const ref = (outgoing?.orderId && orderById.get(outgoing.orderId)?.ref) || pickedOrder?.ref || null;
    entries.push(<ReviewingCard key="reviewing" startedAt={thread.startedAt} orderRef={ref} writing={phase === "replying"} />);
  }

  // Still clarifying, or asked to see the orders: offer them, or confirm a mid-thread pick.
  const lastAssistant = [...messages].reverse().find((m) => m.role === "assistant");
  const offerOrders =
    lastAssistant?.assistant_kind === "order_list" || (lastAssistant?.assistant_kind === "clarify" && !orderSent);
  if (started && !busy && !request && offerOrders && orders?.length) {
    if (pickedOrder && picked !== sentOrderId) {
      const s = orderSummary(pickedOrder);
      entries.push(<OrderBubble key="late-pick" orderRef={pickedOrder.ref} amount={s.amount} items={s.items} />);
      entries.push(...earlier(pickedOrder));
      entries.push(<BotBubble key="late-pick-prompt">Got it: {pickedOrder.ref}. Send a short message to continue.</BotBubble>);
    } else {
      entries.push(
        <OrderChips key="clarify-chips" orders={orders} conversations={conversations} selectedId={picked}
          disabled={busy} onPick={pick} onShowRequest={onShowRequest} />,
      );
    }
  }

  if (notice?.kind === "rate") entries.push(<RateLimitNotice key="rate" until={notice.until} />);
  if (notice?.kind === "expired") {
    const next = `/support${thread.conversationId ? `?chat=${thread.conversationId}` : "?chat=new"}`;
    entries.push(
      <NoticeCard key="expired" tone="error" icon="lock" title="Your session has ended"
        action={<SignInAgainLink href={`/login?reason=expired&next=${encodeURIComponent(next)}`} />}>
        You were signed out. Your conversation and unsent message are saved.
      </NoticeCard>,
    );
  }
  if (notice?.kind === "error") {
    const retry = notice.retry;
    entries.push(
      <NoticeCard key="error" tone="error" icon="alert-circle" title="We couldn't finish reviewing your request"
        action={<Button size="sm" icon="refresh" onClick={() => thread.retry(retry)}>Try again</Button>}>
        {retry.saved
          ? "Something went wrong on our side. Your message is saved, but no decision was made yet."
          : "Your message didn't reach us. Nothing was submitted."}
      </NoticeCard>,
    );
  }

  const placeholder = request
    ? "Add details for the specialist"
    : pickedOrder && !orderSent
      ? "Describe what went wrong"
      : "Describe the problem, or pick an order above";
  const hint =
    notice?.kind === "rate"
      ? "Sending is paused for a moment."
      : notice?.kind === "expired"
        ? "Sign in again to send."
        : phase === "replying"
          ? "Writing the reply…"
          : busy
            ? "Waiting for the review to finish…"
            : "Enter to send · Shift+Enter for a new line";

  return (
    <>
      <div ref={logRef} role="log" aria-live="polite" aria-label="Conversation"
        className="flex min-h-0 flex-grow flex-col gap-3 overflow-y-auto px-4 py-4 [&>*]:shrink-0">
        {entries}
      </div>
      {closed ? (
        <StartNewFooter caption="This request is closed. The full chat is saved in Your requests." onStart={() => onNewThread(null)} />
      ) : (
        <Composer ref={inputRef} value={draft} onChange={setDraft} onSend={send} placeholder={placeholder}
          blocked={busy || paused} hint={hint} large={large} />
      )}
    </>
  );
}
