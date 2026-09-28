import type { ConversationSummary, Message, Order, OrderItem, RequestSummary } from "./api-types";

export type ItemMarker = {
  item: OrderItem;
  /** Approved (refunded), denied, or with a person (escalated). */
  kind: "approved" | "denied" | "review";
  label: string;
  request: RequestSummary | null;
};

/**
 * Requests a customer has made, newest first. The API has no separate list:
 * each conversation holds at most one request.
 */
export function customerRequests(conversations: ConversationSummary[] | undefined): Array<{
  conversationId: string;
  request: RequestSummary;
}> {
  return (conversations ?? [])
    .filter((c): c is ConversationSummary & { request: RequestSummary } => c.request !== null)
    .map((c) => ({ conversationId: c.id, request: c.request }))
    .sort((a, b) => b.request.created_at.localeCompare(a.request.created_at));
}

const MARKERS: Record<RequestSummary["state"], Pick<ItemMarker, "kind" | "label">> = {
  approved: { kind: "approved", label: "Approved" },
  resolved_approved: { kind: "approved", label: "Approved after review" },
  denied: { kind: "denied", label: "Denied" },
  resolved_denied: { kind: "denied", label: "Denied after review" },
  escalated: { kind: "review", label: "Under review" },
};

/**
 * Items that can't start a new request: every item that already has one, in
 * any state (the assistant would only report that request), and any item the
 * orders API marks refunded.
 */
export function itemMarkers(order: Order, conversations: ConversationSummary[] | undefined): ItemMarker[] {
  const requests = customerRequests(conversations).map((r) => r.request);
  const markers: ItemMarker[] = [];
  for (const item of order.items) {
    const request = requests.find((r) => r.order_ref === order.ref && r.item_name === item.name) ?? null;
    if (request) markers.push({ item, ...MARKERS[request.state], request });
    else if (item.active_refund) markers.push({ item, ...MARKERS.approved, request: null });
  }
  return markers;
}

/** Whether a re-read of the thread already holds the message being sent, so its in-flight copy isn't drawn twice. */
export function alreadyStored(messageId: string | undefined, messages: Pick<Message, "id">[]): boolean {
  return messageId !== undefined && messages.some((m) => m.id === messageId);
}

/** Answered requests are closed to new messages; an escalated one stays open. */
export function isClosed(request: RequestSummary | null): boolean {
  return request !== null && request.state !== "escalated";
}

/** Items still open to a new request, and whether the whole order is blocked. */
export function orderAvailability(order: Order, conversations: ConversationSummary[] | undefined) {
  const markers = itemMarkers(order, conversations);
  const blockedNames = new Set(markers.map((m) => m.item.id));
  const free = order.items.filter((i) => !blockedNames.has(i.id));
  return {
    markers,
    free,
    blocked: free.length === 0,
    freeCents: free.reduce((sum, i) => sum + i.amount_cents, 0),
  };
}

export type Fulfilment = { label: string; icon: "box" | "check-circle" | "calendar" | "clock" };

const PHYSICAL = new Set(["accessories", "subscriptions"]);
const BOOKINGS = new Set(["room bookings"]);

/** How far along an order is, from its stored status; older rows fall back to the kind of thing bought. */
export function fulfilment(order: Order, formatShortDate: (iso: string) => string): Fulfilment {
  const on = (iso: string | null) => (iso ? formatShortDate(iso) : "");
  switch (order.fulfilment) {
    case "delivered":
      return { label: `Delivered ${on(order.delivered_at)}`, icon: "box" };
    case "used":
      return { label: `Used ${on(order.delivered_at)}`, icon: "check-circle" };
    case "confirmed":
      return { label: order.starts_at ? `Confirmed, starts ${on(order.starts_at)}` : "Confirmed, not started", icon: "calendar" };
    case "active": {
      const since = on(order.starts_at ?? order.delivered_at);
      return { label: order.ends_at ? `Active since ${since}, until ${on(order.ends_at)}` : `Active since ${since}`, icon: "clock" };
    }
  }
  const category = (order.items[0]?.category ?? "").trim().toLowerCase();
  if (!order.delivered_at) {
    return order.status === "shipped"
      ? { label: "Shipped", icon: "box" }
      : { label: "Confirmed, not started", icon: "calendar" };
  }
  const when = formatShortDate(order.delivered_at);
  if (PHYSICAL.has(category)) return { label: `Delivered ${when}`, icon: "box" };
  if (BOOKINGS.has(category)) return { label: `Used ${when}`, icon: "check-circle" };
  return { label: `Active since ${when}`, icon: "clock" };
}
