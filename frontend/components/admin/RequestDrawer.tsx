"use client";

import Link from "next/link";
import { useState, type ReactNode } from "react";

import { Chip, StatusBadge, stateLabel } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Icon, type IconName } from "@/components/ui/Icon";
import { Dialog, Drawer } from "@/components/ui/Overlay";
import { Alert, DefinitionList, Skeleton } from "@/components/ui/Surface";
import {
  BUILTIN_CHECKS,
  DETECTOR_LABELS,
  DISPUTED_META,
  FLAG_META,
  RULE_META,
  distinctFlags,
  failureSummary,
  shortHash,
  useRequestDetail,
  usePolicyVersion,
} from "@/lib/admin";
import type {
  AuditInfo,
  DetailMessage,
  FiredRule,
  RequestDetail,
  Rule,
  SignalView,
  StageLog,
} from "@/lib/api-types";
import { api } from "@/lib/bff";
import { cn } from "@/lib/cn";
import { splitSignals } from "@/lib/signals";
import {
  REASON_LABELS,
  formatCents,
  formatDate,
  formatDateTime,
  formatInt,
  formatLatency,
  formatRelativeDayTime,
  formatTime,
} from "@/lib/format";

import { ResolveDialog, ReviewDraft, type ResolveTarget } from "./common";

/** The request case file, opened from any admin section with `?ref=`. */
export function RequestDrawer({ requestRef, onClose }: { requestRef: string | null; onClose: () => void }) {
  const detail = useRequestDetail(requestRef);
  const [resolve, setResolve] = useState<ResolveTarget | null>(null);
  const d = detail.data;
  const escalated = d?.request.state === "escalated";

  const header = (
    <div className="flex flex-col gap-1.5">
      <p className="text-caption font-medium text-ink-subtle">Refund request</p>
      <div className="flex flex-wrap items-center gap-2.5">
        <h2 id="dr-ref" className="font-mono text-title font-medium">
          {requestRef}
        </h2>
        {d ? <StatusBadge state={d.request.state} /> : null}
      </div>
      {d ? (
        <p className="text-meta text-ink-muted">
          {[d.customer.name, d.order?.ref, d.request.amount_cents !== null ? formatCents(d.request.amount_cents) : null, formatRelativeDayTime(d.request.created_at)]
            .filter(Boolean)
            .join(" · ")}
        </p>
      ) : null}
    </div>
  );

  const target = (resolution: "approved" | "denied"): ResolveTarget | null =>
    d ? { ref: d.request.ref, customer: d.customer.name, orderRef: d.order?.ref ?? null, amountCents: d.request.amount_cents, resolution } : null;

  return (
    <>
      <Drawer
        open={requestRef !== null}
        onClose={onClose}
        labelledBy="dr-ref"
        header={header}
        footer={
          escalated ? (
            <>
              <Button variant="danger-outline" onClick={() => setResolve(target("denied"))}>
                Deny
              </Button>
              <Button variant="primary" onClick={() => setResolve(target("approved"))}>
                Approve {d?.request.amount_cents !== null ? formatCents(d?.request.amount_cents) : "refund"}
              </Button>
            </>
          ) : null
        }
      >
        {detail.isPending ? (
          <div aria-busy="true" className="flex flex-col gap-4">
            <span className="sr-only">Loading refund details</span>
            <Skeleton className="h-4 w-1/3" />
            <Skeleton className="h-24" />
            <Skeleton className="h-4 w-1/4" />
            <Skeleton className="h-40" />
          </div>
        ) : detail.isError ? (
          <Alert tone="error" title="Couldn't load this request" action={<Button size="sm" onClick={() => detail.refetch()}>Try again</Button>}>
            {detail.error.message}
          </Alert>
        ) : (
          <DrawerBody d={detail.data} />
        )}
      </Drawer>
      <ResolveDialog target={resolve} onClose={() => setResolve(null)} />
    </>
  );
}

function Section({ title, aside, children }: { title: ReactNode; aside?: ReactNode; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="text-title-sm font-semibold">{title}</h3>
        {aside}
      </div>
      {children}
    </section>
  );
}

