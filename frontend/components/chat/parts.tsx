"use client";

import { useEffect, useState, type ReactNode } from "react";

import { buttonClasses } from "@/components/ui/Button";
import { Icon, Spinner, type IconName } from "@/components/ui/Icon";
import type { RequestSummary } from "@/lib/api-types";
import { cn } from "@/lib/cn";
import { formatCents, formatClock } from "@/lib/format";

export function BotBubble({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className={cn("flex animate-wn-in items-end gap-2", className)}>
      <span aria-hidden="true" className="inline-flex size-6 shrink-0 items-center justify-center rounded-full bg-primary text-white">
        <Icon name="logo" size={12} strokeWidth={2.75} />
      </span>
      <p className="max-w-[85%] rounded-[12px_12px_12px_4px] bg-muted px-3 py-2 text-body-sm whitespace-pre-wrap text-ink [overflow-wrap:anywhere]">
        <span className="sr-only">Assistant: </span>
        {children}
      </p>
    </div>
  );
}

export function UserBubble({ children, pending }: { children: ReactNode; pending?: boolean }) {
  return (
    <div className="flex animate-wn-in justify-end">
      <p
        className={cn(
          "max-w-[85%] rounded-[12px_12px_4px_12px] bg-primary px-3 py-2 text-body-sm whitespace-pre-wrap text-white [overflow-wrap:anywhere]",
          pending && "opacity-80",
        )}
      >
        <span className="sr-only">You: </span>
        {children}
      </p>
    </div>
  );
}

export function OrderBubble({ orderRef, amount, items }: { orderRef: string; amount: string; items: string }) {
  return (
    <div className="flex animate-wn-in justify-end">
      <p className="flex max-w-[85%] flex-col gap-0.5 rounded-[12px_12px_4px_12px] bg-primary px-3 py-2 text-white">
        <span className="sr-only">You picked order </span>
        <span className="font-mono text-mono tabular">
          {orderRef} · {amount}
        </span>
        <span className="text-caption text-neutral-border">{items}</span>
      </p>
    </div>
  );
}

const STEPS = ["Reading your message", "Checking {order}", "Applying the refund policy"];

/** Shown between sending and the stored reply. Steps tick over on a timer so it never looks stuck. */
export function ReviewingCard({ startedAt, orderRef, writing }: { startedAt: number; orderRef: string | null; writing: boolean }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), 500);
    return () => window.clearInterval(t);
  }, []);
  const elapsed = (now - startedAt) / 1000;
  const active = writing ? 3 : elapsed >= 4 ? 2 : elapsed >= 2 ? 1 : 0;
  return (
    <div role="status" className="ml-8 flex animate-wn-in flex-col gap-3 rounded-lg border border-info-border bg-info-tint p-3.5">
      <div className="flex items-center justify-between gap-3">
        <p className="flex items-center gap-2 text-body-sm font-medium text-info-fg">
          <Spinner />
          {writing ? "Writing the reply…" : "Reviewing your request…"}
        </p>
        <span aria-label="Elapsed time" className="font-mono text-caption text-ink-subtle tabular">
          {formatClock(elapsed)}
        </span>
      </div>
      <ol className="flex flex-col gap-1.5 text-meta">
        {STEPS.map((label, i) => {
          const text = label.replace("{order}", orderRef ?? "your orders");
          const state = i < active ? "done" : i === active ? "active" : "todo";
          return (
            <li key={label} className={cn("flex items-center gap-2", state === "todo" ? "text-ink-subtle" : "text-ink", state === "active" && "font-medium")}>
              {state === "done" ? (
                <Icon name="check" size={14} strokeWidth={2.5} className="text-approved-icon" />
              ) : state === "active" ? (
                <Spinner size={14} />
              ) : (
                <Icon name="ring" size={14} className="text-ink-disabled" />
              )}
              {text}
              <span className="sr-only">
                {state === "done" ? " (done)" : state === "active" ? " (in progress)" : " (waiting)"}
              </span>
            </li>
          );
        })}
      </ol>
      <p className="text-caption text-ink-subtle">
        Usually 5–15 seconds. You can close this chat; the answer will be waiting here.
      </p>
    </div>
  );
}

