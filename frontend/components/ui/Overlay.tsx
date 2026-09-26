"use client";

import { useId, useRef, type ReactNode, type RefObject } from "react";

import { cn } from "@/lib/cn";
import { useModal } from "@/lib/use-modal";

import { IconButton } from "./Button";
import { Icon, type IconName } from "./Icon";

type DrawerProps = {
  open: boolean;
  onClose: () => void;
  /** Id of the element that names the drawer (usually its title). */
  labelledBy: string;
  header: ReactNode;
  footer?: ReactNode;
  closeLabel?: string;
  children: ReactNode;
};

/**
 * Right-hand modal panel: 600px on desktop, 560px from 768px, full screen
 * below that. Scrim click, the close button and Escape all close it.
 */
export function Drawer({ open, onClose, labelledBy, header, footer, closeLabel = "Close details", children }: DrawerProps) {
  const ref = useRef<HTMLElement>(null);
  useModal(ref, open, onClose);
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-30">
      <div aria-hidden="true" className="absolute inset-0 animate-wn-fade bg-scrim" onClick={onClose} />
      <aside
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelledBy}
        tabIndex={-1}
        className={cn(
          "absolute inset-y-0 right-0 z-[31] flex w-full animate-wn-drawer flex-col bg-surface shadow-lg outline-none",
          "pt-[env(safe-area-inset-top)] md:w-[560px] md:pt-0 xl:w-[600px]",
        )}
      >
        <div className="flex items-start justify-between gap-3 border-b border-border py-4 pr-3 pl-6">
          <div className="min-w-0 flex-grow">{header}</div>
          <IconButton icon="close" label={closeLabel} size={44} onClick={onClose} />
        </div>
        <div className="flex flex-grow flex-col gap-6 overflow-y-auto px-6 py-5">{children}</div>
        {footer ? (
          <div className="flex flex-wrap justify-end gap-3 border-t border-border px-6 pt-3.5 pb-[calc(14px+env(safe-area-inset-bottom))]">
            {footer}
          </div>
        ) : null}
      </aside>
    </div>
  );
}

type DialogProps = {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  description?: ReactNode;
  icon?: IconName;
  iconTone?: "danger" | "approved" | "neutral";
  /** Element focused on open. The design's default is the safe action. */
  initialFocus?: RefObject<HTMLElement | null>;
  actions: ReactNode;
  children?: ReactNode;
  onSubmit?: () => void;
  busy?: boolean;
};

const ICON_TONES = {
  danger: "bg-denied-bg text-denied-icon",
  approved: "bg-approved-bg text-approved-icon",
  neutral: "bg-muted text-ink-muted",
};

/** Centred confirmation dialog (`role="alertdialog"`), max 480px wide. */
export function Dialog({
  open,
  onClose,
  title,
  description,
  icon,
  iconTone = "neutral",
  initialFocus,
  actions,
  children,
  onSubmit,
  busy,
}: DialogProps) {
  const ref = useRef<HTMLFormElement>(null);
  const titleId = useId();
  const descId = useId();
  useModal(ref, open, busy ? () => {} : onClose, initialFocus);
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <div aria-hidden="true" className="absolute inset-0 animate-wn-fade bg-scrim" onClick={busy ? undefined : onClose} />
      <form
        ref={ref}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={description ? descId : undefined}
        aria-busy={busy || undefined}
        noValidate
        onSubmit={(e) => {
          e.preventDefault();
          onSubmit?.();
        }}
        className="relative z-[51] flex w-full max-w-[480px] animate-wn-in flex-col gap-4 rounded-xl bg-surface p-6 shadow-lg"
      >
        <div className="flex items-start gap-3.5">
          {icon ? (
            <span className={cn("inline-flex size-10 shrink-0 items-center justify-center rounded-full", ICON_TONES[iconTone])}>
              <Icon name={icon} size={20} strokeWidth={2.25} />
            </span>
          ) : null}
          <div className="flex min-w-0 flex-col gap-1.5">
            <h2 id={titleId} className="text-title-md font-semibold">
              {title}
            </h2>
            {description ? (
              <p id={descId} className="text-body-sm text-ink-muted">
                {description}
              </p>
            ) : null}
          </div>
        </div>
        {children ? <div className={icon ? "sm:pl-[54px]" : undefined}>{children}</div> : null}
        <div className="flex flex-wrap justify-end gap-3 pt-2">{actions}</div>
      </form>
    </div>
  );
}
