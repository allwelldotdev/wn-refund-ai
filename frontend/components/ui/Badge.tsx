import type { ReactNode } from "react";

import type { RequestState } from "@/lib/api-types";
import { cn } from "@/lib/cn";

import { Icon, type IconName } from "./Icon";

export type Tone = "approved" | "denied" | "escalated" | "neutral" | "info";

const TONES: Record<Tone, { box: string; icon: string }> = {
  approved: { box: "bg-approved-bg text-approved-fg border-approved-border", icon: "text-approved-icon" },
  denied: { box: "bg-denied-bg text-denied-fg border-denied-border", icon: "text-denied-icon" },
  escalated: {
    box: "bg-escalated-bg text-escalated-fg border-escalated-border",
    icon: "text-escalated-icon",
  },
  neutral: { box: "bg-neutral-bg text-neutral-fg border-neutral-border", icon: "text-neutral-icon" },
  info: { box: "bg-info-bg text-info-fg border-info-border", icon: "text-info-icon" },
};

const STATES: Record<RequestState, { label: string; tone: Tone; icon: IconName }> = {
  approved: { label: "Approved", tone: "approved", icon: "check-circle" },
  denied: { label: "Denied", tone: "denied", icon: "x-circle" },
  escalated: { label: "Escalated", tone: "escalated", icon: "warning" },
  resolved_approved: { label: "Approved after review", tone: "approved", icon: "check-circle" },
  resolved_denied: { label: "Denied after review", tone: "denied", icon: "x-circle" },
};

export function stateLabel(state: RequestState): string {
  return STATES[state].label;
}

type PillProps = {
  tone: Tone;
  icon: IconName;
  size?: "sm" | "md";
  children: ReactNode;
  className?: string;
};

/** Rounded status pill: label + icon + colour, so it reads without colour too. */
export function Pill({ tone, icon, size = "md", children, className }: PillProps) {
  const t = TONES[tone];
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-full border font-medium whitespace-nowrap",
        size === "md" ? "h-6 gap-1.5 pr-2.5 pl-2 text-control-sm" : "h-5 gap-1 pr-2 pl-1.5 text-caption",
        t.box,
        className,
      )}
    >
      <Icon
        name={icon}
        size={size === "md" ? 14 : 12}
        strokeWidth={size === "md" ? 2.25 : 2.5}
        className={t.icon}
      />
      {children}
    </span>
  );
}

type StatusBadgeProps = {
  state: RequestState;
  size?: "sm" | "md";
  /** Overrides the default label (the customer view uses its own wording). */
  label?: string;
  className?: string;
};

export function StatusBadge({ state, size, label, className }: StatusBadgeProps) {
  const s = STATES[state];
  return (
    <Pill tone={s.tone} icon={s.icon} size={size} className={className}>
      {label ?? s.label}
    </Pill>
  );
}

type ChipProps = {
  tone?: "neutral" | "denied" | "approved" | "escalated" | "info";
  icon?: IconName;
  /** Hover/focus tooltip; also becomes the chip's accessible name. */
  tip?: string;
  dashed?: boolean;
  className?: string;
  children: ReactNode;
};

const CHIP_TONES = {
  neutral: "bg-canvas text-ink-muted border-neutral-border",
  denied: "bg-denied-bg text-denied-fg border-denied-border",
  approved: "bg-approved-bg text-approved-fg border-approved-border",
  escalated: "bg-escalated-bg text-escalated-fg border-escalated-border",
  info: "bg-info-bg text-info-fg border-info-border",
};

/** Square-cornered tag used for flags, rule outcomes and version markers. */
export function Chip({ tone = "neutral", icon, tip, dashed, className, children }: ChipProps) {
  return (
    <span
      className={cn(
        "inline-flex h-[22px] items-center gap-1 rounded-sm border pr-2 pl-1.5 text-caption font-medium whitespace-nowrap",
        dashed && "border-dashed",
        CHIP_TONES[tone],
        tip && "wn-tip",
        className,
      )}
      {...(tip ? { tabIndex: 0, role: "img", "aria-label": tip, "data-tip": tip } : {})}
    >
      {icon ? <Icon name={icon} size={12} strokeWidth={2.5} /> : null}
      {children}
    </span>
  );
}

/** Marks a message written by a support admin, in the chat and the case file. */
export function AdminTag({ onTint = false }: { onTint?: boolean }) {
  return (
    <span
      className={cn(
        "rounded-[4px] border border-info-border px-1.5 text-[11px] leading-4 font-semibold tracking-[0.06em] text-info-fg uppercase",
        onTint ? "bg-surface" : "bg-info-bg",
      )}
    >
      Admin
    </span>
  );
}
