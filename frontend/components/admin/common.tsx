"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";

import { Chip } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Field, Textarea, describedBy } from "@/components/ui/Field";
import { Icon, Spinner } from "@/components/ui/Icon";
import { Dialog } from "@/components/ui/Overlay";
import { LoadingRegion, Skeleton } from "@/components/ui/Surface";
import { useToast } from "@/components/ui/Toast";
import { DISPUTED_META, FLAG_META, distinctFlags } from "@/lib/admin";
import type { Flag, ResolveResponse, ReviewInfo } from "@/lib/api-types";
import { api, isApiError } from "@/lib/bff";
import { cn } from "@/lib/cn";
import { formatCents, formatTime } from "@/lib/format";

type FlagChipsProps = { flags: Flag[]; disputed?: boolean; full?: boolean; className?: string };

export function FlagChips({ flags, disputed, full, className }: FlagChipsProps) {
  const shown = [...(disputed ? [DISPUTED_META] : []), ...distinctFlags(flags).map((f) => FLAG_META[f])];
  if (!shown.length) return null;
  return (
    <span className={cn("flex flex-wrap gap-1", className)}>
      {shown.map((m) => (
        <Chip key={m.label} tone={m.tone} icon={m.icon} tip={full ? undefined : m.tip}>
          {full ? m.label : m.short}
        </Chip>
      ))}
    </span>
  );
}

/** The red "couldn't load" card each admin section shows on a failed read. */
export function SectionError({ what, error, onRetry }: { what: string; error: unknown; onRetry: () => void }) {
  const detail = isApiError(error)
    ? `${error.message} (HTTP ${error.status}).`
    : "The support service didn't respond.";
  return (
    <div role="alert" className="flex flex-col items-start gap-3 rounded-lg border border-denied-border bg-surface p-6">
      <p className="flex items-center gap-2 text-title-sm font-semibold text-denied-fg">
        <Icon name="alert-circle" size={20} strokeWidth={2.25} className="text-denied-icon" />
        Couldn&apos;t load {what}
      </p>
      <p className="text-body-sm text-ink-muted">{detail} Nothing you did was lost.</p>
      <Button icon="refresh" onClick={onRetry}>
        Try again
      </Button>
    </div>
  );
}

export function TableSkeleton({ label, rows = 6 }: { label: string; rows?: number }) {
  return (
    <LoadingRegion label={label} className="overflow-hidden rounded-lg border border-border bg-surface">
      <div className="grid grid-cols-[110px_minmax(0,1fr)_90px_110px_80px] gap-6 border-b border-border bg-canvas px-5 py-3.5">
        {[60, 80, 56, 48, 40].map((w, i) => (
          <Skeleton key={i} className="h-3" style={{ width: w }} />
        ))}
      </div>
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="grid grid-cols-[110px_minmax(0,1fr)_90px_110px_80px] items-center gap-6 border-b border-muted px-5 py-4">
          <Skeleton className="h-3.5 w-[84px]" />
          <span className="flex flex-col gap-1.5">
            <Skeleton className="h-3.5" style={{ width: `${[62, 48, 70, 54, 66, 58][i % 6]}%` }} />
            <Skeleton className="h-2.5" style={{ width: `${[40, 34, 44, 30, 38, 36][i % 6]}%` }} />
          </span>
          <Skeleton className="h-3.5 w-16" />
          <Skeleton className="h-[22px] w-[88px] rounded-full" />
          <Skeleton className="h-3.5 w-12" />
        </div>
      ))}
    </LoadingRegion>
  );
}

