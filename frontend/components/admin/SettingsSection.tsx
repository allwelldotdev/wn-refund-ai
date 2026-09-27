"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";

import { Switch } from "@/components/ui/Field";
import { Card, LoadingRegion, Skeleton } from "@/components/ui/Surface";
import { useToast } from "@/components/ui/Toast";
import { useSettings } from "@/lib/admin";
import type { AppSettings } from "@/lib/api-types";
import { api, isApiError } from "@/lib/bff";
import { cn } from "@/lib/cn";
import { formatDateTime } from "@/lib/format";

import { SectionError } from "./common";

/** Settings that sit outside the policy versions. Changes apply to customer chats straight away. */
export function SettingsSection() {
  const settings = useSettings();
  const queryClient = useQueryClient();
  const toast = useToast();
  const save = useMutation({
    mutationFn: (allow: boolean) => api<AppSettings>("admin/settings", { method: "PUT", json: { allow_disputes: allow } }),
    onSuccess: (next) => {
      queryClient.setQueryData(["admin", "settings"], next);
      toast(
        next.allow_disputes
          ? { tone: "success", title: "Customer disputes are on", body: "Customers can now dispute automatic denials." }
          : { tone: "success", title: "Customer disputes are off", body: "Automatic denials are now final; disputes already sent stay in Escalations." },
      );
    },
    onError: (e) =>
      toast({ tone: "error", title: "Couldn't save the setting", body: isApiError(e) ? e.message : "The support service didn't respond." }),
  });

  if (settings.isPending) {
    return (
      <LoadingRegion label="Loading settings" className="flex max-w-[760px] flex-col gap-3">
        <Skeleton className="h-4 w-64" />
        <Skeleton className="h-48 rounded-lg" />
      </LoadingRegion>
    );
  }
  if (settings.isError) {
    return <SectionError what="settings" error={settings.error} onRetry={() => void settings.refetch()} />;
  }

  const s = save.isPending && save.variables !== undefined ? { ...settings.data, allow_disputes: save.variables } : settings.data;
  const on = s.allow_disputes;
  return (
    <section aria-label="Settings" className="flex max-w-[760px] flex-col gap-4">
      <p className="text-body-sm text-ink-muted">Changes apply to customer chats straight away.</p>
      <Card>
        <div className="flex items-center gap-2.5 border-b border-muted px-5 py-4">
          <h2 className="text-lead font-semibold">Customer disputes</h2>
          <span
            className={cn(
              "rounded-full border px-2 py-px text-caption font-semibold",
              on ? "border-approved-border bg-approved-bg text-approved-fg" : "border-neutral-border bg-neutral-bg text-neutral-fg",
            )}
          >
            {on ? "On" : "Off"}
          </span>
        </div>
        <div className="flex items-start justify-between gap-6 px-5 py-4">
          <div className="flex flex-col gap-1">
            <p id="set-disp-label" className="text-body-sm font-semibold">
              Allow customers to dispute automatic denials
            </p>
            <p id="set-disp-desc" className="text-meta text-ink-muted">
              When on, a customer can ask a person to review a request the assistant denied. Turn it off to make automatic
              denials final.
            </p>
          </div>
          <Switch checked={on} onChange={(next) => save.mutate(next)} labelledBy="set-disp-label" describedBy="set-disp-desc"
            disabled={save.isPending} />
        </div>
        <dl className="mx-5 mb-4 grid grid-cols-[minmax(0,160px)_minmax(0,1fr)] gap-x-4 gap-y-2 rounded-md border border-border bg-canvas px-3.5 py-3 text-meta">
          <dt className="font-medium">Automatic denials</dt>
          <dd className="text-ink-muted">
            {on
              ? "Customers can dispute once, from Your requests. The request moves to Escalations, marked Disputed."
              : "Final. Customers see “This decision is final.” and can’t dispute."}
          </dd>
          <dt className="font-medium">Denied after review</dt>
          <dd className="text-ink-muted">Always final. Decisions made by an admin can’t be disputed.</dd>
          <dt className="font-medium">Approvals</dt>
          <dd className="text-ink-muted">Not affected.</dd>
        </dl>
        <p className="border-t border-muted px-5 py-2.5 text-caption text-ink-subtle">
          {s.updated_by && s.updated_at
            ? `Last changed by ${s.updated_by} · ${formatDateTime(s.updated_at)}`
            : "Using the default setting; no admin has changed it yet."}
        </p>
      </Card>
    </section>
  );
}
