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

/** Confirms an admin decision on an escalation. The note is required and kept in the audit log. */
export function ResolveDialog({ target, onClose }: { target: ResolveTarget | null; onClose: () => void }) {
  const [note, setNote] = useState("");
  const [error, setError] = useState<string | null>(null);
  const noteRef = useRef<HTMLTextAreaElement>(null);
  const queryClient = useQueryClient();
  const toast = useToast();

  const mutation = useMutation({
    mutationFn: (t: ResolveTarget) =>
      api<ResolveResponse>(`admin/requests/${encodeURIComponent(t.ref)}/resolve`, {
        method: "POST",
        json: { resolution: t.resolution, note: note.trim() },
      }),
    onSuccess: (_, t) => {
      toast({
        tone: "success",
        title: `${t.resolution === "approved" ? "Approved" : "Denied"} ${t.ref}`,
        body: "The customer sees the new status in their requests.",
      });
      void queryClient.invalidateQueries({ queryKey: ["admin"] });
      close();
    },
    onError: (e) => {
      if (isApiError(e, 409) || isApiError(e, 404)) {
        toast({ tone: "error", title: `Couldn't decide ${target?.ref}`, body: e.message });
        void queryClient.invalidateQueries({ queryKey: ["admin"] });
        close();
      } else {
        setError(isApiError(e) ? (e.fields[0]?.message ?? e.message) : "The request didn't go through. Try again.");
        noteRef.current?.focus();
      }
    },
  });

  function close() {
    setNote("");
    setError(null);
    mutation.reset();
    onClose();
  }

  function submit() {
    if (!target || mutation.isPending) return;
    if (note.trim().length < MIN_NOTE) {
      setError("Write a short note (at least 10 characters) explaining the decision.");
      noteRef.current?.focus();
      return;
    }
    mutation.mutate(target);
  }

  if (!target) return null;
  const approve = target.resolution === "approved";
  const amount = formatCents(target.amountCents);
  const summary = [target.customer, target.orderRef, target.amountCents !== null ? amount : null].filter(Boolean).join(" · ");
  const id = "resolve-note";
  return (
    <Dialog
      open
      onClose={close}
      busy={mutation.isPending}
      onSubmit={submit}
      initialFocus={noteRef}
      icon={approve ? "check-circle" : "x-circle"}
      iconTone={approve ? "approved" : "danger"}
      title={`${approve ? "Approve" : "Deny"} refund for ${target.ref}?`}
      description={`${summary}. The customer sees the decision in their requests; this note stays in the audit log.`}
      actions={
        <>
          <Button onClick={close} disabled={mutation.isPending}>
            Cancel
          </Button>
          <Button
            type="submit"
            variant={approve ? "primary" : "danger"}
            loading={mutation.isPending}
            loadingText={approve ? "Approving…" : "Denying…"}
          >
            {approve ? `Approve ${target.amountCents !== null ? amount : "refund"}` : "Deny refund"}
          </Button>
        </>
      }
    >
      <Field
        id={id}
        label={approve ? "Note for the audit log" : "Reason for the audit log"}
        required
        help="Saved with your name and the time. At least 10 characters."
        error={error}
      >
        <Textarea
          ref={noteRef}
          id={id}
          value={note}
          invalid={!!error}
          readOnly={mutation.isPending}
          aria-describedby={describedBy(id, { help: true, error: !!error })}
          placeholder={approve ? "e.g. Photo of the damage checked; within the refund window." : "e.g. The booking was used in full, so the claim doesn't match our records."}
          onChange={(e) => {
            setNote(e.target.value);
            setError(null);
          }}
        />
      </Field>
    </Dialog>
  );
}
