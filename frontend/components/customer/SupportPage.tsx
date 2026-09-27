"use client";

import { useQuery } from "@tanstack/react-query";
import { useRouter, useSearchParams } from "next/navigation";
import { useCallback, useRef, useState, useSyncExternalStore } from "react";

import { ChatWidget, type ThreadTarget } from "@/components/chat/ChatWidget";
import { Chip } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { BrandMark, Icon } from "@/components/ui/Icon";
import { Alert, Avatar, EmptyState, Skeleton } from "@/components/ui/Surface";
import { Pagination, TD, TH, THead, TR, Table } from "@/components/ui/Table";
import type { ConversationSummary, Order, Principal } from "@/lib/api-types";
import { api } from "@/lib/bff";
import { customerRequests, fulfilment, orderAvailability } from "@/lib/customer";
import { formatCents, formatDate, formatShortDate } from "@/lib/format";

const noSubscription = () => () => {};
const PAGE_SIZE = 10;

function subscribeWide(cb: () => void) {
  const mq = window.matchMedia("(min-width: 640px)");
  mq.addEventListener("change", cb);
  return () => mq.removeEventListener("change", cb);
}

/** My orders, plus the help chat. The widget's state lives in the URL so a reload restores it. */
export function SupportPage({ principal }: { principal: Principal }) {
  const router = useRouter();
  const params = useSearchParams();
  const mounted = useSyncExternalStore(noSubscription, () => true, () => false);
  const wide = useSyncExternalStore(subscribeWide, () => window.matchMedia("(min-width: 640px)").matches, () => true);

  const orders = useQuery({ queryKey: ["orders"], queryFn: () => api<Order[]>("orders") });
  const conversations = useQuery({
    queryKey: ["conversations"],
    queryFn: () => api<ConversationSummary[]>("conversations"),
  });

  const initialChat = params.get("chat");
  const initialView = params.get("view") === "requests" ? "requests" : "chat";
  const [open, setOpen] = useState(initialChat !== null || params.get("view") === "requests");
  const [view, setView] = useState<"chat" | "requests">(initialView);
  const [focusRef, setFocusRef] = useState<string | null>(null);
  const [detailRef, setDetailRef] = useState<string | null>(params.get("req"));
  const [page, setPage] = useState(1);
  const [target, setTarget] = useState<ThreadTarget>({
    key: 0,
    conversationId: initialChat && initialChat !== "new" ? initialChat : null,
    orderId: params.get("order"),
  });
  const opener = useRef<HTMLElement | null>(null);
  const launcher = useRef<HTMLButtonElement>(null);

  const writeUrl = useCallback(
    (next: { open: boolean; view: "chat" | "requests"; conversationId: string | null; orderId: string | null; req?: string | null }) => {
      const q = new URLSearchParams();
      if (next.open) {
        q.set("chat", next.conversationId ?? "new");
        if (!next.conversationId && next.orderId) q.set("order", next.orderId);
        if (next.view === "requests") q.set("view", "requests");
        if (next.view === "requests" && next.req) q.set("req", next.req);
      }
      router.replace(q.size ? `/support?${q}` : "/support", { scroll: false });
    },
    [router],
  );

  function remember() {
    opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  }

  function openThread(t: Omit<ThreadTarget, "key">, nextView: "chat" | "requests" = "chat") {
    if (!open) remember();
    setTarget((prev) => ({ key: prev.key + 1, ...t }));
    setView(nextView);
    setOpen(true);
    writeUrl({ open: true, view: nextView, ...t });
  }

  function openLauncher() {
    remember();
    const latest = conversations.data?.[0];
    const t = { conversationId: latest?.id ?? null, orderId: null };
    setTarget((prev) => (prev.conversationId === t.conversationId && prev.orderId === null ? prev : { key: prev.key + 1, ...t }));
    setView("chat");
    setOpen(true);
    writeUrl({ open: true, view: "chat", ...t });
  }

  /** Your requests: the list, or one request's read-only detail. */
  function showRequests(ref: string | null, focus: string | null = null) {
    if (!open) remember();
    setView("requests");
    setDetailRef(ref);
    setFocusRef(focus);
    setOpen(true);
    writeUrl({ open: true, view: "requests", conversationId: target.conversationId, orderId: target.orderId, req: ref });
  }

  function close() {
    setOpen(false);
    setFocusRef(null);
    setDetailRef(null);
    writeUrl({ open: false, view: "chat", conversationId: null, orderId: null });
    const back = opener.current && document.contains(opener.current) ? opener.current : launcher.current;
    window.setTimeout(() => back?.focus(), 0);
  }

  async function signOut() {
    try {
      await api("auth/logout", { method: "POST" });
    } finally {
      window.location.replace("/login");
    }
  }

  const requestCount = customerRequests(conversations.data).length;

  return (
    <div className="min-h-dvh">
      <header className="flex h-14 items-center justify-between gap-3 border-b border-border bg-surface pr-2 pl-4 sm:h-16 sm:px-16">
        <div className="flex items-center gap-2.5">
          <BrandMark />
          <span className="text-title-sm font-semibold tracking-[-0.01em]">Worknoon Support</span>
        </div>
        <div className="flex items-center gap-3">
          <Avatar name={principal.name} />
          <span className="text-body-sm">
            <span className="sr-only">Signed in as </span>
            <span className="hidden sm:inline">{principal.name}</span>
            <span className="sm:hidden">{abbreviate(principal.name)}</span>
          </span>
          <button type="button" onClick={signOut} className="link min-h-11 px-2 text-body-sm font-medium">
            Sign out
          </button>
        </div>
      </header>

      <main className="mx-auto flex max-w-[1120px] flex-col gap-6 px-4 pt-5 pb-[104px] sm:px-16 sm:pt-10 sm:pb-[120px]">
        <div className="flex flex-wrap items-end justify-between gap-3">
          <div className="flex flex-col gap-1">
            <h1 className="text-[22px] leading-[30px] font-semibold sm:text-title-lg">My orders</h1>
            <p className="text-body-sm text-ink-muted">Your orders, newest first.</p>
          </div>
          <button type="button" onClick={() => showRequests(null)} className="link min-h-11 text-body-sm font-medium">
            Your refund requests ({requestCount})
          </button>
        </div>

        {orders.isPending ? (
          <div aria-busy="true" className="flex flex-col gap-2 rounded-lg border border-border bg-surface p-5">
            <span className="sr-only">Loading your orders</span>
            {[0, 1, 2, 3].map((i) => (
              <Skeleton key={i} className="h-10" />
            ))}
          </div>
        ) : orders.isError ? (
          <Alert tone="error" title="Couldn't load your orders" action={<Button size="sm" onClick={() => orders.refetch()}>Try again</Button>}>
            You can still describe the problem in the help chat and include the order ID.
          </Alert>
        ) : orders.data.length === 0 ? (
          <EmptyState icon="box" title="No orders yet">
            Orders you place show up here so you can ask about them.
          </EmptyState>
        ) : (
          <div className="flex flex-col rounded-lg sm:border sm:border-border sm:bg-surface sm:shadow-xs">
            {wide ? (
              <OrdersTable orders={pageOf(orders.data, page)} conversations={conversations.data} onHelp={openThread} onShowRequest={showRequests} />
            ) : (
              <OrdersList orders={pageOf(orders.data, page)} conversations={conversations.data} onHelp={openThread} onShowRequest={showRequests} />
            )}
            {orders.data.length > PAGE_SIZE ? (
              <Pagination page={page} pageSize={PAGE_SIZE} total={orders.data.length} onPage={setPage}
                label="Orders pages" noun="orders" compact={!wide} />
            ) : null}
          </div>
        )}
      </main>

      {!open ? (
        <button
          ref={launcher}
          id="wn-launcher"
          type="button"
          aria-expanded="false"
          aria-controls="wn-chat"
          onClick={openLauncher}
          className="fixed right-4 bottom-[calc(1rem+env(safe-area-inset-bottom))] z-10 inline-flex h-12 items-center gap-2 rounded-full bg-primary px-5 text-body-sm font-medium text-white shadow-md hover:bg-primary-hover sm:right-6 sm:bottom-6"
        >
          <Icon name="chat" size={18} />
          Need help with an order?
        </button>
      ) : null}

      {open && mounted ? (
        <div className={wide ? "fixed right-6 bottom-6 z-20" : "fixed inset-0 z-20 bg-surface"}>
          <ChatWidget
            customerName={principal.name}
            layout={wide ? "panel" : "sheet"}
            target={target}
            view={view}
            focusRef={focusRef}
            detailRef={detailRef}
            orders={{ data: orders.data, isError: orders.isError }}
            conversations={{
              data: conversations.data,
              isPending: conversations.isPending,
              isError: conversations.isError,
              refetch: () => void conversations.refetch(),
            }}
            onShowRequest={(ref) => showRequests(ref)}
            onBackToList={(ref) => showRequests(null, ref)}
            onView={(v) => {
              if (v === "requests") showRequests(null);
              else {
                setView("chat");
                writeUrl({ open: true, view: "chat", conversationId: target.conversationId, orderId: target.orderId });
              }
            }}
            onOpenThread={(t) => openThread(t)}
            onConversationCreated={(id) => writeUrl({ open: true, view: "chat", conversationId: id, orderId: null })}
            onClose={close}
          />
        </div>
      ) : null}
    </div>
  );
}

