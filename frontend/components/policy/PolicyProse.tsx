import type { ReactNode } from "react";

import { cn } from "@/lib/cn";

/**
 * Renders the policy text the API generates from the rules (a small Markdown
 * subset: `#` headings, numbered items, `**bold**`, paragraphs) as React
 * elements. Nothing is injected as HTML; category names are admin-typed text.
 */
export function PolicyProse({ prose, className, compact }: { prose: string; className?: string; compact?: boolean }) {
  const blocks: ReactNode[] = [];
  let list: string[] = [];
  const flush = () => {
    if (list.length) {
      blocks.push(
        <ol key={`ol-${blocks.length}`} className="flex list-decimal flex-col gap-2 pl-5">
          {list.map((item, i) => (
            <li key={i} className="pl-1">
              {inline(item)}
            </li>
          ))}
        </ol>,
      );
      list = [];
    }
  };
  for (const raw of prose.split("\n")) {
    const line = raw.trim();
    const item = /^\d+\.\s+(.*)$/.exec(line);
    if (item) {
      list.push(item[1]);
      continue;
    }
    flush();
    if (!line) continue;
    const heading = /^(#{1,3})\s+(.*)$/.exec(line);
    if (heading) {
      const level = heading[1].length;
      blocks.push(
        <p
          key={`h-${blocks.length}`}
          role="heading"
          aria-level={level + 2}
          className={cn("font-semibold text-ink", level === 1 ? (compact ? "text-title-sm" : "text-title") : "text-title-sm")}
        >
          {inline(heading[2])}
        </p>,
      );
    } else {
      blocks.push(<p key={`p-${blocks.length}`}>{inline(line)}</p>);
    }
  }
  flush();
  return <div className={cn("flex flex-col gap-3 text-body-sm text-ink", className)}>{blocks}</div>;
}

function inline(text: string): ReactNode[] {
  return text.split(/(\*\*[^*]+\*\*)/g).map((part, i) =>
    part.startsWith("**") && part.endsWith("**") && part.length > 4 ? (
      <strong key={i} className="font-semibold">
        {part.slice(2, -2)}
      </strong>
    ) : (
      part
    ),
  );
}
