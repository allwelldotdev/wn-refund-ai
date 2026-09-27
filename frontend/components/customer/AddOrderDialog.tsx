"use client";

import { useMutation, useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";

import { Button, IconButton } from "@/components/ui/Button";
import { DatePicker } from "@/components/ui/DatePicker";
import { Field, SearchInput, Select, TextInput } from "@/components/ui/Field";
import { Icon } from "@/components/ui/Icon";
import { Skeleton } from "@/components/ui/Surface";
import { Tabs, tabPanelProps } from "@/components/ui/Tabs";
import type { Catalog, CatalogItem, CatalogKind, Fulfilment, NewOrderBody, Order, OrderPreview } from "@/lib/api-types";
import { api, isApiError } from "@/lib/bff";
import { cn } from "@/lib/cn";
import { addDays, clampDay, formatShortDay, today } from "@/lib/dates";
import { formatCents } from "@/lib/format";
import { useModal } from "@/lib/use-modal";

const MAX_AGE_DAYS = 60;
const MAX_START_DAYS = 120;
const RUNS: Array<[number, string]> = [
  [7, "1 week"],
  [30, "1 month"],
  [90, "3 months"],
  [365, "12 months"],
];

const STATUS: Record<Fulfilment, { label: string; help: string }> = {
  delivered: { label: "Delivered", help: "Physical items that have arrived" },
  used: { label: "Used", help: "Bookings and passes already used" },
  confirmed: { label: "Confirmed", help: "Booked, starts on a set date" },
  active: { label: "Active", help: "Ongoing plan or rental" },
};

const KIND_NOTE: Record<CatalogKind, string> = {
  booking: "Booking · used on the day",
  plan: "Plan · runs over time",
  deposit: "Deposit · for a start date",
  service: "Service",
  product: "Physical item · shipped",
};

const VERDICT_WORD = { approved: "Approved", denied: "Denied", escalated: "Escalated" } as const;

/** A status that fits what is in the order; the first item added sets it. */
function suggest(items: CatalogItem[]): Fulfilment | null {
  if (!items.length) return null;
  const kinds = new Set(items.map((i) => i.kind));
  if ([...kinds].every((k) => k === "product")) return "delivered";
  if ([...kinds].every((k) => k === "booking")) return "used";
  if (kinds.has("deposit")) return "confirmed";
  if (kinds.has("plan")) return "active";
  if (kinds.has("product")) return "delivered";
  return "used";
}

function maxQuantity(item: CatalogItem) {
  return item.hourly ? 12 : 20;
}

function lineName(item: CatalogItem, q: number) {
  if (item.hourly) return `${item.name} (${q} ${q === 1 ? "hr" : "hrs"})`;
  return q > 1 ? `${item.name} × ${q}` : item.name;
}

type Line = { id: string; quantity: number };

type AddOrderDialogProps = {
  open: boolean;
  customerName: string;
  onClose: () => void;
  onAdded: (order: Order) => void;
};

/** A simulated order to try the refund chat with (demo only). Nothing is charged. */
export function AddOrderDialog(props: AddOrderDialogProps) {
  return props.open ? <AddOrderForm {...props} /> : null;
}

function AddOrderForm({ customerName, onClose, onAdded }: AddOrderDialogProps) {
  const now = today();
  const catalog = useQuery({ queryKey: ["catalog"], queryFn: () => api<Catalog>("catalog") });
  const [placedOn, setPlacedOn] = useState(now);
  const [lines, setLines] = useState<Line[]>([]);
  const [q, setQ] = useState("");
  const [tab, setTab] = useState<CatalogItem["group"]>("workspace");
  const [status, setStatus] = useState<Fulfilment | "">("");
  const [deliveredOn, setDeliveredOn] = useState(now);
  const [startsOn, setStartsOn] = useState(addDays(now, 7));
  const [runsFor, setRunsFor] = useState(30);
  const [submitted, setSubmitted] = useState(false);
  const [serverErrors, setServerErrors] = useState<string[]>([]);
  const dialog = useRef<HTMLDivElement>(null);
  const summary = useRef<HTMLDivElement>(null);
  useModal(dialog, true, onClose);

  const groups = catalog.data?.groups ?? [];
  const byId = new Map(groups.flatMap((g) => g.items.map((i) => [i.id, i] as const)));
  const groupLabel = (item: CatalogItem) => groups.find((g) => g.key === item.group)?.label ?? "";
  const chosen = lines.flatMap((l) => {
    const item = byId.get(l.id);
    return item ? [{ line: l, item }] : [];
  });
  const total = chosen.reduce((s, { line, item }) => s + item.unit_cents * line.quantity, 0);
  const suggestion = suggest(chosen.map((c) => c.item));

  // Dependent dates follow the order date instead of going stale.
  const deliveredDay = clampDay(deliveredOn, placedOn, now);
  const startsDay = clampDay(startsOn, placedOn, addDays(now, MAX_START_DAYS));
  const runEnd = addDays(placedOn, runsFor);
  const ended = runEnd < now;

  const errors: Array<{ key: "items" | "status" | "run"; text: string }> = [];
  if (!chosen.length) errors.push({ key: "items", text: "Add at least one item." });
  if (!status) errors.push({ key: "status", text: "Choose a delivery status." });
  if (status === "active" && ended) {
    errors.push({ key: "run", text: "This period already ended. Pick a longer period or a later order date." });
  }
  const errorOf = (key: (typeof errors)[number]["key"]) => (submitted ? (errors.find((e) => e.key === key)?.text ?? null) : null);
  const showSummary = submitted && (errors.length > 0 || serverErrors.length > 0);

  const body: NewOrderBody = {
    placed_on: placedOn,
    items: lines.map((l) => ({ item: l.id, quantity: l.quantity })),
    fulfilment: status || null,
    delivered_on: status === "delivered" ? deliveredDay : null,
    starts_on: status === "confirmed" ? startsDay : null,
    runs_for_days: status === "active" ? runsFor : null,
  };
  const preview = useQuery({
    queryKey: ["order-preview", body],
    queryFn: () => api<OrderPreview>("orders/preview", { method: "POST", json: body }),
    enabled: errors.length === 0,
    placeholderData: (prev) => prev,
    staleTime: 30_000,
  });

  const create = useMutation({
    mutationFn: () => api<Order>("orders", { method: "POST", json: body }),
    onSuccess: onAdded,
    onError: (e) =>
      setServerErrors(
        isApiError(e) ? (e.fields.length ? e.fields.map((f) => f.message) : [e.message]) : ["The order wasn't added. Try again."],
      ),
  });

  useEffect(() => {
    if (showSummary) summary.current?.focus();
  }, [showSummary, serverErrors]);

  function add(item: CatalogItem) {
    setLines((ls) =>
      ls.some((l) => l.id === item.id)
        ? ls.map((l) => (l.id === item.id ? { ...l, quantity: Math.min(l.quantity + 1, maxQuantity(item)) } : l))
        : [...ls, { id: item.id, quantity: item.hourly ? 2 : 1 }],
    );
    if (!status) choose(suggest([...chosen.map((c) => c.item), item]) ?? "");
  }

  function setQuantity(id: string, quantity: number) {
    setLines((ls) => ls.map((l) => (l.id === id ? { ...l, quantity } : l)));
  }

  function choose(next: Fulfilment | "") {
    setStatus(next);
    if (next === "delivered") setDeliveredOn(clampDay(addDays(placedOn, 1), placedOn, now));
    if (next === "confirmed") setStartsOn(addDays(placedOn, 7));
  }

  function submit() {
    setSubmitted(true);
    setServerErrors([]);
    if (errors.length === 0) create.mutate();
  }

  const query = q.trim().toLowerCase();
  const current = groups.find((g) => g.key === tab);
  const shown = query
    ? groups.flatMap((g) => g.items.filter((i) => i.name.toLowerCase().includes(query)))
    : (current?.items ?? []);
  const orderRef = create.data?.ref ?? catalog.data?.next_order_ref ?? "";
  const statusHelp = !status
    ? "Pick how far along the order is"
    : suggestion && suggestion !== status
      ? `Suggested for these items: ${STATUS[suggestion].label}`
      : STATUS[status].help;

  return (
    <div className="fixed inset-0 z-40 flex items-end justify-center sm:items-center sm:p-4">
      <div aria-hidden="true" className="absolute inset-0 animate-wn-fade bg-scrim" onClick={create.isPending ? undefined : onClose} />
      <div
        ref={dialog}
        role="dialog"
        aria-modal="true"
        aria-labelledby="ao-title"
        aria-describedby="ao-desc"
        className={cn(
          "relative z-[41] flex h-dvh w-full animate-wn-in flex-col bg-surface shadow-lg",
          "pt-[env(safe-area-inset-top)] sm:h-auto sm:max-h-[min(840px,calc(100dvh-2rem))] sm:max-w-[680px] sm:rounded-xl sm:pt-0",
        )}
      >
        <header className="flex items-start justify-between gap-3 border-b border-border py-4 pr-3 pl-6">
          <div className="flex flex-col gap-1">
            <div className="flex items-center gap-2">
              <h2 id="ao-title" className="text-title-md font-semibold">
                Add a test order
              </h2>
              <span className="rounded-sm border border-neutral-border bg-muted px-1.5 text-caption font-medium text-ink-muted">Demo</span>
            </div>
            <p id="ao-desc" className="text-body-sm text-ink-muted">
              Creates a simulated order for {customerName} so you can try the refund chat with it. Nothing is charged.
            </p>
          </div>
          <IconButton icon="close" label="Close" size={44} onClick={onClose} />
        </header>

        <div className="flex flex-grow flex-col gap-5 overflow-y-auto px-6 py-5">
          {showSummary ? (
            <div ref={summary} id="ao-errors" role="alert" tabIndex={-1}
              className="flex gap-3 rounded-lg border border-denied-border bg-denied-bg px-4 py-3 outline-none">
              <Icon name="alert-circle" size={18} strokeWidth={2.25} className="mt-px shrink-0 text-denied-icon" />
              <div className="flex flex-col gap-1">
                <p className="text-body-sm font-semibold text-denied-fg">Fix these before adding the order</p>
                <ul className="list-disc pl-5 text-body-sm text-ink">
                  {[...errors.map((e) => e.text), ...serverErrors].map((t) => (
                    <li key={t}>{t}</li>
                  ))}
                </ul>
              </div>
            </div>
          ) : null}

          <div className="grid gap-4 sm:grid-cols-3">
            <Field id="ao-num" label="Order number" help="Next in sequence · can't be changed">
              <TextInput id="ao-num" readOnly mono value={orderRef} aria-describedby="ao-num-help" className="bg-muted" />
            </Field>
            <DatePicker id="ao-date" label="Order date" value={placedOn} min={addDays(now, -MAX_AGE_DAYS)} max={now}
              onChange={setPlacedOn} help="Up to 60 days back" dialogLabel="Choose order date" />
            <div className="flex flex-col gap-1.5">
              <span className="text-body-sm font-medium">Customer</span>
              <span className="flex h-10 items-center rounded-md bg-muted px-3 text-body-sm">{customerName}</span>
              <span className="text-caption text-ink-subtle">Signed-in customer</span>
            </div>
          </div>

          <section aria-labelledby="ao-items-h" className="flex flex-col gap-3">
            <div className="flex items-baseline justify-between gap-2">
              <h3 id="ao-items-h" className="text-body-sm font-semibold">
                Items <span className="font-normal text-ink-muted">({chosen.length})</span>
              </h3>
              {errorOf("items") ? <span className="text-caption text-denied-fg">{errorOf("items")}</span> : null}
            </div>
            {chosen.length === 0 ? (
              <p className="rounded-lg border border-dashed border-border-control px-4 py-3 text-center text-meta text-ink-muted">
                No items yet. Add workspace bookings, add-ons or products from the catalog below.
              </p>
            ) : (
              <ul aria-label="Items in this order" className="flex flex-col divide-y divide-muted rounded-lg border border-border">
                {chosen.map(({ line, item }) => {
                  const name = lineName(item, line.quantity);
                  return (
                    <li key={item.id} className="flex flex-wrap items-center gap-x-3 gap-y-2 px-3 py-2.5">
                      <span className="flex min-w-[160px] flex-grow flex-col">
                        <span className="text-body-sm font-medium">{name}</span>
                        <span className="text-caption text-ink-subtle">
                          {groupLabel(item)} · {formatCents(item.unit_cents)} {item.unit}
                        </span>
                      </span>
                      <span role="group" aria-label={`Quantity of ${item.name}`} className="inline-flex items-center rounded-md border border-border-strong">
                        <button type="button" aria-label={`One less ${item.name}`} disabled={line.quantity <= 1}
                          onClick={() => setQuantity(item.id, line.quantity - 1)}
                          className="size-9 text-ink disabled:text-ink-disabled">
                          −
                        </button>
                        <span aria-live="polite" className="min-w-[44px] text-center font-mono text-mono tabular">
                          {item.hourly ? `${line.quantity} h` : `× ${line.quantity}`}
                        </span>
                        <button type="button" aria-label={`One more ${item.name}`} disabled={line.quantity >= maxQuantity(item)}
                          onClick={() => setQuantity(item.id, line.quantity + 1)}
                          className="size-9 text-ink disabled:text-ink-disabled">
                          +
                        </button>
                      </span>
                      <span className="min-w-[72px] text-right font-mono text-mono tabular">{formatCents(item.unit_cents * line.quantity)}</span>
                      <IconButton icon="close" label={`Remove ${name}`} size={32}
                        onClick={() => setLines((ls) => ls.filter((l) => l.id !== item.id))} />
                    </li>
                  );
                })}
              </ul>
            )}

            <div className="flex min-w-0 flex-col gap-2 rounded-lg border border-border bg-canvas p-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-caption font-semibold tracking-[0.05em] text-ink-muted uppercase">Catalog</span>
                <label htmlFor="ao-search" className="sr-only">
                  Search the catalog
                </label>
                <SearchInput id="ao-search" value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search all items" className="w-[220px]" />
              </div>
              {query ? null : (
                <Tabs label="Catalog categories" idBase="ao-cat" value={tab} onChange={setTab}
                  className="overflow-x-auto [&>button]:shrink-0 [&>button]:whitespace-nowrap"
                  items={groups.map((g) => ({ value: g.key, label: g.label, count: g.items.length }))} />
              )}
              <ul {...(query ? {} : tabPanelProps("ao-cat", tab))} aria-label={query ? "Search results" : undefined}
                className="flex max-h-[212px] flex-col overflow-y-auto">
                {catalog.isPending ? (
                  <li className="flex flex-col gap-2 py-2">
                    <Skeleton className="h-8" />
                    <Skeleton className="h-8" />
                  </li>
                ) : catalog.isError ? (
                  <li className="py-3 text-meta text-denied-fg">Couldn&apos;t load the catalog.</li>
                ) : shown.length === 0 ? (
                  <li className="py-3 text-meta text-ink-muted">No items match “{q.trim()}”.</li>
                ) : (
                  shown.map((item) => {
                    const added = lines.some((l) => l.id === item.id);
                    return (
                      <li key={item.id} className="flex items-center gap-3 border-b border-muted py-2 last:border-b-0">
                        <span className="flex min-w-0 flex-grow flex-col">
                          <span className="text-body-sm">{item.name}</span>
                          <span className="text-caption text-ink-subtle">
                            {query ? `${groupLabel(item)} · ` : ""}
                            {KIND_NOTE[item.kind]}
                            {item.final_sale ? " · Final sale" : ""}
                          </span>
                        </span>
                        <span className="font-mono text-mono tabular">
                          {formatCents(item.unit_cents)}
                          {item.hourly ? "/h" : ""}
                        </span>
                        <Button size="sm" aria-label={`${added ? "Add another" : "Add"} ${item.name}`} onClick={() => add(item)}>
                          {added ? "Add another" : "Add"}
                        </Button>
                      </li>
                    );
                  })
                )}
              </ul>
            </div>
          </section>

          <div className="grid gap-4 sm:grid-cols-2">
            <div className="flex flex-col gap-1.5">
              <span className="text-body-sm font-medium">Total</span>
              <span aria-live="polite" className="flex h-10 items-center font-mono text-[15px] font-medium tabular">
                {formatCents(total)}
              </span>
              <span className="text-caption text-ink-subtle">Calculated from the items</span>
            </div>
            <Field id="ao-status" label="Delivery status" help={statusHelp} error={errorOf("status")}>
              <Select id="ao-status" value={status} invalid={!!errorOf("status")} aria-describedby="ao-status-help"
                onChange={(e) => choose(e.target.value as Fulfilment | "")}>
                <option value="">Choose a status</option>
                {(Object.keys(STATUS) as Fulfilment[]).map((f) => (
                  <option key={f} value={f}>
                    {STATUS[f].label}
                  </option>
                ))}
              </Select>
            </Field>
            {status === "delivered" ? (
              <DatePicker id="ao-delivered" label="Delivered on" value={deliveredDay} min={placedOn} max={now}
                onChange={setDeliveredOn} help="On or after the order date" dialogLabel="Choose delivery date" />
            ) : status === "confirmed" ? (
              <DatePicker id="ao-starts" label="Starts on" value={startsDay} min={placedOn} max={addDays(now, MAX_START_DAYS)}
                onChange={setStartsOn} help="Bookings and plans can start in the future" dialogLabel="Choose start date" />
            ) : status === "used" ? (
              <div className="flex flex-col gap-1.5">
                <span className="text-body-sm font-medium">Used on</span>
                <span className="flex h-10 items-center rounded-md bg-muted px-3 font-mono text-mono">{formatShortDay(placedOn)}</span>
                <span className="text-caption text-ink-subtle">Same as the order date</span>
              </div>
            ) : status === "active" ? (
              <Field id="ao-run" label="Runs for" error={errorOf("run")}
                help={`Active since ${formatShortDay(placedOn)} · until ${formatShortDay(runEnd)} (${ended ? "ended" : "still running"})`}>
                <Select id="ao-run" value={String(runsFor)} invalid={!!errorOf("run")} onChange={(e) => setRunsFor(Number(e.target.value))}>
                  {RUNS.map(([days, label]) => (
                    <option key={days} value={days}>
                      {label}
                    </option>
                  ))}
                </Select>
              </Field>
            ) : null}
          </div>

          <div className="flex flex-col gap-1.5 rounded-lg border border-info-border bg-info-tint px-4 py-3">
            <span className="text-body-sm font-semibold">What the assistant will likely decide</span>
            {errors.length > 0 || !preview.data ? (
              <p className="text-meta text-ink-muted">
                {preview.isError
                  ? "Couldn't check the refund policy just now."
                  : errors.length > 0
                    ? "Add items and a delivery status to see how the refund policy applies."
                    : "Checking the refund policy…"}
              </p>
            ) : (
              <>
                <p className="text-meta text-ink-muted">{preview.data.assumption}:</p>
                <ul className="flex flex-col gap-1 text-meta">
                  {preview.data.items.map((i) => (
                    <li key={i.name}>
                      <span className="font-medium">{i.name}</span>: {VERDICT_WORD[i.verdict]}
                      {i.reason ? `. ${i.reason}` : ""}
                    </li>
                  ))}
                </ul>
              </>
            )}
          </div>
        </div>

        <footer className="flex flex-wrap justify-end gap-3 border-t border-border px-6 pt-3.5 pb-[calc(14px+env(safe-area-inset-bottom))]">
          <Button variant="ghost" size="lg" onClick={onClose} disabled={create.isPending}>
            Cancel
          </Button>
          <Button variant="primary" size="lg" onClick={submit} loading={create.isPending} loadingText="Adding…">
            Add order {orderRef}
          </Button>
        </footer>
      </div>
    </div>
  );
}
