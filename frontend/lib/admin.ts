"use client";

import { useQuery } from "@tanstack/react-query";

import type { IconName } from "@/components/ui/Icon";

import type {
  AdminList,
  AdminStats,
  AppSettings,
  Flag,
  PolicyVersion,
  PolicyVersionView,
  RequestDetail,
  RequestState,
  RuleKind,
  Detector,
} from "./api-types";
import { api } from "./bff";

/** Admin views refresh by polling (ADR-024). */
export const POLL_MS = 5000;

export type RequestFilter = {
  state?: RequestState | "";
  q?: string;
  since?: string | null;
  flagGroups?: Flag[][];
  /** Only requests a customer disputed. */
  disputed?: boolean;
  limit: number;
  offset: number;
};

export function requestListPath(f: RequestFilter): string {
  const p = new URLSearchParams();
  if (f.state) p.set("state", f.state);
  if (f.q?.trim()) p.set("q", f.q.trim());
  if (f.since) p.set("since", f.since);
  for (const g of f.flagGroups ?? []) if (g.length) p.append("flag", g.join(","));
  if (f.disputed) p.set("disputed", "true");
  p.set("limit", String(f.limit));
  p.set("offset", String(f.offset));
  return `admin/requests?${p}`;
}

export function useRequests(f: RequestFilter, opts: { enabled?: boolean } = {}) {
  const path = requestListPath(f);
  return useQuery({
    queryKey: ["admin", "requests", path],
    queryFn: () => api<AdminList>(path),
    refetchInterval: POLL_MS,
    placeholderData: (prev) => prev,
    enabled: opts.enabled,
  });
}

export function useStats(since: string) {
  return useQuery({
    queryKey: ["admin", "stats", since],
    queryFn: () => api<AdminStats>(`admin/stats?since=${encodeURIComponent(since)}`),
    refetchInterval: POLL_MS,
  });
}

export function useRequestDetail(ref: string | null) {
  return useQuery({
    queryKey: ["admin", "request", ref],
    queryFn: () => api<RequestDetail>(`admin/requests/${encodeURIComponent(ref!)}`),
    enabled: ref !== null,
    refetchInterval: POLL_MS,
  });
}

export function detailQuery(ref: string) {
  return {
    queryKey: ["admin", "request", ref],
    queryFn: () => api<RequestDetail>(`admin/requests/${encodeURIComponent(ref)}`),
    refetchInterval: POLL_MS,
  };
}

export function useSettings() {
  return useQuery({
    queryKey: ["admin", "settings"],
    queryFn: () => api<AppSettings>("admin/settings"),
    refetchInterval: 15_000,
  });
}

export function useCurrentPolicy() {
  return useQuery({
    queryKey: ["admin", "policy", "current"],
    queryFn: () => api<PolicyVersionView>("admin/policy/current"),
    refetchInterval: 15_000,
  });
}

export function usePolicyVersions() {
  return useQuery({
    queryKey: ["admin", "policy", "versions"],
    queryFn: () => api<PolicyVersion[]>("admin/policy/versions"),
    refetchInterval: 15_000,
  });
}

export function usePolicyVersion(id: string | null) {
  return useQuery({
    queryKey: ["admin", "policy", "version", id],
    queryFn: () => api<PolicyVersionView>(`admin/policy/versions/${id}`),
    enabled: id !== null,
    staleTime: Infinity,
  });
}

// Labels

type FlagMeta = { label: string; short: string; tip: string; icon: IconName; tone: "neutral" | "denied" | "escalated" };