const VERDICT_STYLE: Record<
  "approved" | "denied" | "escalated",
  { border: string; head: string; icon: IconName; iconColor: string; label: string }
> = {
  approved: { border: "border-approved-border", head: "bg-approved-bg text-approved-fg", icon: "check-circle", iconColor: "text-approved-icon", label: "Refund approved" },
  denied: { border: "border-denied-border", head: "bg-denied-bg text-denied-fg", icon: "x-circle", iconColor: "text-denied-icon", label: "Not refunded" },
  escalated: { border: "border-escalated-border", head: "bg-escalated-bg text-escalated-fg", icon: "warning", iconColor: "text-escalated-icon", label: "Escalated" },
};

function verdictOf(state: RequestSummary["state"]): "approved" | "denied" | "escalated" {
  if (state === "resolved_approved") return "approved";
  if (state === "resolved_denied") return "denied";
  return state;
}

function prefersReducedMotion() {
  return typeof window !== "undefined" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** Types out `text` once, ~100 characters a second; instant under reduced motion. */
function useTypewriter(text: string, animate: boolean) {
  const [shown, setShown] = useState(() => (animate && !prefersReducedMotion() ? 0 : Number.POSITIVE_INFINITY));
  const running = shown < text.length;
  useEffect(() => {
    if (!running) return;
    const t = window.setInterval(() => setShown((n) => n + 3), 30);
    return () => window.clearInterval(t);
  }, [running]);
  return { visible: text.slice(0, shown), done: !running };
}

type VerdictCardProps = {
  id: string;
  body: string;
  request: RequestSummary;
  animate: boolean;
  onShowPolicy: () => void;
  /** Called once the text has finished playing. */
  onDone?: () => void;
};

/**
 * The decision in the thread: badge and reference first, then the stored
 * reply, then details. A request an admin later decided shows that outcome.
 */
export function VerdictCard({ id, body, request, animate, onShowPolicy, onDone }: VerdictCardProps) {
  const verdict = verdictOf(request.state);
  const s = VERDICT_STYLE[verdict];
  const { visible, done } = useTypewriter(body, animate);
  useEffect(() => {
    if (done) onDone?.();
  }, [done, onDone]);
  const reviewed = request.state === "resolved_approved" || request.state === "resolved_denied";
  const headId = `${id}-head`;
  return (
    <article aria-labelledby={headId} aria-busy={!done} className={cn("ml-8 flex animate-wn-in flex-col overflow-hidden rounded-lg border bg-surface", s.border)}>
      <div className={cn("flex items-center justify-between gap-3 px-3.5 py-2.5", s.head)}>
        <h3 id={headId} className="flex items-center gap-2 text-body-sm font-semibold">
          <Icon name={s.icon} size={16} strokeWidth={2.25} className={s.iconColor} />
          {reviewed ? (verdict === "approved" ? "Approved after review" : "Denied after review") : s.label}
        </h3>
        <span className="font-mono text-caption tabular">Ref {request.ref}</span>
      </div>
      <div className="flex flex-col gap-3 px-3.5 py-3">
        {verdict === "escalated" ? <p className="text-body-sm font-semibold">A support specialist will review this</p> : null}
        <p className="text-body-sm whitespace-pre-wrap text-ink [overflow-wrap:anywhere]">
          {visible}
          {!done ? (
            <span aria-hidden="true" className="ml-px inline-block animate-wn-blink text-ink-muted">
              ▍
            </span>
          ) : null}
        </p>
        {done ? <VerdictDetails verdict={verdict} request={request} onShowPolicy={onShowPolicy} /> : null}
      </div>
    </article>
  );
}

function VerdictDetails({
  verdict,
  request,
  onShowPolicy,
}: {
  verdict: "approved" | "denied" | "escalated";
  request: RequestSummary;
  onShowPolicy: () => void;
}) {
  const item = [request.item_name, request.order_ref].filter(Boolean).join(" · ");
  if (verdict === "approved") {
    return (
      <div className="flex flex-col gap-1">
        {request.amount_cents !== null ? (
          <p className="font-mono text-title font-medium tabular">{formatCents(request.amount_cents)}</p>
        ) : null}
        {item ? <p className="text-meta text-ink-muted">{item}</p> : null}
        <p className="text-meta text-ink-muted">Keep the reference if you contact us about this refund.</p>
      </div>
    );
  }
  if (verdict === "denied") {
    return (
      <div className="flex flex-col gap-2.5">
        <div className="flex flex-wrap items-center gap-2 text-meta text-ink-muted">
          <button type="button" onClick={onShowPolicy} className="link font-medium">
            Read the refund policy
          </button>
          {item ? <span>· {item}</span> : null}
        </div>
        <p className="flex items-center gap-1.5 border-t border-muted pt-2.5 text-meta text-ink-muted">
          <Icon name="lock" size={14} />
          {request.state === "resolved_denied"
            ? "A support specialist reviewed this request. This decision is final."
            : "This decision is final."}
        </p>
      </div>
    );
  }
  return (
    <div className="flex items-start gap-2 rounded-md bg-muted px-3 py-2 text-meta text-ink">
      <Icon name="clock" size={16} className="mt-px shrink-0 text-ink-muted" />
      <p>
        A person on our support team makes the decision. You&apos;ll see it under Your requests; you don&apos;t need to do
        anything else.
        {item ? <span className="text-ink-muted"> · {item}</span> : null}
      </p>
    </div>
  );
}

export function NoticeCard({
  tone,
  icon,
  title,
  children,
  action,
}: {
  tone: "neutral" | "error";
  icon: IconName;
  title: string;
  children: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div
      role={tone === "error" ? "alert" : "status"}
      className={cn(
        "ml-8 flex animate-wn-in flex-col gap-2 rounded-lg border p-3.5",
        tone === "error" ? "border-denied-border bg-denied-bg" : "border-neutral-border bg-muted",
      )}
    >
      <p className={cn("flex items-center gap-2 text-body-sm font-semibold", tone === "error" ? "text-denied-fg" : "text-ink")}>
        <Icon name={icon} size={16} strokeWidth={2.25} className={tone === "error" ? "text-denied-icon" : "text-ink-muted"} />
        {title}
      </p>
      <div className="text-meta text-ink">{children}</div>
      {action ? <div className="pt-1">{action}</div> : null}
    </div>
  );
}

export function RateLimitNotice({ until }: { until: number }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(t);
  }, []);
  return (
    <NoticeCard tone="neutral" icon="hourglass" title="Please wait a moment">
      You&apos;ve sent a lot of messages in the last minute. You can send another in{" "}
      <span className="font-mono tabular">{formatClock((until - now) / 1000)}</span>.
    </NoticeCard>
  );
}

export function SignInAgainLink({ href }: { href: string }) {
  return (
    <a href={href} className={buttonClasses("primary", "sm")}>
      Sign in again
    </a>
  );
}

/** The widget's footer once there is nothing to type: a single way forward. */
export function StartNewFooter({ onStart, caption }: { onStart: () => void; caption?: string }) {
  return (
    <div className="flex shrink-0 flex-col gap-2 border-t border-border bg-surface px-4 pt-3 pb-[max(0.75rem,env(safe-area-inset-bottom))]">
      {caption ? <p className="text-center text-caption text-ink-muted">{caption}</p> : null}
      <button
        type="button"
        onClick={onStart}
        className="h-12 w-full rounded-md border border-primary bg-primary text-[15px] leading-5 font-medium text-white hover:bg-primary-hover"
      >
        Start new request
      </button>
    </div>
  );
}
