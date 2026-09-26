import type { ConversationSummary, Order, OrderItem, RequestSummary } from "./api-types";

export type ItemMarker = {
  item: OrderItem;
  /** Refunded (approved), or under review by a person (escalated). */
  kind: "refunded" | "review";
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

/**
 * Items that can't start a new request: refunded ones (`active_refund`) and
 * ones waiting for a person. Denied items stay selectable, because the
 * policy's repeat-claim rule decides what a second request gets.
 */
export function itemMarkers(order: Order, conversations: ConversationSummary[] | undefined): ItemMarker[] {
  const requests = customerRequests(conversations).map((r) => r.request);
  const find = (item: OrderItem, states: RequestSummary["state"][]) =>
    requests.find((r) => r.order_ref === order.ref && r.item_name === item.name && states.includes(r.state)) ?? null;
  const markers: ItemMarker[] = [];
  for (const item of order.items) {
    if (item.active_refund) {
      markers.push({ item, kind: "refunded", request: find(item, ["approved", "resolved_approved"]) });
    } else {
      const open = find(item, ["escalated"]);
      if (open) markers.push({ item, kind: "review", request: open });
    }
  }
  return markers;
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

/** How an order was fulfilled, worded for the kind of thing bought. */
export function fulfilment(order: Order, formatShortDate: (iso: string) => string): Fulfilment {
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