function DrawerBody({ d }: { d: RequestDetail }) {
  const verdictMessage = [...d.messages].reverse().find((m) => m.role === "assistant" && m.assistant_kind === "verdict");
  const resolvedEvent = d.timeline.find((e) => e.kind === "resolved");
  const staffMessage = [...d.messages].reverse().find((m) => m.role === "admin");
  return (
    <>
      <Timeline d={d} />
      <MessageThread messages={d.messages} />
      <Extracted audit={d.audit} />
      <RuleTrace audit={d.audit} />
      <Flags audit={d.audit} messages={d.messages} disputedAt={d.request.disputed_at} />
      {d.review ? (
        <Section title="AI review draft">
          <ReviewDraft review={d.review} createdAt={d.request.created_at} />
        </Section>
      ) : null}
      <PolicyAndModel d={d} />
      {verdictMessage ? (
        <Section title="Response sent to the customer">
          <blockquote className="border-l-2 border-border-strong pl-3 text-body-sm whitespace-pre-wrap [overflow-wrap:anywhere]">
            {verdictMessage.body}
          </blockquote>
          <p className="font-mono text-caption text-ink-subtle">
            Sent in chat · {formatRelativeDayTime(verdictMessage.created_at)} · {d.request.ref}
          </p>
        </Section>
      ) : null}
      {staffMessage ? (
        <Section title="Message sent after review">
          <blockquote className="border-l-2 border-border-strong pl-3 text-body-sm whitespace-pre-wrap [overflow-wrap:anywhere]">
            {staffMessage.body}
          </blockquote>
          <p className="font-mono text-caption text-ink-subtle">Sent in chat · {formatRelativeDayTime(staffMessage.created_at)}</p>
        </Section>
      ) : null}
      {resolvedEvent ? (
        <p className="rounded-md bg-muted px-3 py-2 text-body-sm">
          {resolvedEvent.payload.resolution === "approved" ? "Approved" : "Denied"} by{" "}
          <span className="font-semibold">{resolvedEvent.actor_name ?? "an admin"}</span> · {formatDateTime(resolvedEvent.created_at)}.{" "}
          Note for the audit log: {String(resolvedEvent.payload.note ?? "")}
        </p>
      ) : null}
      <RawAuditButton requestRef={d.request.ref} />
    </>
  );
}

type Step = { label: string; detail: string; at: string | null; done: boolean; icon: IconName };

