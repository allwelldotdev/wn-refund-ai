"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import Link from "next/link";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { useRef, useState } from "react";

import { SectionError } from "@/components/admin/common";
import { Button, buttonClasses } from "@/components/ui/Button";
import { Field, TextInput, describedBy } from "@/components/ui/Field";
import { Dialog } from "@/components/ui/Overlay";
import { Alert, EmptyState, LoadingRegion, Skeleton } from "@/components/ui/Surface";
import { useToast } from "@/components/ui/Toast";
import { useCurrentPolicy, usePolicyVersion, usePolicyVersions } from "@/lib/admin";
import type { PolicyVersion, PolicyVersionView } from "@/lib/api-types";
import { api, isApiError } from "@/lib/bff";

import { type EditorState, PolicyEditor, startEditing } from "./PolicyEditor";
import { PolicyArticle, VersionHistory } from "./PolicyView";

/**
 * The refund policy: the current version (or any past one, read-only), its
 * generated text, the version history, editing with a live preview, and
 * reverting. Every save and revert appends a version; nothing is overwritten.
 */
export function PolicySection() {
  const router = useRouter();
  const pathname = usePathname();
  const params = useSearchParams();
  const queryClient = useQueryClient();
  const toast = useToast();

  const current = useCurrentPolicy();
  const versions = usePolicyVersions();
  const viewingNumber = params.get("v") ? Number(params.get("v")) : null;
  const tab = params.get("tab") === "text" ? "text" : "rules";
  const fromRef = params.get("from");
  const viewingMeta = viewingNumber !== null ? versions.data?.find((v) => v.version === viewingNumber) : undefined;
  const past = usePolicyVersion(viewingMeta && viewingMeta.id !== current.data?.version.id ? viewingMeta.id : null);

  const [editor, setEditor] = useState<EditorState | null>(null);
  const [editing, setEditing] = useState(false);
  const [conflict, setConflict] = useState<PolicyVersionView | null>(null);
  const [revertTarget, setRevertTarget] = useState<PolicyVersion | null>(null);

  function setUrl(next: { v?: number | null; tab?: "rules" | "text"; from?: string | null }) {
    const p = new URLSearchParams(params);
    const v = next.v === undefined ? viewingNumber : next.v;
    const t = next.tab ?? tab;
    const f = next.from === undefined ? fromRef : next.from;
    for (const [k, val] of [
      ["v", v === null ? null : String(v)],
      ["tab", t === "text" ? "text" : null],
      ["from", f],
    ] as const) {
      if (val === null) p.delete(k);
      else p.set(k, val);
    }
    router.replace(p.size ? `${pathname}?${p}` : pathname, { scroll: false });
  }

  function refresh() {
    void queryClient.invalidateQueries({ queryKey: ["admin", "policy"] });
  }

  if (current.isPending || versions.isPending) {
    return (
      <LoadingRegion label="Loading Policy" className="grid grid-cols-1 gap-5 xl:grid-cols-[minmax(0,1fr)_320px]">
        <Skeleton className="h-[480px] rounded-lg" />
        <Skeleton className="h-[320px] rounded-lg" />
      </LoadingRegion>
    );
  }
  if (current.isError || versions.isError) {
    return <SectionError what="the refund policy" error={current.error ?? versions.error} onRetry={() => { void current.refetch(); void versions.refetch(); }} />;
  }
  if (!versions.data.length) {
    return (
      <EmptyState icon="policy" title="No policy version yet">
        The assistant can&apos;t decide requests until a policy exists. Restart the backend to seed version 1 from the default policy.
      </EmptyState>
    );
  }

  const cur = current.data;
  const viewingOld = viewingMeta !== undefined && viewingMeta.id !== cur.version.id;
  const shown = viewingOld ? past.data : cur;
  const draftKept = editor !== null && !editing;

  const banner = viewingOld && fromRef ? (
    <Alert tone="neutral" icon="eye" action={<Link className={buttonClasses("secondary", "sm")} href={`/admin/requests?ref=${encodeURIComponent(fromRef)}`}>Back to {fromRef}</Link>}>
      Version {viewingNumber} was used to decide {fromRef}. Read-only.
    </Alert>
  ) : viewingOld ? (
    <Alert tone="neutral" icon="eye" action={<Button size="sm" onClick={() => setUrl({ v: null, from: null })}>Back to current</Button>}>
      Viewing version {viewingNumber}, read-only. The current version is {cur.version.version}.
    </Alert>
  ) : fromRef ? (
    <Alert tone="neutral" icon="eye" action={<Link className={buttonClasses("secondary", "sm")} href={`/admin/requests?ref=${encodeURIComponent(fromRef)}`}>Back to {fromRef}</Link>}>
      Version {cur.version.version} (current) was used to decide {fromRef}.
    </Alert>
  ) : null;

  return (
    <div className="flex flex-col gap-4">
      {draftKept ? (
        <Alert
          tone="info"
          title="You have unsaved changes"
          action={
            <Button
              size="sm"
              variant="primary"
              onClick={() => {
                // Carry the draft onto the latest version and keep editing.
                if (editor) setEditor({ ...editor, base: cur });
                setConflict(null);
                setEditing(true);
                setUrl({ v: null, from: null });
              }}
            >
              Back to my draft
            </Button>
          }
        >
          Compare them with version {cur.version.version}, then go back to finish.
        </Alert>
      ) : null}
      {!editing ? banner : null}

      {editing && editor ? (
        <PolicyEditor
          state={editor}
          onChange={setEditor}
          conflict={conflict}
          onConflict={(latest) => {
            setConflict(latest);
            refresh();
          }}
          onViewLatest={() => {
            setEditing(false);
            setConflict(null);
            refresh();
            setUrl({ v: null, tab: "rules", from: null });
          }}
          onCancel={() => {
            setEditor(null);
            setEditing(false);
            setConflict(null);
          }}
          onSaved={(saved) => {
            setEditor(null);
            setEditing(false);
            setConflict(null);
            refresh();
            setUrl({ v: null, tab: "rules", from: null });
            toast({ tone: "success", title: `Policy version ${saved.version.version} saved`, body: "New requests use it from now on." });
          }}
        />
      ) : (
        <div className="grid grid-cols-1 gap-5 xl:grid-cols-[minmax(0,1fr)_320px]">
          {shown ? (
            <PolicyArticle
              view={shown}
              current={!viewingOld}
              readOnly={viewingOld || draftKept}
              tab={tab}
              onTab={(t) => setUrl({ tab: t })}
              onEdit={() => {
                setEditor(startEditing(cur));
                setEditing(true);
              }}
              onRevert={() => viewingMeta && setRevertTarget(viewingMeta)}
            />
          ) : past.isError ? (
            <SectionError what={`version ${viewingNumber}`} error={past.error} onRetry={() => void past.refetch()} />
          ) : (
            <Skeleton className="h-[480px] rounded-lg" />
          )}
          <VersionHistory
            versions={versions.data}
            currentId={cur.version.id}
            viewingId={viewingOld && viewingMeta ? viewingMeta.id : cur.version.id}
            editing={draftKept}
            onView={(v) => setUrl({ v: v.id === cur.version.id ? null : v.version, from: null })}
            onRevert={(v) => {
              setUrl({ v: v.version, from: null });
              setRevertTarget(v);
            }}
          />
        </div>
      )}

      <RevertDialog
        target={revertTarget}
        nextVersion={cur.version.version + 1}
        onClose={() => setRevertTarget(null)}
        onDone={(created, from) => {
          setRevertTarget(null);
          refresh();
          setUrl({ v: null, tab: "rules", from: null });
          toast({ tone: "success", title: `Version ${created.version.version} created from version ${from}` });
        }}
      />
    </div>
  );
}

