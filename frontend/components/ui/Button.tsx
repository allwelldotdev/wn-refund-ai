import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from "react";

import { cn } from "@/lib/cn";

import { Icon, type IconName, Spinner } from "./Icon";

export type ButtonVariant =
  | "primary"
  | "secondary"
  | "danger"
  | "danger-outline"
  | "ghost"
  | "muted"
  | "text";
export type ButtonSize = "sm" | "md" | "lg";

const VARIANTS: Record<ButtonVariant, string> = {
  primary:
    "border font-medium border-primary bg-primary text-white hover:border-primary-hover hover:bg-primary-hover",
  secondary:
    "border font-medium border-border-control bg-surface text-ink hover:border-border-strong hover:bg-canvas",
  danger:
    "border font-medium border-danger bg-danger text-white hover:border-danger-hover hover:bg-danger-hover",
  "danger-outline": "border font-medium border-danger bg-surface text-denied-fg hover:bg-denied-bg",
  ghost: "border font-medium border-transparent bg-transparent text-ink hover:bg-muted",
  muted: "border font-medium border-transparent bg-muted text-ink hover:bg-neutral-border",
  text: "font-semibold bg-transparent text-ink underline underline-offset-3 hover:bg-muted",
};

const SIZES: Record<ButtonSize, string> = {
  sm: "h-8 px-3 text-control-sm",
  md: "h-10 px-4 text-body-sm",
  lg: "h-11 px-5 text-lead",
};

const DISABLED =
  "disabled:cursor-not-allowed disabled:border-border disabled:bg-muted disabled:text-ink-disabled disabled:no-underline";

/** Class list for anything that should look like a button (e.g. a Next `<Link>`). */
export function buttonClasses(
  variant: ButtonVariant = "secondary",
  size: ButtonSize = "md",
  className?: string,
) {
  return cn(
    "inline-flex shrink-0 items-center justify-center gap-2 rounded-md whitespace-nowrap",
    "transition-colors duration-120 ease-standard",
    variant === "text" ? "h-7 px-2.5 text-control-sm" : SIZES[size],
    VARIANTS[variant],
    DISABLED,
    className,
  );
}

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant;
  size?: ButtonSize;
  /** Keeps the button enabled but busy: spinner, present-tense label, `aria-busy`. */
  loading?: boolean;
  loadingText?: ReactNode;
  icon?: IconName;
};

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  {
    variant = "secondary",
    size = "md",
    loading = false,
    loadingText,
    icon,
    className,
    children,
    type = "button",
    onClick,
    ...rest
  },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type}
      aria-busy={loading || undefined}
      className={buttonClasses(variant, size, cn(loading && "cursor-progress", className))}
      onClick={loading ? (e) => e.preventDefault() : onClick}
      {...rest}
    >
      {loading ? <Spinner /> : icon ? <Icon name={icon} size={16} /> : null}
      {loading && loadingText ? loadingText : children}
    </button>
  );
});

type IconButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  icon: IconName;
  label: string;
  size?: 32 | 40 | 44;
  iconSize?: number;
};

export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton(
  { icon, label, size = 40, iconSize, className, type = "button", ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type}
      aria-label={label}
      className={cn(
        "inline-flex shrink-0 items-center justify-center rounded-md text-ink-muted",
        "transition-colors duration-120 ease-standard hover:bg-muted hover:text-ink",
        className,
      )}
      style={{ width: size, height: size }}
      {...rest}
    >
      <Icon name={icon} size={iconSize ?? (size === 32 ? 16 : 18)} />
    </button>
  );
});