/** The review model's advisory draft for an escalation (ready / drafting / failed). */
export function ReviewDraft({ review, createdAt }: { review: ReviewInfo | null | undefined; createdAt?: string }) {
  if (!review) return null;
  const meta =
    review.status === "drafted"
      ? review.model
        ? `Drafted by ${review.model}`
        : "Drafted"
      : review.status === "pending"
        ? createdAt
          ? `Started ${formatTime(createdAt)}`
          : "Started"
        : "Failed";
  return (
    <div role="group" aria-label="AI review draft, advisory only" className="flex flex-col gap-2 rounded-lg border border-border bg-canvas p-3.5">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="flex items-center gap-2 text-body-sm font-semibold">
          <Icon name="document" size={16} className="text-ink-muted" />
          AI review draft (advisory)
        </p>
        <span className="font-mono text-caption text-ink-subtle">{meta}</span>
      </div>
      {review.status === "pending" ? (
        <div aria-busy="true" className="flex flex-col gap-2">
          <p className="flex items-center gap-2 text-body-sm text-ink-muted">
            <Spinner size={14} />
            Drafting summary…
          </p>
          <Skeleton className="h-3 w-[92%]" />
          <Skeleton className="h-3 w-[70%]" />
        </div>
      ) : review.status === "failed" || !review.draft ? (
        <p className="flex items-center gap-2 text-body-sm text-ink-muted">
          <Icon name="info-circle" size={16} />
          Draft unavailable. Decide manually.
        </p>
      ) : (
        <div className="flex flex-col gap-2 text-body-sm">
          <p>{review.draft.summary}</p>
          <p>
            <span className="font-semibold">Suggested: {review.draft.suggested_resolution === "approve" ? "Approve" : "Deny"}.</span>{" "}
            {review.draft.rationale}
          </p>
          {review.draft.risk_notes.length ? (
            <ul className="list-disc pl-5 text-meta text-ink-muted">
              {review.draft.risk_notes.map((n, i) => (
                <li key={i}>{n}</li>
              ))}
            </ul>
          ) : null}
          {review.draft.questions_for_customer.length ? (
            <p className="text-meta text-ink-muted">
              Questions worth asking: {review.draft.questions_for_customer.join(" ")}
            </p>
          ) : null}
        </div>
      )}
    </div>
  );
}

export type ResolveTarget = {
  ref: string;
  customer: string;
  orderRef: string | null;
  amountCents: number | null;
  resolution: "approved" | "denied";
};

const MIN_NOTE = 10;

type NoticeDraft = { message: string; summary: string };

/** A note survives closing the dialog, e.g. while the assistant is unavailable. */
const noteKey = (ref: string) => `resolve-note:${ref}`;

function readNote(ref: string): string {
  try {
    return sessionStorage.getItem(noteKey(ref)) ?? "";
  } catch {
    return "";
  }
}

function writeNote(ref: string, note: string) {
  try {
    if (note) sessionStorage.setItem(noteKey(ref), note);
    else sessionStorage.removeItem(noteKey(ref));
  } catch {
    // Keeping the note is a convenience only.
  }
}

/**
 * Decides an escalation in two steps: the admin writes a note (kept in the
 * audit log), previews the message the assistant drafts from it for the
 * customer, then confirms. Without a draft the review can't be completed.
 */
