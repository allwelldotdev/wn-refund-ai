"use client";

import { useMutation, useQuery } from "@tanstack/react-query";
import { useEffect, useId, useRef, useState, type ReactNode } from "react";

import { Button } from "@/components/ui/Button";
import { Field, Select, Switch, TextInput, describedBy } from "@/components/ui/Field";
import { Icon } from "@/components/ui/Icon";
import { Alert, Skeleton } from "@/components/ui/Surface";
import { Tabs, tabPanelProps } from "@/components/ui/Tabs";
import { RULE_META } from "@/lib/admin";
import type { PolicyVersionView, Rule, RuleKind } from "@/lib/api-types";
import { api, isApiError } from "@/lib/bff";
import { cn } from "@/lib/cn";
import { formatUtc } from "@/lib/format";
import {
  type DraftErrors,
  type DraftField,
  type DraftRule,
  errorCount,
  fromDraft,
  newWindow,
  sameRules,
  serverErrors,
  toDraft,
  validateDraft,
} from "@/lib/policy-draft";

import { PolicyProse } from "./PolicyProse";
import { NoRuleNote } from "./PolicyView";

const FIXED_KINDS: RuleKind[] = [
  "final_sale_not_refundable",
  "human_review_above",
  "damaged_or_incorrect_eligible",
  "repeat_claim_limit",
  "conflicting_claim_escalates",
];

function draftName(r: DraftRule): string {
  if (r.kind !== "refund_window") return RULE_META[r.kind].name;
  return `Refund window · ${r.scopeKind === "all" ? "All" : r.category.trim() || "New category"}`;
}

function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value);
  useEffect(() => {
    const t = window.setTimeout(() => setV(value), ms);
    return () => window.clearTimeout(t);
  }, [value, ms]);
  return v;
}

export type EditorState = { base: PolicyVersionView; draft: DraftRule[]; note: string };

export function startEditing(base: PolicyVersionView): EditorState {
  return { base, draft: toDraft(base.rules.rules), note: "" };
}

type EditorProps = {
  state: EditorState;
  onChange: (next: EditorState) => void;
  /** The newer version that made the last save fail with 409, if any. */
  conflict: PolicyVersionView | null;
  onConflict: (latest: PolicyVersionView) => void;
  onViewLatest: () => void;
  onCancel: () => void;
  onSaved: (saved: PolicyVersionView) => void;
};