function Timeline({ d }: { d: RequestDetail }) {
  const firstMessage = d.messages.find((m) => m.role === "customer");
  const steps: Step[] = [
    { label: "Received", detail: "Request started in chat", at: firstMessage?.created_at ?? d.request.created_at, done: true, icon: "clock" },
  ];
  for (const e of d.timeline) {
    if (e.kind === "decided") {
      const verdict = String(e.payload.verdict ?? "escalated");
      const flags = (e.payload.flags as string[] | undefined) ?? [];
      const llm = flags.includes("llm_failure");
      const injection = flags.includes("prescan_signal") || flags.includes("intake_injection_signal");
      steps.push({
        label: verdict[0].toUpperCase() + verdict.slice(1),
        detail: e.payload.seeded
          ? "Seeded history, decided before this system"
          : verdict === "escalated"
            ? llm
              ? "Automatic review failed; sent to a person"
              : injection
                ? "Flagged as a possible injection; sent to a person"
                : "Sent to a person by the rules"
            : "Decided automatically",
        at: e.created_at,
        done: true,
        icon: verdict === "approved" ? "check-circle" : verdict === "denied" ? "x-circle" : "warning",
      });
    } else if (e.kind === "disputed") {
      steps.push({ label: "Disputed", detail: "Customer asked a person to review the denial", at: e.created_at, done: true, icon: "dispute" });
    } else if (e.kind === "review_drafted") {
      steps.push({ label: "AI review drafted", detail: `Advisory draft by ${String(e.payload.model ?? "the review model")}`, at: e.created_at, done: true, icon: "document" });
    } else if (e.kind === "review_failed") {
      steps.push({ label: "AI review failed", detail: "No draft; decide manually", at: e.created_at, done: true, icon: "info-circle" });
    } else if (e.kind === "resolved") {
      steps.push({
        label: "Resolved",
        detail: `${e.payload.resolution === "approved" ? "Approved" : "Denied"} by ${e.actor_name ?? "an admin"}`,
        at: e.created_at,
        done: true,
        icon: "check-circle",
      });
    }
  }
  if (d.request.state === "escalated") {
    steps.push({ label: "Resolved", detail: "Waiting for an admin decision", at: null, done: false, icon: "ring" });
  } else if (d.request.state === "approved" || d.request.state === "denied") {
    steps.push({ label: "Resolved", detail: "Closed with the automatic decision", at: d.timeline[0]?.created_at ?? null, done: true, icon: "check-circle" });
  }
  return (
    <Section title="Status timeline">
      <ol className="flex flex-col">
        {steps.map((s, i) => (
          <li key={i} className="relative grid grid-cols-[20px_minmax(0,1fr)_auto] gap-x-3 pb-4 last:pb-0">
            {i < steps.length - 1 ? (
              <span aria-hidden="true" className={cn("absolute top-5 bottom-0 left-[9px] w-px", steps[i + 1].done ? "bg-ink" : "bg-border")} />
            ) : null}
            <Icon name={s.icon} size={20} className={cn("relative bg-surface", s.done ? "text-ink" : "text-ink-disabled")} />
            <span className="flex flex-col">
              <span className={cn("text-body-sm font-semibold", !s.done && "text-ink-subtle")}>{s.label}</span>
              <span className="text-meta text-ink-muted">{s.detail}</span>
            </span>
            <span className="font-mono text-caption text-ink-subtle tabular">{s.at ? `${formatDate(s.at).replace(/, \d{4}$/, "")} · ${formatTime(s.at)}` : "—"}</span>
          </li>
        ))}
      </ol>
    </Section>
  );
}

/** Message text with matched spans underlined; everything stays plain text. */
export function SignalText({ text, signals }: { text: string; signals: SignalView[] }) {
  if (!signals.length) return <>{text}</>;
  return (
    <>
      {splitSignals(text, signals).map((seg, i) =>
        seg.marked ? (
          <span key={i} className="bg-denied-bg underline decoration-denied-icon decoration-wavy underline-offset-4">
            {seg.text}
          </span>
        ) : (
          seg.text
        ),
      )}
    </>
  );
}

const SENDER: Record<DetailMessage["role"], string> = {
  customer: "Customer",
  assistant: "Assistant",
  admin: "Support team",
  system: "Note",
};

function MessageThread({ messages }: { messages: DetailMessage[] }) {
  const [expanded, setExpanded] = useState(false);
  const customer = messages.filter((m) => m.role === "customer");
  const collapse = messages.length > 4 && !expanded;
  const hidden = messages.length - 3;
  const shown = collapse ? [messages[0], null, ...messages.slice(-2)] : messages;
  const markedIndex = customer.findIndex((m) => m.signals.length > 0);
  return (
    <Section
      title={`Customer messages (${customer.length})`}
      aside={
        <span className="inline-flex items-center gap-1.5 text-caption text-ink-subtle">
          <Icon name="shield" size={14} />
          Untrusted · shown as plain text
        </span>
      }
    >
      <ol className="flex flex-col gap-2">
        {shown.map((m) =>
          m === null ? (
            <li key="collapsed">
              <button
                type="button"
                aria-expanded="false"
                onClick={() => setExpanded(true)}
                className="w-full rounded-md border border-dashed border-border-strong px-3 py-2 text-body-sm font-medium text-ink-muted hover:bg-canvas"
              >
                Show {hidden} earlier messages
              </button>
            </li>
          ) : m.role === "customer" ? (
            <li key={m.id} className="flex flex-col gap-1.5 rounded-lg bg-muted px-3 py-2.5">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="flex items-center gap-2 text-caption">
                  <span className="font-semibold">Customer</span>
                  {m.id === customer[0]?.id ? <Chip>Original</Chip> : null}
                  <span className="font-mono text-ink-subtle">{formatRelativeDayTime(m.created_at)}</span>
                </span>
                {m.tag === "used_in_decision" ? (
                  <span className="rounded-sm border border-border-strong bg-surface px-1.5 py-0.5 text-caption font-medium">Used in decision</span>
                ) : m.tag === "after_decision" ? (
                  <span className="rounded-sm border border-dashed border-border-strong bg-muted px-1.5 py-0.5 text-caption font-medium text-ink-muted">After decision</span>
                ) : null}
              </div>
              <p className="text-body-sm whitespace-pre-wrap [overflow-wrap:anywhere]">
                <SignalText text={m.body} signals={m.signals} />
              </p>
            </li>
          ) : (
            <li key={m.id} className={cn("ml-4 flex flex-col gap-0.5 px-3 py-1.5 text-body-sm text-ink-muted", m.role === "system" && "rounded-md border border-dashed border-border-control")}>
              <span className="text-caption font-semibold tracking-[0.05em] uppercase">
                {SENDER[m.role]} <span className="font-mono font-normal normal-case">{formatTime(m.created_at)}</span>
              </span>
              <span className="whitespace-pre-wrap [overflow-wrap:anywhere]">{m.body}</span>
            </li>
          ),
        )}
      </ol>
      {markedIndex >= 0 ? (
        <p className="text-meta text-ink-muted">
          Underlined: text the injection screen matched (message {markedIndex + 1} of {customer.length}). The request
          went to a person without the message being read by the model.
        </p>
      ) : null}
    </Section>
  );
}