export function ResolveDialog({ target, onClose }: { target: ResolveTarget | null; onClose: () => void }) {
  const [note, setNoteState] = useState("");
  const [loadedFor, setLoadedFor] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState<NoticeDraft | null>(null);
  const [unavailable, setUnavailable] = useState(false);
  const noteRef = useRef<HTMLTextAreaElement>(null);
  const sendRef = useRef<HTMLButtonElement>(null);
  const queryClient = useQueryClient();
  const toast = useToast();

  if (target && loadedFor !== target.ref) {
    setLoadedFor(target.ref);
    setNoteState(readNote(target.ref));
  }

  function setNote(text: string) {
    setNoteState(text);
    if (target) writeNote(target.ref, text);
  }

  function failed(e: unknown) {
    if (isApiError(e, 409) || isApiError(e, 404)) {
      toast({ tone: "error", title: `Couldn't decide ${target?.ref}`, body: e.message });
      void queryClient.invalidateQueries({ queryKey: ["admin"] });
      close();
      return true;
    }
    return false;
  }

  const drafting = useMutation({
    mutationFn: (t: ResolveTarget) =>
      api<NoticeDraft>(`admin/requests/${encodeURIComponent(t.ref)}/resolve/draft`, {
        method: "POST",
        json: { resolution: t.resolution, note: note.trim() },
      }),
    onSuccess: (d) => {
      setDraft(d);
      window.setTimeout(() => sendRef.current?.focus(), 0);
    },
    onError: (e) => {
      if (failed(e)) return;
      if (isApiError(e, 503)) setUnavailable(true);
      else setError(isApiError(e) ? (e.fields[0]?.message ?? e.message) : "The request didn't go through. Try again.");
    },
  });

  const resolving = useMutation({
    mutationFn: ({ t, d }: { t: ResolveTarget; d: NoticeDraft }) =>
      api<ResolveResponse>(`admin/requests/${encodeURIComponent(t.ref)}/resolve`, {
        method: "POST",
        json: { resolution: t.resolution, note: note.trim(), message: d.message, summary: d.summary },
      }),
    onSuccess: (_, { t }) => {
      toast({
        tone: "success",
        title: `${t.resolution === "approved" ? "Approved" : "Denied"} ${t.ref}`,
        body: "The message was sent to the customer's chat.",
      });
      writeNote(t.ref, "");
      void queryClient.invalidateQueries({ queryKey: ["admin"] });
      close();
    },
    onError: (e) => {
      if (failed(e)) return;
      setDraft(null);
      setError(isApiError(e) ? (e.fields[0]?.message ?? e.message) : "The request didn't go through. Try again.");
    },
  });

  const busy = drafting.isPending || resolving.isPending;

  function close() {
    setError(null);
    setDraft(null);
    setUnavailable(false);
    setLoadedFor(null);
    drafting.reset();
    resolving.reset();
    onClose();
  }

  function submit() {
    if (!target || busy) return;
    if (draft) {
      resolving.mutate({ t: target, d: draft });
      return;
    }
    if (note.trim().length < MIN_NOTE) {
      setError("Write a short note (at least 10 characters) explaining the decision.");
      noteRef.current?.focus();
      return;
    }
    setUnavailable(false);
    drafting.mutate(target);
  }

  if (!target) return null;
  const approve = target.resolution === "approved";
  const amount = formatCents(target.amountCents);
  const first = target.customer.split(/\s+/)[0] ?? target.customer;
  const summary = [target.customer, target.orderRef, target.amountCents !== null ? amount : null].filter(Boolean).join(" · ");
  const id = "resolve-note";
  return (
    <Dialog
      open
      onClose={close}
      busy={busy}
      onSubmit={submit}
      initialFocus={noteRef}
      icon={approve ? "check-circle" : "x-circle"}
      iconTone={approve ? "approved" : "danger"}
      title={`${approve ? "Approve" : "Deny"} refund for ${target.ref}?`}
      description={
        draft
          ? `${summary}. Check the message ${first} will get in their chat, then send it.`
          : `${summary}. Your note stays in the audit log; the assistant turns it into a message to ${first} for you to check first.`
      }
      actions={
        draft ? (
          <>
            <Button onClick={() => setDraft(null)} disabled={busy}>
              Edit note
            </Button>
            <Button ref={sendRef} type="submit" variant={approve ? "primary" : "danger"} loading={resolving.isPending}
              loadingText={approve ? "Approving…" : "Denying…"}>
              {approve ? `Approve ${target.amountCents !== null ? amount : "refund"} and send` : "Deny and send"}
            </Button>
          </>
        ) : (
          <>
            <Button onClick={close} disabled={busy}>
              Cancel
            </Button>
            <Button type="submit" variant="primary" loading={drafting.isPending} loadingText="Writing the message…">
              Preview message
            </Button>
          </>
        )
      }
    >
      {draft ? (
        <div className="flex flex-col gap-3">
          <div className="flex flex-col gap-1.5">
            <p className="text-caption font-semibold tracking-[0.05em] text-ink-subtle uppercase">Message to {first}</p>
            <p className="rounded-md border border-border bg-canvas px-3 py-2.5 text-body-sm whitespace-pre-wrap [overflow-wrap:anywhere]">
              {draft.message}
            </p>
          </div>
          <div className="flex flex-col gap-1.5">
            <p className="text-caption font-semibold tracking-[0.05em] text-ink-subtle uppercase">Note shown in the chat</p>
            <p className="rounded-md border border-dashed border-border-control px-3 py-2 text-center text-meta text-ink-muted">
              {draft.summary}
            </p>
          </div>
        </div>
      ) : (
        <div className="flex flex-col gap-3">
          {unavailable ? (
            <p role="alert" className="flex items-start gap-2 rounded-md border border-denied-border bg-denied-bg px-3 py-2.5 text-body-sm text-denied-fg">
              <Icon name="alert-circle" size={16} className="mt-0.5 shrink-0 text-denied-icon" />
              The assistant is unavailable, so this review can&apos;t be completed right now. Your note is kept.
            </p>
          ) : null}
          <Field
            id={id}
            label={approve ? "Your note: how and why you approved" : "Your note: how and why you denied"}
            required
            help="Saved in the audit log with your name and the time. At least 10 characters."
            error={error}
          >
            <Textarea
              ref={noteRef}
              id={id}
              value={note}
              invalid={!!error}
              readOnly={busy}
              aria-describedby={describedBy(id, { help: true, error: !!error })}
              placeholder={approve ? "e.g. Photo of the damage checked; within the refund window." : "e.g. The booking was used in full, so the claim doesn't match our records."}
              onChange={(e) => {
                setNote(e.target.value);
                setError(null);
              }}
            />
          </Field>
        </div>
      )}
    </Dialog>
  );
}