export function PolicyEditor({ state, onChange, conflict, onConflict, onViewLatest, onCancel, onSaved }: EditorProps) {
  const { base, draft, note } = state;
  const [submitted, setSubmitted] = useState(false);
  const [server, setServer] = useState<{ byRule: DraftErrors; other: string[] }>({ byRule: {}, other: [] });
  const [pane, setPane] = useState<"rules" | "preview">("rules");
  const summaryRef = useRef<HTMLDivElement>(null);
  const conflictId = conflict?.version.id ?? null;

  // Bring the conflict banner into view when a save loses the race.
  useEffect(() => {
    if (conflictId) document.getElementById("pol-conflict")?.focus();
  }, [conflictId]);

  const rules = fromDraft(draft);
  const noChanges = rules !== null && sameRules(rules, base.rules.rules);
  const local = validateDraft(draft);
  const errors: DraftErrors = { ...server.byRule };
  for (const [k, v] of Object.entries(local)) errors[k] = { ...errors[k], ...v };
  const fieldErrors = errorCount(errors);
  const noteError = submitted && !note.trim() ? "Add a change note so other admins know what changed." : null;
  const nextVersion = (conflict?.version.version ?? base.version.version) + 1;

  const save = useMutation({
    mutationFn: (payload: Rule[]) =>
      api<PolicyVersionView>("admin/policy/versions", {
        method: "POST",
        json: { base_version_id: base.version.id, rules: { rules: payload }, change_note: note.trim() },
      }),
    onSuccess: onSaved,
    onError: (e) => {
      if (isApiError(e, 409) && e.code === "stale_base" && e.latest) {
        onConflict(e.latest);
      } else if (isApiError(e, 409)) {
        setServer({ byRule: {}, other: [e.message] });
        summaryRef.current?.focus();
      } else if (isApiError(e, 422)) {
        setServer(serverErrors(draft, e.fields.length ? e.fields : [{ path: "", message: e.message }]));
        summaryRef.current?.focus();
      } else {
        setServer({ byRule: {}, other: [isApiError(e) ? e.message : "The policy wasn't saved. Try again."] });
        summaryRef.current?.focus();
      }
    },
  });

  function update(uid: string, patch: Partial<DraftRule>) {
    setServer((s) => ({ ...s, byRule: { ...s.byRule, [uid]: {} } }));
    onChange({ ...state, draft: draft.map((r) => (r.uid === uid ? { ...r, ...patch } : r)) });
  }

  function submit() {
    setSubmitted(true);
    setServer({ byRule: {}, other: [] });
    if (fieldErrors || !note.trim() || !rules) {
      window.setTimeout(() => summaryRef.current?.focus(), 0);
      return;
    }
    save.mutate(rules);
  }

  const debouncedRules = useDebounced(rules, 400);
  const preview = useQuery({
    queryKey: ["admin", "policy", "preview", JSON.stringify(debouncedRules)],
    queryFn: () => api<{ prose: string }>("admin/policy/preview", { method: "POST", json: { rules: { rules: debouncedRules } } }),
    enabled: debouncedRules !== null && debouncedRules.length > 0,
    placeholderData: (prev) => prev,
    staleTime: Infinity,
  });

  const summaryItems: Array<{ label: string; target: string | null }> = [];
  if (submitted) {
    for (const r of draft) {
      for (const [field, message] of Object.entries(errors[r.uid] ?? {})) {
        summaryItems.push({ label: `${draftName(r)}: ${message}`, target: `${r.uid}-${field}` });
      }
    }
    if (noteError) summaryItems.push({ label: "Change note: required", target: "pol-note" });
  }
  for (const o of server.other) summaryItems.push({ label: o, target: null });

  const fixed = draft.filter((r) => r.kind !== "refund_window");
  const windows = draft.filter((r) => r.kind === "refund_window");
  const missingFixed = FIXED_KINDS.filter((k) => !draft.some((r) => r.kind === k));

  const form = (
    <form
      noValidate
      aria-labelledby="pol-edit-title"
      aria-busy={save.isPending || undefined}
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
      className="flex flex-col gap-5"
    >
      {summaryItems.length ? (
        <div ref={summaryRef} id="pol-errors" tabIndex={-1} role="alert" className="flex gap-3 rounded-lg border border-denied-border bg-denied-bg px-5 py-4 outline-none">
          <Icon name="alert-circle" size={20} strokeWidth={2.25} className="mt-0.5 shrink-0 text-denied-icon" />
          <div className="flex flex-col gap-1.5">
            <p className="text-body-sm font-semibold text-denied-fg">
              {summaryItems.length === 1 ? "1 thing to fix before saving" : `${summaryItems.length} things to fix before saving`}
            </p>
            <ul className="list-disc pl-[18px] text-body-sm">
              {summaryItems.map((item, i) => (
                <li key={i}>
                  {item.target ? (
                    <button type="button" className="text-left text-denied-fg underline" onClick={() => document.getElementById(item.target!)?.focus()}>
                      {item.label}
                    </button>
                  ) : (
                    item.label
                  )}
                </li>
              ))}
            </ul>
          </div>
        </div>
      ) : null}

      <NoRuleNote />

      <fieldset className="flex flex-col gap-3">
        <legend className="mb-1 flex flex-col gap-0.5">
          <span className="text-title-sm font-semibold">Fixed rules</span>
          <span className="text-meta text-ink-muted">Turn them on or off and adjust values.</span>
        </legend>
        <ul className="grid grid-cols-1 gap-3 lg:grid-cols-2">
          {fixed.map((r) => (
            <RuleEditorCard key={r.uid} rule={r} errors={errors[r.uid] ?? {}} disabled={save.isPending} onChange={(p) => update(r.uid, p)} />
          ))}
        </ul>
      </fieldset>

      <fieldset className="flex flex-col gap-3">
        <legend className="mb-1 flex flex-col gap-0.5">
          <span className="text-title-sm font-semibold">Scoped rules</span>
          <span className="text-meta text-ink-muted">A refund window can appear more than once, each for a different scope. Every window that applies is checked, so a category window can only shorten the general one.</span>
        </legend>
        {windows.length === 0 ? (
          <p className="rounded-lg border border-dashed border-border-strong px-4 py-3 text-body-sm text-ink-muted">
            No refund windows. Without one, late requests are not denied on age.
          </p>
        ) : (
          <ul className="grid grid-cols-1 gap-3 lg:grid-cols-2">
            {windows.map((r) => (
              <RuleEditorCard
                key={r.uid}
                rule={r}
                errors={errors[r.uid] ?? {}}
                disabled={save.isPending}
                onChange={(p) => update(r.uid, p)}
                onRemove={() => {
                  onChange({ ...state, draft: draft.filter((x) => x.uid !== r.uid) });
                  window.setTimeout(() => document.getElementById("pol-add")?.focus(), 0);
                }}
              />
            ))}
          </ul>
        )}
        <AddRuleMenu
          missingFixed={missingFixed}
          disabled={save.isPending}
          onAdd={(kind) => {
            const added = kind === "refund_window" ? newWindow(draft) : { ...toDraft([defaultRule(kind)])[0] };
            onChange({ ...state, draft: [...draft, added] });
            window.setTimeout(() => document.getElementById(`${added.uid}-${kind === "refund_window" ? "days" : "switch"}`)?.focus(), 0);
          }}
        />
      </fieldset>

      <Field id="pol-note" label="Change note" required error={noteError}>
        <TextInput
          id="pol-note"
          value={note}
          readOnly={save.isPending}
          invalid={!!noteError}
          placeholder="e.g. Large refunds now need review above $600"
          aria-describedby={describedBy("pol-note", { error: !!noteError })}
          onChange={(e) => onChange({ ...state, note: e.target.value })}
        />
      </Field>

      <div className="flex flex-wrap items-center justify-between gap-3 border-t border-border pt-4">
        <p id="pol-save-hint" className="text-meta text-ink-muted">
          {conflict
            ? `Review version ${conflict.version.version} before saving`
            : noChanges
              ? "No changes from the current version"
              : fieldErrors
                ? `${fieldErrors} ${fieldErrors === 1 ? "field needs" : "fields need"} attention`
                : `Saving creates version ${nextVersion}`}
        </p>
        <div className="flex gap-3">
          <Button variant="ghost" onClick={onCancel} disabled={save.isPending}>
            Cancel
          </Button>
          <Button
            type="submit"
            variant="primary"
            aria-describedby="pol-save-hint"
            disabled={noChanges || conflict !== null}
            loading={save.isPending}
            loadingText="Saving…"
          >
            Save as version {nextVersion}
          </Button>
        </div>
      </div>
    </form>
  );

  const previewBody = (
    <div className="flex flex-col gap-3">
      {rules === null ? (
        <p className="text-meta text-ink-muted">Fix the highlighted values to update the preview.</p>
      ) : preview.isError ? (
        <p className="text-meta text-denied-fg">Preview unavailable right now.</p>
      ) : null}
      {preview.data ? (
        <PolicyProse prose={preview.data.prose} compact className={cn(rules === null && "opacity-60")} />
      ) : (
        <div aria-busy="true" className="flex flex-col gap-2">
          <Skeleton className="h-4 w-1/2" />
          <Skeleton className="h-3" />
          <Skeleton className="h-3 w-4/5" />
        </div>
      )}
    </div>
  );

  return (
    <div className="flex flex-col gap-4">
      {conflict ? (
        <Alert
          id="pol-conflict"
          tabIndex={-1}
          className="scroll-mt-4 outline-none"
          tone="error"
          title="A newer version was saved while you were editing"
          action={<Button variant="primary" size="sm" onClick={onViewLatest}>View latest</Button>}
        >
          {conflict.version.author_name ?? "Another admin"} saved version {conflict.version.version} at {formatUtc(conflict.version.created_at)}. Nothing
          of yours was saved, and your changes are kept here.
        </Alert>
      ) : null}
      <div className="grid grid-cols-1 gap-5 xl:grid-cols-[minmax(0,1fr)_360px]">
        <section className="flex flex-col gap-4 rounded-lg border border-border bg-surface p-5 shadow-xs md:p-6">
          <div className="flex flex-col gap-1">
            <h2 id="pol-edit-title" className="text-title font-semibold">
              Edit rules
            </h2>
            <p className="text-body-sm text-ink-muted">
              Based on version {base.version.version}. Saving creates version {nextVersion}; new requests use it right away. The policy text is generated from these rules.
            </p>
          </div>
          <div className="xl:hidden">
            <Tabs
              label="Edit view"
              idBase="pe"
              value={pane}
              onChange={setPane}
              items={[
                { value: "rules", label: "Rules" },
                { value: "preview", label: "Preview" },
              ]}
            />
          </div>
          <div className={cn(pane === "preview" && "max-xl:hidden")} {...(pane === "rules" ? tabPanelProps("pe", "rules") : {})}>
            {form}
          </div>
          {pane === "preview" ? (
            <div {...tabPanelProps("pe", "preview")} className="flex flex-col gap-3 outline-none xl:hidden">
              <span className="inline-flex items-center gap-1.5 self-start rounded-sm bg-muted px-2 py-1 text-caption font-medium text-ink-muted">
                <Icon name="lock" size={12} />
                Live preview · generated from rules
              </span>
              {previewBody}
            </div>
          ) : null}
        </section>
        <aside aria-labelledby="pol-preview" className="hidden xl:block">
          <div className="sticky top-0 flex flex-col gap-3 rounded-lg border border-border bg-surface p-5 shadow-xs">
            <div className="flex flex-col gap-0.5">
              <h2 id="pol-preview" className="text-title-sm font-semibold">
                Live preview
              </h2>
              <p className="text-caption text-ink-subtle">Generated from rules</p>
            </div>
            {previewBody}
          </div>
        </aside>
      </div>
    </div>
  );
}