function RevertDialog({
  target,
  nextVersion,
  onClose,
  onDone,
}: {
  target: PolicyVersion | null;
  nextVersion: number;
  onClose: () => void;
  onDone: (created: PolicyVersionView, from: number) => void;
}) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const value = note ?? (target ? `Reverted to version ${target.version}.` : "");
  const revert = useMutation({
    mutationFn: (t: PolicyVersion) =>
      api<PolicyVersionView>(`admin/policy/versions/${t.id}/revert`, { method: "POST", json: { change_note: value.trim() } }),
    onSuccess: (created, t) => {
      setNote(null);
      onDone(created, t.version);
    },
    onError: (e) => {
      setError(
        isApiError(e, 409) && e.code === "no_op"
          ? `Version ${target?.version} has the same rules as the current policy, so there is nothing to revert.`
          : isApiError(e)
            ? (e.fields[0]?.message ?? e.message)
            : "The revert didn't go through. Try again.",
      );
    },
  });
  if (!target) return null;
  const close = () => {
    setNote(null);
    setError(null);
    revert.reset();
    onClose();
  };
  return (
    <Dialog
      open
      onClose={close}
      busy={revert.isPending}
      initialFocus={cancelRef}
      icon="revert"
      title={`Revert to version ${target.version}?`}
      description={`This creates version ${nextVersion} with the rules from version ${target.version}. Current and past versions stay in history.`}
      onSubmit={() => {
        if (!value.trim()) {
          setError("Add a change note.");
          return;
        }
        revert.mutate(target);
      }}
      actions={
        <>
          <Button ref={cancelRef} onClick={close} disabled={revert.isPending}>
            Cancel
          </Button>
          <Button type="submit" variant="primary" loading={revert.isPending} loadingText="Reverting…">
            Revert to version {target.version}
          </Button>
        </>
      }
    >
      <Field id="revert-note" label="Change note" required help="Shown in the version history." error={error}>
        <TextInput
          id="revert-note"
          value={value}
          placeholder="Why are you reverting?"
          invalid={!!error}
          aria-describedby={describedBy("revert-note", { help: true, error: !!error })}
          onChange={(e) => {
            setNote(e.target.value);
            setError(null);
          }}
        />
      </Field>
    </Dialog>
  );
}
