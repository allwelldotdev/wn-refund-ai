import type { SVGProps } from "react";

const CIRCLE = "M3 12a9 9 0 1 0 18 0a9 9 0 1 0-18 0";

/** Stroke icons drawn on the design boards (24×24, round caps). */
const PATHS = {
  logo: "M3 6l4.5 12L12 8l4.5 10L21 6",
  "check-circle": `${CIRCLE}M8.5 12.5l2.5 2.5 4.5-5`,
  "x-circle": `${CIRCLE}M9 9l6 6M15 9l-6 6`,
  "alert-circle": `${CIRCLE}M12 8v5M12 16h.01`,
  "info-circle": `${CIRCLE}M12 11v5M12 8h.01`,
  "help-circle": `${CIRCLE}M9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.6.3-1 .9-1 1.6M12 17h.01`,
  clock: `${CIRCLE}M12 7v5l3 2`,
  ring: "M4 12a8 8 0 1 0 16 0a8 8 0 1 0-16 0",
  warning: "M12 3.5L2.8 19.5h18.4ZM12 10v4M12 17h.01",
  refresh:
    "M20 11a8 8 0 0 0-14.3-4.5L4 8M4 4v4h4M4 13a8 8 0 0 0 14.3 4.5L20 16M20 20v-4h-4",
  revert: "M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5",
  check: "M5 12.5l4.5 4.5L19 7",
  close: "M6 6l12 12M18 6L6 18",
  minus: "M6 12h12",
  plus: "M12 5v14M5 12h14",
  "chevron-down": "M6 9l6 6 6-6",
  "chevron-left": "M15 6l-6 6 6 6",
  "arrow-down": "M12 5v14M6 13l6 6 6-6",
  "arrow-up": "M12 19V5M6 11l6-6 6 6",
  search: "M4 11a7 7 0 1 0 14 0a7 7 0 1 0-14 0M20 20l-3.5-3.5",
  inbox: "M3 13h5l1.5 3h5l1.5-3h5M5.5 5h13L21 13v6H3v-6Z",
  box: "M3.5 7.5L12 3l8.5 4.5v9L12 21l-8.5-4.5ZM3.5 7.5L12 12l8.5-4.5M12 12v9",
  calendar: "M4 6h16v14H4ZM4 10h16M8 3v4M16 3v4",
  shield: "M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6ZM12 8v4M12 15h.01",
  bolt: "M13 2L4 14h7l-1 8 9-12h-7Z",
  repeat: "M17 2l4 4-4 4M3 11V9a3 3 0 0 1 3-3h15M7 22l-4-4 4-4M21 13v2a3 3 0 0 1-3 3H3",
  lock: "M5 11h14v10H5ZM8 11V7.5a4 4 0 0 1 8 0V11",
  eye: "M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12ZM9 12a3 3 0 1 0 6 0a3 3 0 1 0-6 0",
  hourglass:
    "M6 3h12M6 21h12M7 3c0 5 10 5 10 9s-10 4-10 9M17 3c0 5-10 5-10 9s10 4 10 9",
  chat: "M4 5h16v11H9l-5 4Z",
  "two-pane": "M3 5h8v14H3ZM13 5h8v14h-8Z",
  document: "M6 3h9l4 4v14H6ZM14 3v5h5M9 13h7M9 17h5",
  pencil: "M4 20h4L19 9l-4-4L4 16ZM13.5 6.5l4 4",
  trash: "M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13",
  grid: "M4 4h7v7H4ZM13 4h7v7h-7ZM4 13h7v7H4ZM13 13h7v7h-7Z",
  list: "M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01",
  policy: "M6 3h9l4 4v14H6ZM14 3v5h5M9 13h7M9 17h7",
  external: "M14 4h6v6M20 4l-9 9M18 14v6H4V6h6",
} as const;

export type IconName = keyof typeof PATHS;

type IconProps = Omit<SVGProps<SVGSVGElement>, "name"> & {
  name: IconName;
  size?: number;
  strokeWidth?: number;
};

export function Icon({ name, size = 16, strokeWidth = 2, ...rest }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      {...rest}
    >
      <path d={PATHS[name]} />
    </svg>
  );
}

export function Spinner({ size = 16, className }: { size?: number; className?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2.5}
      strokeLinecap="round"
      aria-hidden="true"
      focusable="false"
      className={`animate-wn-spin ${className ?? ""}`}
    >
      <circle cx="12" cy="12" r="9" strokeOpacity="0.25" />
      <path d="M21 12a9 9 0 0 0-9-9" />
    </svg>
  );
}

/** The Worknoon "W" mark on its dark tile. */
export function BrandMark({ size = 28, radius = 6 }: { size?: number; radius?: number }) {
  return (
    <span
      aria-hidden="true"
      className="inline-flex shrink-0 items-center justify-center bg-primary text-white"
      style={{ width: size, height: size, borderRadius: radius }}
    >
      <Icon name="logo" size={Math.round(size * 0.57)} strokeWidth={2.25} />
    </span>
  );
}