function Extracted({ audit }: { audit: AuditInfo | null }) {
  if (!audit) {
    return (
      <Section title="AI-extracted fields">
        <p className="text-body-sm text-ink-muted">No audit record: this request predates the assistant (seeded history).</p>
      </Section>
    );
  }
  const x = audit.extracted;
  const order = audit.facts.order;
  const missing = <span className="text-ink-subtle">Not extracted</span>;
  return (
    <Section title="AI-extracted fields">
      {!x ? <p className="text-meta text-ink-muted">The reading model was skipped (the message screen flagged it) or failed.</p> : null}
      <DefinitionList
        labelWidth={150}
        rows={[
          { label: "Order", value: order ? <span className="font-mono">{order.order_ref} · matched</span> : x?.mentioned_order_refs.length ? <span className="font-mono">{x.mentioned_order_refs.join(", ")} · not this customer&apos;s</span> : missing },
          { label: "Item", value: order?.item.name ?? missing },
          { label: "Amount requested", value: x?.claimed_amount_cents != null ? <span className="font-mono tabular">{formatCents(x.claimed_amount_cents)}</span> : order ? <span className="font-mono tabular">{formatCents(order.item.amount_cents)}</span> : missing },
          { label: "Reason category", value: x?.reason_category ? REASON_LABELS[x.reason_category] : missing },
          { label: "Contradictions", value: x ? (x.contradictory_statements ? "Yes" : "None found") : missing },
          { label: "Extraction confidence", value: x ? <span className="font-mono tabular">{x.confidence.toFixed(2)}</span> : <span className="text-ink-subtle">— (no model output)</span> },
        ]}
      />
    </Section>
  );
}

function outcomePill(verdict: "approved" | "denied" | "escalated") {
  const map = {
    approved: { tone: "approved", icon: "check", label: "Approves" },
    denied: { tone: "denied", icon: "close", label: "Denies" },
    escalated: { tone: "escalated", icon: "warning", label: "Escalates" },
  } as const;
  const m = map[verdict];
  return (
    <Chip tone={m.tone} icon={m.icon}>
      {m.label}
    </Chip>
  );
}

function ruleTitle(rule: Rule): string {
  if (rule.kind === "refund_window") {
    return `Refund window · ${rule.scope.kind === "all" ? "All" : rule.scope.category}`;
  }
  return RULE_META[rule.kind].name;
}

function firedFor(rule: Rule, fired: FiredRule[]): FiredRule | undefined {
  return fired.find((f) => {
    if (f.kind !== rule.kind) return false;
    if (rule.kind !== "refund_window") return true;
    const scope = (f.detail as { scope?: { kind: string; category?: string } }).scope;
    return scope?.kind === rule.scope.kind && (rule.scope.kind === "all" || scope?.category === rule.scope.category) && (f.detail as { days?: number }).days === rule.days;
  });
}

