import type { HTMLAttributes, ReactNode } from "react";

import { cn } from "@/lib/cn";

import { Icon, type IconName } from "./Icon";

export function Card({ className, ...rest }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn("flex flex-col rounded-lg border border-border bg-surface shadow-xs", className)}
      {...rest}
    />
  );
}

export function CardHeader({
  eyebrow,
  title,
  aside,
  className,
}: {
  eyebrow?: ReactNode;
  title: ReactNode;
  aside?: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex items-start justify-between gap-3 px-6 pt-5 pb-4", className)}>
      <div className="flex min-w-0 flex-col gap-0.5">
        {eyebrow ? <p className="text-caption font-medium text-ink-subtle">{eyebrow}</p> : null}
        <div className="text-title-sm font-semibold">{title}</div>
      </div>
      {aside}
    </div>
  );
}

export function CardFooter({ className, ...rest }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn("flex items-center justify-between gap-3 border-t border-muted px-6 py-3.5", className)}
      {...rest}
    />
  );
}

/** Two-column key/value list used in cards and the request drawer. */
export function DefinitionList({
  rows,
  labelWidth = 100,
  className,
}: {
  rows: Array<{ label: ReactNode; value: ReactNode }>;
  labelWidth?: number;
  className?: string;
}) {
  return (
    <dl
      className={cn("grid gap-x-3 gap-y-2.5 text-body-sm", className)}
      style={{ gridTemplateColumns: `${labelWidth}px minmax(0, 1fr)` }}
    >
      {rows.map((r, i) => (
        <div key={i} className="contents">
          <dt className="text-ink-subtle">{r.label}</dt>
          <dd className="m-0 min-w-0 break-words">{r.value}</dd>
        </div>
      ))}
    </dl>
  );
}

type AlertTone = "error" | "neutral" | "info" | "warning" | "success";

const ALERT_TONES: Record<AlertTone, { box: string; title: string; icon: IconName; iconColor: string }> = {
  error: { box: "bg-denied-bg border-denied-border", title: "text-denied-fg", icon: "alert-circle", iconColor: "text-denied-icon" },
  neutral: { box: "bg-muted border-neutral-border", title: "text-ink", icon: "info-circle", iconColor: "text-ink-muted" },
  info: { box: "bg-info-tint border-info-border", title: "text-info-fg", icon: "info-circle", iconColor: "text-info-icon" },
  warning: {
    box: "bg-escalated-bg border-escalated-border",
    title: "text-escalated-fg",
    icon: "warning",
    iconColor: "text-escalated-icon",
  },
  success: {
    box: "bg-approved-bg border-approved-border",
    title: "text-approved-fg",
    icon: "check-circle",
    iconColor: "text-approved-icon",
  },
};

type AlertProps = {
  tone?: AlertTone;
  title?: ReactNode;
  icon?: IconName;
  role?: "alert" | "status";
  action?: ReactNode;
  className?: string;
  children?: ReactNode;
  id?: string;
  tabIndex?: number;
};

/** Banner / inline alert. Errors are `role="alert"`, everything else `role="status"`. */
export function Alert({ tone = "neutral", title, icon, role, action, className, children, id, tabIndex }: AlertProps) {
  const t = ALERT_TONES[tone];
  return (
    <div
      id={id}
      tabIndex={tabIndex}
      role={role ?? (tone === "error" ? "alert" : "status")}
      className={cn("flex gap-3 rounded-lg border px-4 py-3", t.box, className)}
    >
      <Icon name={icon ?? t.icon} size={20} strokeWidth={2.25} className={cn("mt-px shrink-0", t.iconColor)} />
      <div className="flex min-w-0 flex-grow flex-col gap-1">
        {title ? <p className={cn("text-body-sm font-semibold", t.title)}>{title}</p> : null}
        {children ? <div className="text-body-sm text-ink">{children}</div> : null}
        {action ? <div className="mt-1 flex flex-wrap gap-2">{action}</div> : null}
      </div>
    </div>
  );
}

type EmptyStateProps = {
  icon: IconName;
  title: ReactNode;
  children?: ReactNode;
  action?: ReactNode;
  /** `dashed` is the chat widget's lighter variant. */
  variant?: "card" | "dashed" | "plain";
  className?: string;
};

export function EmptyState({ icon, title, children, action, variant = "card", className }: EmptyStateProps) {
  return (
    <div
      className={cn(
        "flex flex-col items-center gap-3 text-center",
        variant === "card" && "rounded-lg border border-border bg-surface px-8 py-12",
        variant === "dashed" && "rounded-lg border border-dashed border-border-control px-5 py-6",
        className,
      )}
    >
      <span
        className={cn(
          "inline-flex items-center justify-center rounded-full bg-muted text-ink-muted",
          variant === "dashed" ? "size-10" : "size-12",
        )}
      >
        <Icon name={icon} size={variant === "dashed" ? 20 : 22} />
      </span>
      <h2 className={cn("mt-1 font-semibold", variant === "dashed" ? "text-lead" : "text-title-sm")}>{title}</h2>
      {children ? <div className="max-w-[300px] text-body-sm text-ink-muted">{children}</div> : null}
      {action ? <div className="mt-2">{action}</div> : null}
    </div>
  );
}

/** Pulsing placeholder block; the container announces loading via `LoadingRegion`. */
export function Skeleton({ className, style }: { className?: string; style?: React.CSSProperties }) {
  return <span aria-hidden="true" className={cn("block animate-wn-pulse rounded-sm bg-skeleton", className)} style={style} />;
}

export function LoadingRegion({
  label,
  className,
  children,
}: {
  label: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <div aria-busy="true" className={className}>
      <span className="sr-only">{label}</span>
      {children}
    </div>
  );
}

export function Avatar({ name, size = 32 }: { name: string; size?: number }) {
  const initials = name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((p) => p[0]?.toUpperCase())
    .join("");
  return (
    <span
      aria-hidden="true"
      className="inline-flex shrink-0 items-center justify-center rounded-full bg-muted text-caption font-semibold text-ink-muted"
      style={{ width: size, height: size }}
    >
      {initials}
    </span>
  );
}
