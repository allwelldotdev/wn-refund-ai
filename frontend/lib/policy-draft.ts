import type { Rule, RuleKind } from "./api-types";
import { centsToDollarsInput, dollarsToCents } from "./format";

/**
 * The policy editor's working copy. Numbers stay as typed strings until
 * saving, so a half-typed value never snaps back. Rule order is kept: the
 * content hash (and therefore "no changes") depends on it.
 */
export type DraftRule = {
  uid: string;
  kind: RuleKind;
  enabled: boolean;
  amount: string;
  days: string;
  maxClaims: string;
  lookback: string;
  scopeKind: "all" | "category";
  category: string;
};

export type DraftField = "amount" | "days" | "maxClaims" | "lookback" | "category";
export type DraftErrors = Record<string, Partial<Record<DraftField, string>>>;

/** Ranges the API enforces (`domain::policy::Policy::validate`). */
export const LIMITS = {
  days: [1, 365],
  maxClaims: [1, 100],
  lookback: [1, 3650],
} as const;

let seq = 0;
const uid = () => `r${++seq}`;

export function toDraft(rules: Rule[]): DraftRule[] {
  return rules.map((r) => {
    const base: DraftRule = {
      uid: uid(),
      kind: r.kind,
      enabled: r.enabled,
      amount: "",
      days: "",
      maxClaims: "",
      lookback: "",
      scopeKind: "all",
      category: "",
    };
    switch (r.kind) {
      case "human_review_above":
        return { ...base, amount: centsToDollarsInput(r.amount_cents) };
      case "refund_window":
        return {
          ...base,
          days: String(r.days),
          scopeKind: r.scope.kind,
          category: r.scope.kind === "category" ? r.scope.category : "",
        };
      case "repeat_claim_limit":
        return { ...base, maxClaims: String(r.max_claims), lookback: String(r.lookback_days) };
      default:
        return base;
    }
  });
}

export function newWindow(existing: DraftRule[]): DraftRule {
  const hasAll = existing.some((r) => r.kind === "refund_window" && r.scopeKind === "all");
  return {
    uid: uid(),
    kind: "refund_window",
    enabled: true,
    amount: "",
    days: "14",
    maxClaims: "",
    lookback: "",
    scopeKind: hasAll ? "category" : "all",
    category: "",
  };
}

function wholeNumber(value: string, [min, max]: readonly [number, number]): number | null {
  if (!/^\d+$/.test(value.trim())) return null;
  const n = Number(value);
  return n >= min && n <= max ? n : null;
}

export function windowScopeKey(r: DraftRule): string {
  return r.scopeKind === "all" ? "all" : `category:${r.category.trim().toLowerCase()}`;
}

/** Field errors, keyed by rule uid. A duplicate refund-window scope is flagged on the later card. */
export function validateDraft(draft: DraftRule[]): DraftErrors {
  const errors: DraftErrors = {};
  const set = (r: DraftRule, field: DraftField, message: string) => {
    errors[r.uid] = { ...errors[r.uid], [field]: message };
  };
  const scopes = new Set<string>();
  for (const r of draft) {
    if (r.kind === "human_review_above") {
      const cents = dollarsToCents(r.amount);
      if (cents === null || cents <= 0) set(r, "amount", "Amount must be greater than 0");
    }
    if (r.kind === "repeat_claim_limit") {
      if (wholeNumber(r.maxClaims, LIMITS.maxClaims) === null) set(r, "maxClaims", "Max claims must be a whole number from 1 to 100");
      if (wholeNumber(r.lookback, LIMITS.lookback) === null) set(r, "lookback", "Lookback must be between 1 and 3650 days");
    }
    if (r.kind === "refund_window") {
      if (wholeNumber(r.days, LIMITS.days) === null) set(r, "days", "Days must be between 1 and 365");
      if (r.scopeKind === "category" && !r.category.trim()) set(r, "category", "Enter a product category");
      else {
        const key = windowScopeKey(r);
        if (scopes.has(key)) set(r, "category", "A refund window for this scope already exists");
        scopes.add(key);
      }
    }
  }
  return errors;
}

export function errorCount(errors: DraftErrors): number {
  return Object.values(errors).reduce((n, e) => n + Object.keys(e).length, 0);
}

/** The API's rule list, or null while any number is invalid. */
export function fromDraft(draft: DraftRule[]): Rule[] | null {
  const out: Rule[] = [];
  for (const r of draft) {
    switch (r.kind) {
      case "human_review_above": {
        const cents = dollarsToCents(r.amount);
        if (cents === null || cents <= 0) return null;
        out.push({ kind: r.kind, enabled: r.enabled, amount_cents: cents });
        break;
      }
      case "refund_window": {
        const days = wholeNumber(r.days, LIMITS.days);
        if (days === null) return null;
        const scope =
          r.scopeKind === "all" ? ({ kind: "all" } as const) : ({ kind: "category", category: r.category.trim() } as const);
        if (scope.kind === "category" && !scope.category) return null;
        out.push({ kind: r.kind, enabled: r.enabled, days, scope });
        break;
      }
      case "repeat_claim_limit": {
        const max = wholeNumber(r.maxClaims, LIMITS.maxClaims);
        const lookback = wholeNumber(r.lookback, LIMITS.lookback);
        if (max === null || lookback === null) return null;
        out.push({ kind: r.kind, enabled: r.enabled, max_claims: max, lookback_days: lookback });
        break;
      }
      default:
        out.push({ kind: r.kind, enabled: r.enabled });
    }
  }
  return out;
}

function canonical(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value && typeof value === "object") {
    const entries = Object.entries(value as Record<string, unknown>).sort(([a], [b]) => a.localeCompare(b));
    return `{${entries.map(([k, v]) => `${JSON.stringify(k)}:${canonical(v)}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

/** Same rules in the same order, ignoring key order (what the API's no-op check compares). */
export function sameRules(a: Rule[], b: Rule[]): boolean {
  return canonical(a) === canonical(b);
}

const SERVER_FIELDS: Record<string, DraftField> = {
  amount_cents: "amount",
  days: "days",
  max_claims: "maxClaims",
  lookback_days: "lookback",
  "scope.category": "category",
};

/** Maps the API's 422 paths (`rules[1].days`, relative to the Policy object) onto draft cards. */
export function serverErrors(
  draft: DraftRule[],
  fields: { path: string; message: string }[],
): { byRule: DraftErrors; other: string[] } {
  const byRule: DraftErrors = {};
  const other: string[] = [];
  for (const f of fields) {
    const m = /^rules\[(\d+)\]\.(.+)$/.exec(f.path);
    const rule = m ? draft[Number(m[1])] : undefined;
    const field = m ? SERVER_FIELDS[m[2]] : undefined;
    if (rule && field) byRule[rule.uid] = { ...byRule[rule.uid], [field]: f.message };
    else other.push(f.path ? `${f.path}: ${f.message}` : f.message);
  }
  return { byRule, other };
}
