"use client";

import {
  forwardRef,
  type InputHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
  type TextareaHTMLAttributes,
} from "react";

import { cn } from "@/lib/cn";

import { Icon } from "./Icon";

/** ids for a field's help and error text, to pass to `aria-describedby`. */
export function fieldIds(id: string) {
  return { help: `${id}-help`, error: `${id}-err` };
}

export function describedBy(id: string, opts: { help?: boolean; error?: boolean; extra?: string }) {
  const ids = fieldIds(id);
  return (
    [opts.error ? ids.error : null, opts.help && !opts.error ? ids.help : null, opts.extra]
      .filter(Boolean)
      .join(" ") || undefined
  );
}

export function FieldError({ id, children }: { id?: string; children: ReactNode }) {
  return (
    <p id={id} className="flex items-start gap-1.5 text-meta text-denied-fg">
      <Icon name="alert-circle" size={16} strokeWidth={2.25} className="mt-px shrink-0 text-denied-icon" />
      <span>{children}</span>
    </p>
  );
}

type FieldProps = {
  id: string;
  label: ReactNode;
  help?: ReactNode;
  error?: ReactNode;
  required?: boolean;
  optional?: boolean;
  disabled?: boolean;
  hideLabel?: boolean;
  className?: string;
  children: ReactNode;
};

/** Label above the control, then help or error text linked by id. */
export function Field({
  id,
  label,
  help,
  error,
  required,
  optional,
  disabled,
  hideLabel,
  className,
  children,
}: FieldProps) {
  const ids = fieldIds(id);
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <div className={cn("flex items-baseline justify-between gap-2", hideLabel && "sr-only")}>
        <label
          htmlFor={id}
          className={cn("text-body-sm font-medium", disabled ? "text-ink-subtle" : "text-ink")}
        >
          {label}
          {required ? <span className="font-normal text-ink-subtle"> (required)</span> : null}
        </label>
        {optional ? <span className="text-caption text-ink-subtle">Optional</span> : null}
      </div>
      {children}
      {error ? (
        <FieldError id={ids.error}>{error}</FieldError>
      ) : help ? (
        <p id={ids.help} className="text-caption text-ink-subtle">
          {help}
        </p>
      ) : null}
    </div>
  );
}

const CONTROL =
  "w-full rounded-md bg-surface text-ink placeholder:text-ink-subtle " +
  "focus-visible:border-focus disabled:cursor-not-allowed disabled:border-border disabled:bg-muted disabled:text-ink-disabled";

function controlBorder(invalid?: boolean) {
  return invalid ? "border-[1.5px] border-danger" : "border border-border-strong";
}

type InputProps = InputHTMLAttributes<HTMLInputElement> & {
  invalid?: boolean;
  /** 40px on admin, 44px in customer views. */
  controlSize?: "md" | "lg";
  mono?: boolean;
  /** Short adornment inside the left edge, e.g. "$". */
  prefix?: string;
};

export const TextInput = forwardRef<HTMLInputElement, InputProps>(function TextInput(
  { invalid, controlSize = "md", mono, prefix, className, ...rest },
  ref,
) {
  const input = (
    <input
      ref={ref}
      aria-invalid={invalid || undefined}
      className={cn(
        CONTROL,
        controlBorder(invalid),
        controlSize === "lg" ? "h-11 text-body" : "h-10 text-body-sm",
        prefix ? "pr-3 pl-[26px]" : "px-3",
        mono && "font-mono tabular",
        className,
      )}
      {...rest}
    />
  );
  if (!prefix) return input;
  return (
    <div className="relative flex">
      <span aria-hidden="true" className="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 font-mono text-body-sm text-ink-muted">
        {prefix}
      </span>
      {input}
    </div>
  );
});

type TextareaProps = TextareaHTMLAttributes<HTMLTextAreaElement> & { invalid?: boolean };

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaProps>(function Textarea(
  { invalid, className, rows = 3, ...rest },
  ref,
) {
  return (
    <textarea
      ref={ref}
      rows={rows}
      aria-invalid={invalid || undefined}
      className={cn(CONTROL, controlBorder(invalid), "resize-y px-3 py-2.5 text-body-sm", className)}
      {...rest}
    />
  );
});

type SelectProps = SelectHTMLAttributes<HTMLSelectElement> & { invalid?: boolean };

/** Native select with the design's chevron: free keyboard and screen-reader support. */
export const Select = forwardRef<HTMLSelectElement, SelectProps>(function Select(
  { invalid, className, children, ...rest },
  ref,
) {
  return (
    <div className="relative flex">
      <select
        ref={ref}
        aria-invalid={invalid || undefined}
        className={cn(CONTROL, controlBorder(invalid), "h-10 appearance-none pr-9 pl-3 text-body-sm", className)}
        {...rest}
      >
        {children}
      </select>
      <Icon
        name="chevron-down"
        size={16}
        className="pointer-events-none absolute top-3 right-3 text-ink-muted"
      />
    </div>
  );
});

export const SearchInput = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(
  function SearchInput({ className, ...rest }, ref) {
    return (
      <div className="relative flex">
        <Icon name="search" size={16} className="pointer-events-none absolute top-3 left-3 text-ink-subtle" />
        <input
          ref={ref}
          type="search"
          className={cn(CONTROL, controlBorder(false), "h-10 pr-3 pl-9 text-body-sm", className)}
          {...rest}
        />
      </div>
    );
  },
);

type CheckboxProps = Omit<InputHTMLAttributes<HTMLInputElement>, "type"> & { label: ReactNode };

export function Checkbox({ label, className, id, ...rest }: CheckboxProps) {
  return (
    <label htmlFor={id} className={cn("flex min-h-10 items-center gap-2.5 text-body-sm text-ink", className)}>
      <input id={id} type="checkbox" className="m-0 size-[18px] accent-primary" {...rest} />
      {label}
    </label>
  );
}

type SwitchProps = {
  checked: boolean;
  onChange: (next: boolean) => void;
  labelledBy: string;
  disabled?: boolean;
  id?: string;
};

/** On/off switch (`role="switch"`), 52×44 hit area around a 40×24 track. */
export function Switch({ checked, onChange, labelledBy, disabled, id }: SwitchProps) {
  return (
    <span className="inline-flex items-center gap-1">
      <span aria-hidden="true" className="min-w-[22px] text-right text-caption font-medium text-ink-muted">
        {checked ? "On" : "Off"}
      </span>
      <button
        id={id}
        type="button"
        role="switch"
        aria-checked={checked}
        aria-labelledby={labelledBy}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className="relative inline-flex h-11 w-[52px] items-center justify-center rounded-md disabled:cursor-not-allowed disabled:opacity-60"
      >
        <span
          className={cn(
            "relative h-6 w-10 rounded-full transition-colors duration-120 ease-standard",
            checked ? "bg-primary" : "bg-border-strong",
          )}
        >
          <span
            className={cn(
              "absolute top-[3px] size-[18px] rounded-full bg-white shadow-[0_1px_2px_rgb(21_26_24/0.2)]",
              "transition-[left] duration-120 ease-standard",
              checked ? "left-[19px]" : "left-[3px]",
            )}
          />
        </span>
      </button>
    </span>
  );
}