function pageOf(orders: Order[], page: number) {
  return orders.slice((page - 1) * PAGE_SIZE, page * PAGE_SIZE);
}

function abbreviate(name: string) {
  const [first, ...rest] = name.split(/\s+/);
  const last = rest.at(-1);
  return last ? `${first} ${last[0]}.` : first;
}

type OrdersViewProps = {
  orders: Order[];
  conversations: ConversationSummary[] | undefined;
  onHelp: (t: Omit<ThreadTarget, "key">) => void;
  onShowRequest: (ref: string | null) => void;
};

function HelpAction({ order, conversations, onHelp, onShowRequest }: Omit<OrdersViewProps, "orders"> & { order: Order }) {
  const { blocked, markers } = orderAvailability(order, conversations);
  if (blocked) {
    const ref = markers.find((m) => m.request)?.request?.ref ?? null;
    return (
      <button type="button" aria-label={`View refund request for ${order.ref}`} onClick={() => onShowRequest(ref)}
        className="link min-h-11 text-body-sm font-medium whitespace-nowrap">
        View refund request
      </button>
    );
  }
  return (
    <button type="button" aria-label={`Get help with order ${order.ref}`} onClick={() => onHelp({ conversationId: null, orderId: order.id })}
      className="link min-h-11 text-body-sm font-medium whitespace-nowrap">
      Get help with this order
    </button>
  );
}