function defaultRule(kind: RuleKind): Rule {
  switch (kind) {
    case "human_review_above":
      return { kind, enabled: true, amount_cents: 50000 };
    case "repeat_claim_limit":
      return { kind, enabled: true, max_claims: 2, lookback_days: 30 };
    case "refund_window":
      return { kind, enabled: true, days: 14, scope: { kind: "all" } };
    default:
      return { kind, enabled: true } as Rule;
  }
}

function NumberField({
  rule,
  field,
  label,
  error,
  disabled,
  prefix,
  onChange,
}: {
  rule: DraftRule;
  field: DraftField;
  label: string;
  error?: string;
  disabled: boolean;
  prefix?: string;
  onChange: (value: string) => void;
}) {
  const id = `${rule.uid}-${field}`;
  return (
    <Field id={id} label={label} error={error}>
      <TextInput
        id={id}
        mono
        prefix={prefix}
        inputMode={field === "amount" ? "decimal" : "numeric"}
        value={rule[field]}
        readOnly={disabled}
        invalid={!!error}
        aria-describedby={describedBy(id, { error: !!error })}
        className="w-[180px]"
        onChange={(e) => onChange(e.target.value)}
      />
    </Field>
  );
}

function RuleEditorCard({
  rule,
  errors,
  disabled,
  onChange,
  onRemove,
}: {
  rule: DraftRule;
  errors: Partial<Record<DraftField, string>>;
  disabled: boolean;
  onChange: (patch: Partial<DraftRule>) => void;
  onRemove?: () => void;
}) {
  const nameId = useId();
  const fields: ReactNode[] = [];
  if (rule.kind === "human_review_above") {
    fields.push(<NumberField key="a" rule={rule} field="amount" label="Refunds over (USD)" prefix="$" error={errors.amount} disabled={disabled} onChange={(v) => onChange({ amount: v })} />);
  }
  if (rule.kind === "repeat_claim_limit") {
    fields.push(<NumberField key="m" rule={rule} field="maxClaims" label="Max claims" error={errors.maxClaims} disabled={disabled} onChange={(v) => onChange({ maxClaims: v })} />);
    fields.push(<NumberField key="l" rule={rule} field="lookback" label="Lookback (days)" error={errors.lookback} disabled={disabled} onChange={(v) => onChange({ lookback: v })} />);
  }
  if (rule.kind === "refund_window") {
    fields.push(<NumberField key="d" rule={rule} field="days" label="Days" error={errors.days} disabled={disabled} onChange={(v) => onChange({ days: v })} />);
    const scopeId = `${rule.uid}-scope`;
    fields.push(
      <Field key="s" id={scopeId} label="Scope">
        <Select id={scopeId} value={rule.scopeKind} disabled={disabled} onChange={(e) => onChange({ scopeKind: e.target.value as "all" | "category" })}>
          <option value="all">All products</option>
          <option value="category">One category</option>
        </Select>
      </Field>,
    );
    if (rule.scopeKind === "category") {
      const id = `${rule.uid}-category`;
      fields.push(
        <Field key="c" id={id} label="Category" error={errors.category} help="Matches the product category, ignoring case.">
          <TextInput
            id={id}
            value={rule.category}
            readOnly={disabled}
            invalid={!!errors.category}
            placeholder="e.g. Accessories"
            aria-describedby={describedBy(id, { help: true, error: !!errors.category })}
            onChange={(e) => onChange({ category: e.target.value })}
          />
        </Field>,
      );
    } else if (errors.category) {
      fields.push(<p key="dup" className="text-meta text-denied-fg" id={`${rule.uid}-category`} tabIndex={-1}>{errors.category}</p>);
    }
  }
  return (
    <li className={cn("flex flex-col gap-3 rounded-lg border p-4", rule.enabled ? "border-border bg-surface" : "border-dashed border-border-strong bg-canvas")}>
      <div className="flex items-start justify-between gap-3">
        <h3 id={nameId} className="pt-3 text-body-sm font-semibold">
          {draftName(rule)}
        </h3>
        <div className="flex shrink-0 items-center gap-1">
          <Switch id={`${rule.uid}-switch`} checked={rule.enabled} labelledBy={nameId} disabled={disabled} onChange={(enabled) => onChange({ enabled })} />
          {onRemove ? (
            <Button variant="ghost" size="sm" icon="trash" className="text-denied-fg" aria-label={`Remove ${draftName(rule).toLowerCase()}`} disabled={disabled} onClick={onRemove}>
              Remove
            </Button>
          ) : null}
        </div>
      </div>
      <p className="-mt-2 text-meta text-ink-muted">{RULE_META[rule.kind].description}</p>
      {fields.length ? <div className="flex flex-wrap gap-3">{fields}</div> : null}
    </li>
  );
}

