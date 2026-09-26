"use client";

import { Chip } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Icon } from "@/components/ui/Icon";
import { Tabs, tabPanelProps } from "@/components/ui/Tabs";
import { RULE_META, shortHash } from "@/lib/admin";
import type { PolicyVersion, PolicyVersionView, Rule } from "@/lib/api-types";
import { cn } from "@/lib/cn";
import { formatCents, formatUtc } from "@/lib/format";

import { PolicyProse } from "./PolicyProse";

export function ruleName(rule: Rule): string {
  if (rule.kind === "refund_window") return `Refund window · ${rule.scope.kind === "all" ? "All" : rule.scope.category}`;
  return RULE_META[rule.kind].name;
}

function params(rule: Rule): Array<[string, string]> {
  switch (rule.kind) {
    case "human_review_above":
      return [["Amount", formatCents(rule.amount_cents)]];
    case "refund_window":
      return [
        ["Days", String(rule.days)],
        ["Scope", rule.scope.kind === "all" ? "All" : rule.scope.category],
      ];
    case "repeat_claim_limit":
      return [
        ["Max claims", String(rule.max_claims)],
        ["Lookback", `${rule.lookback_days} days`],
      ];
    default:
      return [];
  }
}

export function NoRuleNote() {
  return (
    <p className="flex items-center gap-2 text-body-sm text-ink-muted">
      <Icon name="info-circle" size={16} />
      Requests that match no rule are escalated for human review.
    </p>
  );
}

export function RuleCards({ rules }: { rules: Rule[] }) {
  return (
    <ul className="grid grid-cols-1 gap-3 lg:grid-cols-2">
      {rules.map((rule, i) => (
        <li
          key={i}
          className={cn(
            "flex flex-col gap-2 rounded-lg border p-4",
            rule.enabled ? "border-border bg-surface" : "border-dashed border-border-strong bg-canvas",
          )}
        >
          <div className="flex items-start justify-between gap-3">
            <h3 className={cn("text-body-sm font-semibold", !rule.enabled && "text-ink-muted")}>{ruleName(rule)}</h3>
            {rule.enabled ? (
              <Chip icon="check">Enabled</Chip>
            ) : (
              <Chip icon="minus" dashed>
                Disabled
              </Chip>
            )}
          </div>
          <p className="text-meta text-ink-muted">{RULE_META[rule.kind].description}</p>
          {params(rule).length ? (
            <dl className="flex flex-wrap gap-2">
              {params(rule).map(([k, v]) => (
                <div key={k} className="inline-flex items-center gap-1.5 rounded-sm bg-muted px-2 py-1 text-caption">
                  <dt className="text-ink-muted">{k}</dt>
                  <dd className="font-mono font-medium text-ink tabular">{v}</dd>
                </div>
              ))}
            </dl>
          ) : null}
          <p className="mt-auto text-caption font-semibold tracking-[0.05em] text-ink-subtle uppercase">
            {RULE_META[rule.kind].scoped ? "Scoped rule" : "Fixed rule"}
          </p>
        </li>
      ))}
    </ul>
  );
}

type ArticleProps = {
  view: PolicyVersionView;
  current: boolean;
  readOnly: boolean;
  tab: "rules" | "text";
  onTab: (tab: "rules" | "text") => void;
  onEdit: () => void;
  onRevert: () => void;
};