/**
 * Every rule of the policy version that decided the request, marked fired or
 * not, plus any built-in safety check. The engine is deterministic: the most
 * severe fired verdict wins; with none fired, a person decides.
 */
function RuleTrace({ audit }: { audit: AuditInfo | null }) {
  const version = usePolicyVersion(audit?.policy_version.id ?? null);
  if (!audit) return null;
  const rules = version.data?.rules.rules ?? [];
  const builtins = audit.rule_trace.filter((f) => f.kind === "fail_closed" || f.kind === "active_refund_exists");
  return (
    <Section title={`Rule trace · deterministic, policy version ${audit.policy_version.version}`}>
      {version.isPending ? (
        <Skeleton className="h-32" />
      ) : (
        <ol className="flex flex-col divide-y divide-muted rounded-lg border border-border">
          {rules.map((rule, i) => {
            const f = firedFor(rule, audit.rule_trace);
            return (
              <li key={i} className="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-3 px-3 py-2.5">
                <span className="flex flex-col gap-0.5">
                  <span className={cn("text-body-sm font-medium", !rule.enabled && "text-ink-subtle")}>{ruleTitle(rule)}</span>
                  <span className="text-meta text-ink-muted">{f ? f.explanation : rule.enabled ? "Did not apply to this request." : "Turned off in this version."}</span>
                </span>
                {f ? outcomePill(f.verdict) : <Chip dashed>{rule.enabled ? "Not triggered" : "Off"}</Chip>}
              </li>
            );
          })}
          {builtins.map((f, i) => (
            <li key={`b${i}`} className="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-3 px-3 py-2.5">
              <span className="flex flex-col gap-0.5">
                <span className="text-body-sm font-medium">{BUILTIN_CHECKS[f.kind as keyof typeof BUILTIN_CHECKS]}</span>
                <span className="text-meta text-ink-muted">{f.explanation}</span>
              </span>
              {outcomePill(f.verdict)}
            </li>
          ))}
          <li className="px-3 py-2.5 text-body-sm">
            <span className="font-semibold">Result: {stateLabel(audit.verdict)}.</span>{" "}
            {audit.rule_trace.length === 0 ? "No rule applied, so a person decides." : audit.verdict === "escalated" ? "One or more checks need a person." : audit.verdict === "denied" ? "A rule denies it; denials outrank everything else." : "Every rule that applied allows it."}
          </li>
        </ol>
      )}
    </Section>
  );
}

function Flags({ audit, messages, disputedAt }: { audit: AuditInfo | null; messages: DetailMessage[]; disputedAt: string | null }) {
  if (!audit) return null;
  const flags = distinctFlags(audit.flags.filter((f) => f !== "no_rule_fired"));
  const dispute = disputedAt ? (
    <li className="flex gap-3 rounded-lg border border-escalated-border bg-escalated-bg p-3">
      <Icon name={DISPUTED_META.icon} size={18} className="mt-px shrink-0 text-escalated-icon" />
      <span className="flex flex-col gap-0.5">
        <span className="text-body-sm font-semibold">{DISPUTED_META.label}</span>
        <span className="text-meta">The assistant denied this automatically. The customer disputed it from Your requests, so a person needs to decide.</span>
        <span className="font-mono text-caption text-ink-subtle">disputed {formatDateTime(disputedAt)} · one dispute per request</span>
      </span>
    </li>
  ) : null;
  const customer = messages.filter((m) => m.role === "customer");
  const signals = customer.flatMap((m, i) => m.signals.map((s) => ({ ...s, n: i + 1 })));
  return (
    <Section title="Flags">
      {flags.length === 0 && !dispute ? (
        <p className="text-body-sm text-ink-muted">No injection or suspicion flags.</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {dispute}
          {flags.map((f) => {
            const m = FLAG_META[f];
            const meta =
              f === "prescan_signal" && signals.length
                ? signals.map((s) => `${DETECTOR_LABELS[s.detector]} · score ${s.score.toFixed(2)} · ${s.scope === "window" ? "across messages" : `in message ${s.n} of ${customer.length}`}`).join("; ")
                : f === "low_confidence" && audit.extracted
                  ? `confidence ${audit.extracted.confidence.toFixed(2)} · threshold 0.60`
                  : null;
            return (
              <li key={f} className={cn("flex gap-3 rounded-lg border p-3", m.tone === "denied" ? "border-denied-border bg-denied-bg" : "border-neutral-border bg-canvas")}>
                <Icon name={m.icon} size={18} className={cn("mt-px shrink-0", m.tone === "denied" ? "text-denied-icon" : "text-ink-muted")} />
                <span className="flex flex-col gap-0.5">
                  <span className="text-body-sm font-semibold">{m.label}</span>
                  <span className="text-meta">{m.tip.replace(/^[^:]+:\s*/, "")}</span>
                  {meta ? <span className="font-mono text-caption text-ink-subtle">{meta}</span> : null}
                </span>
              </li>
            );
          })}
        </ul>
      )}
    </Section>
  );
}

