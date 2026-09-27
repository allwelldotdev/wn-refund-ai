"use client";

import { useEffect, useState } from "react";

import { Button } from "@/components/ui/Button";
import { Checkbox, Field, SearchInput, Select } from "@/components/ui/Field";
import { EmptyState } from "@/components/ui/Surface";
import { Pagination } from "@/components/ui/Table";
import { stateLabel } from "@/components/ui/Badge";
import { INJECTION_FLAGS, REASON_FILTERS, useRequests } from "@/lib/admin";
import { REQUEST_STATES, type Flag, type RequestState } from "@/lib/api-types";
import { plural } from "@/lib/format";
import { useOpenRequest } from "@/lib/use-open-request";

import { RequestTable } from "./RequestTable";
import { SectionError, TableSkeleton } from "./common";

const PAGE = 10;
const RANGES = [
  { value: "today", label: "Today" },
  { value: "7", label: "Last 7 days" },
  { value: "30", label: "Last 30 days" },
  { value: "60", label: "Last 60 days" },
  { value: "all", label: "All time" },
] as const;
type Range = (typeof RANGES)[number]["value"];

const DEFAULTS = { q: "", state: "" as RequestState | "", range: "60" as Range, reason: "", injection: false };

function sinceFor(range: Range): string | null {
  if (range === "all") return null;
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  if (range !== "today") d.setDate(d.getDate() - (Number(range) - 1));
  return d.toISOString();
}

export function RequestsSection() {
  const [filters, setFilters] = useState(DEFAULTS);
  const [q, setQ] = useState("");
  const [page, setPage] = useState(1);
  const { open, selected } = useOpenRequest();

  // Search is debounced by 250 ms; every filter change goes back to page 1.
  useEffect(() => {
    const t = window.setTimeout(() => {
      setFilters((f) => (f.q === q ? f : { ...f, q }));
      setPage(1);
    }, 250);
    return () => window.clearTimeout(t);
  }, [q]);

  const [since] = useState(() => Object.fromEntries(RANGES.map((r) => [r.value, sinceFor(r.value)])) as Record<Range, string | null>);
  const flagGroups: Flag[][] = [];
  const reason = REASON_FILTERS.find((r) => r.value === filters.reason);
  if (reason?.flags.length) flagGroups.push(reason.flags);
  if (filters.injection) flagGroups.push(INJECTION_FLAGS);

  const list = useRequests({
    state: filters.state,
    q: filters.q,
    since: since[filters.range],
    flagGroups,
    disputed: reason?.disputed,
    limit: PAGE,
    offset: (page - 1) * PAGE,
  });

  const filtered =
    filters.q !== "" || filters.state !== "" || filters.range !== DEFAULTS.range || filters.reason !== "" || filters.injection;

  function update(patch: Partial<typeof DEFAULTS>) {
    setFilters((f) => ({ ...f, ...patch }));
    setPage(1);
  }

  function clear() {
    setQ("");
    setFilters(DEFAULTS);
    setPage(1);
  }

  const rangeLabel = RANGES.find((r) => r.value === filters.range)!.label.toLowerCase();
  const total = list.data?.total ?? 0;

  return (
    <div className="flex flex-col gap-4">
      <form role="search" onSubmit={(e) => e.preventDefault()} className="grid grid-cols-1 items-end gap-3 md:grid-cols-2 xl:grid-cols-[1.6fr_repeat(3,1fr)_auto]">
        <Field id="f-q" label="Search">
          <SearchInput id="f-q" placeholder="Reference, customer, email or order" value={q} onChange={(e) => setQ(e.target.value)} />
        </Field>
        <Field id="f-status" label="Status">
          <Select id="f-status" value={filters.state} onChange={(e) => update({ state: e.target.value as RequestState | "" })}>
            <option value="">All statuses</option>
            {REQUEST_STATES.map((s) => (
              <option key={s} value={s}>
                {stateLabel(s)}
              </option>
            ))}
          </Select>
        </Field>
        <Field id="f-range" label="Date range">
          <Select id="f-range" value={filters.range} onChange={(e) => update({ range: e.target.value as Range })}>
            {RANGES.map((r) => (
              <option key={r.value} value={r.value}>
                {r.label}
              </option>
            ))}
          </Select>
        </Field>
        <Field id="f-reason" label="Escalation reason">
          <Select id="f-reason" value={filters.reason} onChange={(e) => update({ reason: e.target.value })}>
            <option value="">Any reason</option>
            {REASON_FILTERS.map((r) => (
              <option key={r.value} value={r.value}>
                {r.label}
              </option>
            ))}
          </Select>
        </Field>
        <Checkbox id="f-injection" label="Flagged for injection" checked={filters.injection} onChange={(e) => update({ injection: e.target.checked })} />
      </form>

      <div className="flex flex-wrap items-center justify-between gap-3">
        <p role="status" className="text-meta text-ink-muted">
          {list.data ? (filtered ? `${plural(total, "request")} ${total === 1 ? "matches" : "match"}` : `${plural(total, "request")} in the ${rangeLabel === "all time" ? "system" : rangeLabel}`) : " "}
        </p>
        {filtered ? (
          <Button variant="muted" size="sm" onClick={clear}>
            Clear filters
          </Button>
        ) : null}
      </div>

      {list.isPending ? (
        <TableSkeleton label="Loading requests" />
      ) : list.isError && !list.data ? (
        <SectionError what="requests" error={list.error} onRetry={() => void list.refetch()} />
      ) : total === 0 ? (
        filtered ? (
          <EmptyState icon="search" title="No requests match these filters" action={<Button variant="muted" onClick={clear}>Clear filters</Button>}>
            Try a wider date range or clear the filters.
          </EmptyState>
        ) : (
          <EmptyState icon="inbox" title={`No refund requests in the ${rangeLabel}`}>
            Requests appear here as soon as a customer starts one in chat.
          </EmptyState>
        )
      ) : (
        <div className="flex flex-col">
          <RequestTable variant="queue" caption="Refund requests, newest first. Select a reference to open details." items={list.data.items} selected={selected} onOpen={open} />
          <div className="hidden md:block">
            <Pagination label="Requests pages" page={page} pageSize={PAGE} total={total} summaryId="req-results" onPage={(p) => { setPage(p); document.getElementById("req-results")?.focus(); }} />
          </div>
          <div className="md:hidden">
            <Pagination label="Requests pages" compact page={page} pageSize={PAGE} total={total} onPage={setPage} />
          </div>
        </div>
      )}
    </div>
  );
}