/** One policy version: header, then its rules or its generated text. */
export function PolicyArticle({ view, current, readOnly, tab, onTab, onEdit, onRevert }: ArticleProps) {
  const v = view.version;
  return (
    <article aria-labelledby="pol-title" className="flex flex-col gap-4 rounded-lg border border-border bg-surface p-5 shadow-xs md:p-6">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-1.5">
          <div className="flex flex-wrap items-center gap-2">
            <h2 id="pol-title" className="text-title font-semibold">
              Version {v.version}
            </h2>
            {current && !readOnly ? (
              <span className="rounded-full bg-primary px-2.5 py-0.5 text-caption font-medium text-white">Current</span>
            ) : (
              <span className="rounded-full bg-muted px-2.5 py-0.5 text-caption font-medium text-ink-muted">Read-only</span>
            )}
            {v.reverted_from_version ? <Chip icon="revert">Reverted from v{v.reverted_from_version}</Chip> : null}
          </div>
          <p className="flex flex-wrap items-center gap-x-2 text-meta text-ink-muted">
            <span>
              By <span className="font-semibold text-ink">{v.author_name ?? "System"}</span>
            </span>
            <span aria-hidden="true">·</span>
            <span className="font-mono tabular">{formatUtc(v.created_at)}</span>
            <span aria-hidden="true">·</span>
            <span className="font-mono" aria-label={`Content hash ${shortHash(v.content_hash)}`}>
              #{shortHash(v.content_hash)}
            </span>
          </p>
          {v.change_note ? <p className="text-body-sm">“{v.change_note}”</p> : null}
        </div>
        {current && !readOnly ? (
          <Button icon="pencil" variant="primary" onClick={onEdit}>
            Edit rules
          </Button>
        ) : !current ? (
          <Button icon="revert" onClick={onRevert}>
            Revert to version {v.version}
          </Button>
        ) : null}
      </header>

      <Tabs
        label="Policy view"
        idBase="pol"
        value={tab}
        onChange={onTab}
        items={[
          { value: "rules", label: "Rules", count: view.rules.rules.length },
          { value: "text", label: "Policy text" },
        ]}
      />
      {tab === "rules" ? (
        <div {...tabPanelProps("pol", "rules")} className="flex flex-col gap-3 outline-none">
          <NoRuleNote />
          <RuleCards rules={view.rules.rules} />
        </div>
      ) : (
        <div {...tabPanelProps("pol", "text")} className="flex flex-col gap-3 outline-none">
          <Chip icon="lock" className="self-start">
            Generated from rules · read-only
          </Chip>
          <PolicyProse prose={view.prose} />
        </div>
      )}
    </article>
  );
}

type HistoryProps = {
  versions: PolicyVersion[];
  currentId: string;
  viewingId: string;
  editing: boolean;
  onView: (v: PolicyVersion) => void;
  onRevert: (v: PolicyVersion) => void;
};

export function VersionHistory({ versions, currentId, viewingId, editing, onView, onRevert }: HistoryProps) {
  return (
    <section aria-labelledby="pol-history" className="flex flex-col gap-3">
      <h2 id="pol-history" className="text-title-sm font-semibold">
        Version history
      </h2>
      <ol className="flex flex-col gap-2">
        {versions.map((v) => {
          const current = v.id === currentId;
          const viewing = v.id === viewingId;
          return (
            <li key={v.id} className={cn("flex flex-col gap-1.5 rounded-lg border border-border bg-surface p-3", viewing && "shadow-[inset_0_0_0_1.5px_var(--color-ink)]")}>
              <div className="flex items-center justify-between gap-2">
                <button type="button" aria-current={viewing ? "true" : undefined} onClick={() => onView(v)} className="flex flex-wrap items-center gap-2 text-left text-body-sm font-semibold hover:underline">
                  Version {v.version}
                  {current ? <span className="rounded-full bg-primary px-2 py-0.5 text-caption font-medium text-white">Current</span> : null}
                  {v.reverted_from_version ? <Chip icon="revert">Reverted from v{v.reverted_from_version}</Chip> : null}
                </button>
                {!current && !editing ? (
                  <Button size="sm" variant="ghost" icon="revert" aria-label={`Revert to version ${v.version}`} onClick={() => onRevert(v)}>
                    Revert
                  </Button>
                ) : null}
              </div>
              <p className="font-mono text-caption text-ink-subtle">
                {v.author_name ?? "System"} · {formatUtc(v.created_at)}
              </p>
              {v.change_note ? <p className="text-meta text-ink-muted">{v.change_note}</p> : null}
            </li>
          );
        })}
      </ol>
    </section>
  );
}