export const FLAG_META: Record<Flag, FlagMeta> = {
  prescan_signal: {
    label: "Injection suspected",
    short: "Injection",
    tip: "Injection suspected: the message screen matched instruction-like text. It was handled as data and the request went to a person.",
    icon: "shield",
    tone: "denied",
  },
  intake_injection_signal: {
    label: "Injection suspected",
    short: "Injection",
    tip: "Injection suspected: the reading model found text aimed at the system. It was handled as data and the request went to a person.",
    icon: "shield",
    tone: "denied",
  },
  foreign_order_reference: {
    label: "Someone else's order",
    short: "Other order",
    tip: "The message cited an order that belongs to another customer.",
    icon: "shield",
    tone: "denied",
  },
  low_confidence: {
    label: "Low confidence",
    short: "Low conf.",
    tip: "Low confidence: the reading model was less than 60% sure it understood the request.",
    icon: "help-circle",
    tone: "neutral",
  },
  llm_failure: {
    label: "LLM error",
    short: "LLM error",
    tip: "LLM error: an AI step failed on the main and the fallback model, so the request went to a person.",
    icon: "bolt",
    tone: "neutral",
  },
  responder_failure: {
    label: "Reply fallback",
    short: "Reply fallback",
    tip: "The reply model failed twice, so the customer got a standard written reply.",
    icon: "bolt",
    tone: "neutral",
  },
  clarification_limit: {
    label: "Still unclear",
    short: "Unclear",
    tip: "Still unclear after three clarifying questions, so a person decides.",
    icon: "help-circle",
    tone: "neutral",
  },
  no_rule_fired: {
    label: "No matching rule",
    short: "No rule",
    tip: "No policy rule applied to this request, so a person decides.",
    icon: "policy",
    tone: "neutral",
  },
};

/** Not a pipeline flag: shown whenever the customer disputed an automatic denial. */
export const DISPUTED_META: FlagMeta = {
  label: "Disputed",
  short: "Disputed",
  tip: "Disputed: the customer asked a person to review an automatic denial.",
  icon: "dispute",
  tone: "escalated",
};

/** Flags that mean the same thing to an admin are shown once. */
export function distinctFlags(flags: Flag[]): Flag[] {
  const seen = new Set<string>();
  return flags.filter((f) => {
    const key = FLAG_META[f].label;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

export const INJECTION_FLAGS: Flag[] = ["prescan_signal", "intake_injection_signal"];

/** Escalation-reason filter options, each a group of flags (any of them matches). */
export const REASON_FILTERS: Array<{ value: string; label: string; flags: Flag[]; disputed?: boolean }> = [
  { value: "injection", label: "Injection signal", flags: INJECTION_FLAGS },
  { value: "lowconf", label: "Low confidence", flags: ["low_confidence"] },
  { value: "dispute", label: "Customer dispute", flags: [], disputed: true },
  { value: "llm", label: "LLM error", flags: ["llm_failure", "responder_failure"] },
  { value: "foreign", label: "Someone else's order", flags: ["foreign_order_reference"] },
  { value: "unclear", label: "Still unclear", flags: ["clarification_limit"] },
  { value: "norule", label: "No matching rule", flags: ["no_rule_fired"] },
];

export const DETECTOR_LABELS: Record<Detector, string> = {
  role_marker: "role marker",
  instruction_override: "instruction override",
  encoded_payload: "encoded payload",
  unusual_unicode: "hidden or unusual characters",
  abnormal_length: "unusually long message",
};

export const RULE_META: Record<RuleKind, { name: string; description: string; scoped: boolean }> = {
  final_sale_not_refundable: {
    name: "Final sale items are not refundable",
    description: "Items marked final sale at checkout are never refunded.",
    scoped: false,
  },
  refund_window: {
    name: "Refund window",
    description: "Refunds are allowed within this many days of delivery (or of the order date if nothing was delivered).",
    scoped: true,
  },
  human_review_above: {
    name: "Large refunds need review",
    description: "Refunds above this amount need a person to approve them.",
    scoped: false,
  },
  damaged_or_incorrect_eligible: {
    name: "Damaged or incorrect items are eligible",
    description: "Items that arrive damaged, or that differ from what was ordered, can be refunded.",
    scoped: false,
  },
  repeat_claim_limit: {
    name: "Repeat claim limit",
    description: "Customers with this many claims in the lookback period have new claims reviewed by a person.",
    scoped: false,
  },
  conflicting_claim_escalates: {
    name: "Conflicting claims escalate",
    description: "If a claim contradicts the order records or earlier statements, a person decides.",
    scoped: false,
  },
};

export const BUILTIN_CHECKS: Record<"fail_closed" | "active_refund_exists", string> = {
  fail_closed: "Safety check: flagged requests go to a person",
  active_refund_exists: "Safety check: this item already has an approved refund",
};

/** The first 7 hex characters of a policy content hash, as the design shows it. */
export function shortHash(hash: string): string {
  return hash.slice(0, 7);
}