function Items({ order }: { order: Order }) {
  return (
    <span className="flex flex-wrap items-center gap-x-1.5 gap-y-1">
      {order.items.map((item, i) => (
        <span key={item.id} className="inline-flex items-center gap-1.5">
          {item.name}
          {item.final_sale ? <Chip tone="neutral">Final sale</Chip> : null}
          {i < order.items.length - 1 ? "," : ""}
        </span>
      ))}
    </span>
  );
}

function Delivery({ order }: { order: Order }) {
  const f = fulfilment(order, formatShortDate);
  return (
    <span className="inline-flex items-center gap-1.5 text-ink-muted">
      <Icon name={f.icon} size={16} className="text-ink-subtle" />
      {f.label}
    </span>
  );
}

function OrdersTable({ orders, conversations, onHelp, onShowRequest }: OrdersViewProps) {
  return (
    <div>
      <Table caption="Your orders, newest first">
        <THead>
          <TH>Order</TH>
          <TH>Date</TH>
          <TH>Items</TH>
          <TH numeric>Total</TH>
          <TH>Delivery</TH>
          <TH>
            <span className="sr-only">Help</span>
          </TH>
        </THead>
        <tbody>
          {orders.map((o) => (
            <TR key={o.id}>
              <TD className="font-mono text-mono font-medium whitespace-nowrap">{o.ref}</TD>
              <TD className="font-mono text-mono whitespace-nowrap text-ink-muted tabular">{formatDate(o.placed_at)}</TD>
              <TD>
                <Items order={o} />
              </TD>
              <TD numeric>{formatCents(o.total_cents)}</TD>
              <TD className="whitespace-nowrap">
                <Delivery order={o} />
              </TD>
              <TD className="text-right">
                <HelpAction order={o} conversations={conversations} onHelp={onHelp} onShowRequest={onShowRequest} />
              </TD>
            </TR>
          ))}
        </tbody>
      </Table>
    </div>
  );
}

function OrdersList({ orders, conversations, onHelp, onShowRequest }: OrdersViewProps) {
  return (
    <ul className="flex flex-col gap-3" aria-label="Your orders, newest first">
      {orders.map((o) => (
        <li key={o.id} className="flex flex-col gap-2 rounded-lg border border-border bg-surface p-4">
          <div className="flex items-center justify-between font-mono text-mono">
            <span className="font-medium">{o.ref}</span>
            <span className="text-ink-muted tabular">{formatDate(o.placed_at)}</span>
          </div>
          <div className="text-lead">
            <Items order={o} />
          </div>
          <div className="flex items-center justify-between gap-3 text-body-sm">
            <Delivery order={o} />
            <span className="font-mono text-mono tabular">{formatCents(o.total_cents)}</span>
          </div>
          <HelpAction order={o} conversations={conversations} onHelp={onHelp} onShowRequest={onShowRequest} />
        </li>
      ))}
    </ul>
  );
}