function stageLine(name: string, log: StageLog | null): ReactNode {
  if (!log) return `${name}: skipped`;
  const r = log.record;
  const failed = log.failures.map((f) => `${f.model} — ${failureSummary(f.error)}`).join("; ");
  if (!r) return `${name}: failed (${failed})`;
  return `${name}: ${r.model}${r.fallback ? " (fallback)" : ""}${failed ? ` after ${failed}` : ""}`;
}

function PolicyAndModel({ d }: { d: RequestDetail }) {
  const a = d.audit;
  if (!a) return null;
  const records = [a.stages.intake?.record, a.stages.responder?.record].filter((r): r is NonNullable<typeof r> => !!r);
  const latency = records.reduce((s, r) => s + r.latency_ms, 0);
  const tokensIn = records.reduce((s, r) => s + (r.prompt_tokens ?? 0), 0);
  const tokensOut = records.reduce((s, r) => s + (r.completion_tokens ?? 0), 0);
  return (
    <Section title="Policy & model">
      <DefinitionList
        labelWidth={120}
        rows={[
          {
            label: "Policy version",
            value: (
              <Link className="link" href={`/admin/policy?v=${a.policy_version.version}&from=${encodeURIComponent(d.request.ref)}`}>
                Version {a.policy_version.version} · <span className="font-mono">#{shortHash(a.policy_version.content_hash)}</span>
              </Link>
            ),
          },
          {
            label: "Models",
            value: (
              <span className="flex flex-col font-mono text-meta">
                <span>{stageLine("intake", a.stages.intake)}</span>
                <span>{stageLine("reply", a.stages.responder)}</span>
              </span>
            ),
          },
          { label: "Latency", value: <span className="font-mono tabular">{records.length ? formatLatency(latency) : "—"}</span> },
          { label: "Tokens", value: <span className="font-mono tabular">{records.length ? `${formatInt(tokensIn)} in · ${formatInt(tokensOut)} out` : "—"}</span> },
        ]}
      />
    </Section>
  );
}

function RawAuditButton({ requestRef }: { requestRef: string }) {
  const [open, setOpen] = useState(false);
  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  async function show() {
    setOpen(true);
    setText(null);
    setError(null);
    try {
      setText(JSON.stringify(await api<unknown>(`admin/requests/${encodeURIComponent(requestRef)}/audit`), null, 2));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Couldn't load the audit record.");
    }
  }
  return (
    <>
      <Button variant="ghost" size="sm" icon="document" className="self-start" onClick={show}>
        View raw audit JSON
      </Button>
      <Dialog
        open={open}
        onClose={() => setOpen(false)}
        title={`Raw audit record · ${requestRef}`}
        description="Every stored row for this request, as the API returns it."
        actions={<Button variant="primary" onClick={() => setOpen(false)}>Close</Button>}
      >
        {error ? (
          <p role="alert" className="text-body-sm text-denied-fg">{error}</p>
        ) : text === null ? (
          <Skeleton className="h-40" />
        ) : (
          <pre className="max-h-[50dvh] overflow-auto rounded-md bg-muted p-3 font-mono text-caption whitespace-pre">{text}</pre>
        )}
      </Dialog>
    </>
  );
}
