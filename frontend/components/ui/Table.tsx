import type { HTMLAttributes, ReactNode, TdHTMLAttributes, ThHTMLAttributes } from "react";

import { cn } from "@/lib/cn";

import { Button } from "./Button";
import { Icon } from "./Icon";

export function Table({ caption, className, children }: { caption: string; className?: string; children: ReactNode }) {
  return (
    <div className={cn("overflow-x-auto", className)}>
      <table className="w-full border-collapse text-body-sm">
        <caption className="sr-only">{caption}</caption>
        {children}
      </table>
    </div>
  );
}

export function THead({ children }: { children: ReactNode }) {
  return (
    <thead>
      <tr className="border-b border-border bg-canvas">{children}</tr>
    </thead>
  );
}

export function TH({
  className,
  numeric,
  sorted,
  children,
  ...rest
}: ThHTMLAttributes<HTMLTableCellElement> & { numeric?: boolean; sorted?: "descending" | "ascending" }) {
  return (
    <th
      scope="col"
      aria-sort={sorted}
      className={cn(
        "px-4 py-3 text-left text-caption font-semibold tracking-[0.05em] text-ink-muted uppercase first:pl-5 last:pr-5",
        numeric && "text-right",
        className,
      )}
      {...rest}
    >
      {sorted ? (
        <span className="inline-flex items-center gap-1.5 text-ink">
          {children}
          <Icon name={sorted === "descending" ? "arrow-down" : "arrow-up"} size={14} strokeWidth={2.25} />
        </span>
      ) : (
        children
      )}
    </th>
  );
}

export function TR({
  selected,
  onActivate,
  className,
  ...rest
}: HTMLAttributes<HTMLTableRowElement> & { selected?: boolean; onActivate?: () => void }) {
  return (
    <tr
      aria-selected={selected || undefined}
      onClick={onActivate}
      className={cn(
        "border-b border-muted bg-surface",
        onActivate && "cursor-pointer hover:bg-muted",
        selected && "bg-selected shadow-[inset_0_0_0_1px_var(--color-border-strong)] hover:bg-selected",
        className,
      )}
      {...rest}
    />
  );
}

export function TD({ className, numeric, ...rest }: TdHTMLAttributes<HTMLTableCellElement> & { numeric?: boolean }) {
  return (
    <td
      className={cn("px-4 py-3 align-middle first:pl-5 last:pr-5", numeric && "text-right font-mono text-mono tabular", className)}
      {...rest}
    />
  );
}

type PaginationProps = {
  page: number;
  pageSize: number;
  total: number;
  onPage: (page: number) => void;
  label: string;
  /** Mobile shows "Page X of Y" instead of numbered buttons. */
  compact?: boolean;
  summaryId?: string;
};

export function Pagination({ page, pageSize, total, onPage, label, compact, summaryId }: PaginationProps) {
  const pages = Math.max(1, Math.ceil(total / pageSize));
  const from = total === 0 ? 0 : (page - 1) * pageSize + 1;
  const to = Math.min(total, page * pageSize);
  return (
    <nav aria-label={label} className="flex flex-wrap items-center justify-between gap-3 px-5 py-3.5 text-meta text-ink-muted">
      <p id={summaryId} tabIndex={-1} aria-live="polite" className="outline-none">
        Showing {from}–{to} of {total}
      </p>
      <div className="flex items-center gap-2">
        <Button size="sm" disabled={page <= 1} onClick={() => onPage(page - 1)}>
          Previous
        </Button>
        {compact ? (
          <span className="px-2 tabular">
            Page {page} of {pages}
          </span>
        ) : (
          pageNumbers(page, pages).map((n, i) =>
            n === null ? (
              <span key={`gap-${i}`} aria-hidden="true" className="px-1">
                …
              </span>
            ) : (
              <Button
                key={n}
                size="sm"
                variant={n === page ? "primary" : "secondary"}
                aria-current={n === page ? "page" : undefined}
                aria-label={`Page ${n}`}
                className="min-w-8 px-2 tabular"
                onClick={() => onPage(n)}
              >
                {n}
              </Button>
            ),
          )
        )}
        <Button size="sm" disabled={page >= pages} onClick={() => onPage(page + 1)}>
          Next
        </Button>
      </div>
    </nav>
  );
}

function pageNumbers(page: number, pages: number): Array<number | null> {
  if (pages <= 7) return Array.from({ length: pages }, (_, i) => i + 1);
  const set = new Set([1, pages, page - 1, page, page + 1].filter((n) => n >= 1 && n <= pages));
  const sorted = [...set].sort((a, b) => a - b);
  const out: Array<number | null> = [];
  sorted.forEach((n, i) => {
    if (i > 0 && n - sorted[i - 1] > 1) out.push(null);
    out.push(n);
  });
  return out;
}