function AddRuleMenu({ missingFixed, disabled, onAdd }: { missingFixed: RuleKind[]; disabled: boolean; onAdd: (kind: RuleKind) => void }) {
  const [open, setOpen] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const items: RuleKind[] = ["refund_window", ...missingFixed];
  return (
    <div className="relative self-start">
      <Button
        ref={buttonRef}
        id="pol-add"
        icon="plus"
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={disabled}
        onClick={() => setOpen((o) => !o)}
      >
        Add rule
      </Button>
      {open ? (
        <ul
          role="menu"
          aria-label="Rule kinds"
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              e.stopPropagation();
              setOpen(false);
              buttonRef.current?.focus();
            }
          }}
          className="absolute top-full left-0 z-5 mt-1 flex min-w-[260px] flex-col rounded-lg border border-border bg-surface p-1 shadow-md"
        >
          {items.map((kind, i) => (
            <li key={kind} role="none">
              <button
                role="menuitem"
                type="button"
                autoFocus={i === 0}
                onClick={() => {
                  setOpen(false);
                  onAdd(kind);
                }}
                className="flex w-full flex-col items-start rounded-md px-3 py-2 text-left hover:bg-muted"
              >
                <span className="text-body-sm font-medium">{RULE_META[kind].name}</span>
                <span className="text-caption text-ink-subtle">
                  {kind === "refund_window" ? "Adds a window for all products or one category" : "This fixed rule is missing from the draft"}
                </span>
              </button>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
