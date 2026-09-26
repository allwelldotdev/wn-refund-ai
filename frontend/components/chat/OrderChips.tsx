"use client";

import { Pill } from "@/components/ui/Badge";
import type { ConversationSummary, Order } from "@/lib/api-types";
import { cn } from "@/lib/cn";
import { orderAvailability } from "@/lib/customer";
import { formatCents, formatDate } from "@/lib/format";

type OrderChipsProps = {
  orders: Order[];
  conversations: ConversationSummary[] | undefined;
  selectedId: string | null;
  disabled: boolean;
  onPick: (order: Order) => void;
  onShowRequest: (requestRef: string) => void;
};

/**
 * The customer's orders as pickable chips. Orders whose items are all
 * refunded or under review can't be picked; their markers link to the
 * matching row in Your requests.
 */
export function OrderChips({ orders, conversations, selectedId, disabled, onPick, onShowRequest }: OrderChipsProps) {
  const rows = orders.map((order) => ({ order, ...orderAvailability(order, conversations) }));
  const anyOpen = rows.some((r) => !r.blocked);
  return (
    <div className="ml-8 flex animate-wn-in flex-col gap-2">
      <div role="group" aria-label="Your orders" className="flex flex-col gap-2">
        {rows.map(({ order, markers, free, blocked, freeCents }) => {
          const selected = selectedId === order.id;
          const markerId = `markers-${order.id}`;
          const names = (blocked ? order.items : free)
            .map((i) => (i.final_sale ? `${i.name} (final sale)` : i.name))
            .join(", ");
          return (
            <div key={order.id} className="flex flex-col gap-1.5">
              <button
                type="button"
                aria-pressed={selected}
                aria-describedby={markers.length ? markerId : undefined}
                disabled={disabled || blocked || (selectedId !== null && !selected)}
                onClick={() => onPick(order)}
                className={cn(
                  "flex min-h-11 w-full flex-col gap-0.5 rounded-lg px-3 py-2 text-left transition-colors duration-120 ease-standard",
                  blocked
                    ? "cursor-not-allowed border border-dashed border-border-control bg-canvas text-ink-muted"
                    : selected
                      ? "border-[1.5px] border-ink bg-surface"
                      : "border border-border-control bg-surface hover:border-border-strong",
                  !blocked && !selected && (disabled || selectedId !== null) && "cursor-not-allowed opacity-55 hover:border-border-control",
                )}
              >
                <span className="flex items-center justify-between gap-3 font-mono text-mono tabular">
                  <span className="font-medium">{order.ref}</span>
                  <span>{formatCents(blocked ? order.total_cents : freeCents)}</span>
                </span>
                <span className="text-meta">{names}</span>
                <span className="text-caption text-ink-subtle">
                  Ordered {formatDate(order.placed_at)}
                  {blocked ? " · not selectable" : ""}
                </span>
              </button>
              {markers.length ? (
                <ul id={markerId} className="flex flex-col gap-1 pl-1">
                  {markers.map((m) => {
                    const label = m.kind === "refunded" ? "Refunded" : "Under review";
                    const content = (
                      <>
                        <Pill tone={m.kind === "refunded" ? "approved" : "info"} icon={m.kind === "refunded" ? "check-circle" : "clock"} size="sm">
                          {label}
                        </Pill>
                        <span className="text-caption text-ink-muted">{m.item.name}</span>
                        {m.request ? <span className="font-mono text-caption text-focus underline underline-offset-2">{m.request.ref}</span> : null}
                      </>
                    );
                    return (
                      <li key={m.item.id}>
                        {m.request ? (
                          <button
                            type="button"
                            aria-label={`${label}: ${m.item.name}. View ${m.request.ref} in Your requests`}
                            onClick={() => onShowRequest(m.request!.ref)}
                            className="inline-flex min-h-7 flex-wrap items-center gap-1.5 rounded-sm text-left"
                          >
                            {content}
                          </button>
                        ) : (
                          <span className="inline-flex flex-wrap items-center gap-1.5">{content}</span>
                        )}
                      </li>
                    );
                  })}
                </ul>
              ) : null}
            </div>
          );
        })}
      </div>
      <p className="text-caption text-ink-subtle">
        {anyOpen ? "Not listed? Describe the problem below and include the order ID." : "No other orders are eligible right now."}
      </p>
    </div>
  );
}
