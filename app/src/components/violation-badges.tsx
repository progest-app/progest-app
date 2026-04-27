import { Triangle, Hash } from "lucide-react";

import type { RichViolationCounts } from "@/lib/ipc";

/**
 * Full violation badges with category icon + count. Designed for
 * inline rows where horizontal space is plentiful (palette results,
 * flat list).
 *
 * Colors come from semantic tokens defined in `index.css`
 * (`--badge-naming` / `--badge-placement` / `--badge-sequence`) so
 * dark mode swaps one variable instead of every utility — see
 * `customization.md §Dark Mode` in the shadcn skill for the rule.
 */
export function ViolationBadges({ counts }: { counts: RichViolationCounts }) {
  const total = counts.naming + counts.placement + counts.sequence;
  if (total === 0) return null;
  return (
    <span className="ml-auto flex items-center gap-1 text-[0.625rem] tracking-wide">
      {counts.naming > 0 ? (
        <span className="rounded bg-badge-naming/15 px-1 py-0.5 text-badge-naming">
          <Triangle className="inline size-2.5" /> {counts.naming}
        </span>
      ) : null}
      {counts.placement > 0 ? (
        <span className="rounded bg-badge-placement/15 px-1 py-0.5 text-badge-placement">
          <Hash className="inline size-2.5" /> {counts.placement}
        </span>
      ) : null}
      {counts.sequence > 0 ? (
        <span className="rounded bg-badge-sequence/15 px-1 py-0.5 text-badge-sequence">
          ≡ {counts.sequence}
        </span>
      ) : null}
    </span>
  );
}

/**
 * Compact dot indicator used in the tree view where the rows are
 * dense and an icon-and-count badge would crowd out the filename.
 */
export function ViolationDots({ counts }: { counts: RichViolationCounts }) {
  const total = counts.naming + counts.placement + counts.sequence;
  if (total === 0) return null;
  return (
    <span className="ml-1 flex items-center gap-0.5">
      {counts.naming > 0 ? (
        <span className="size-1.5 rounded-full bg-badge-naming" />
      ) : null}
      {counts.placement > 0 ? (
        <span className="size-1.5 rounded-full bg-badge-placement" />
      ) : null}
      {counts.sequence > 0 ? (
        <span className="size-1.5 rounded-full bg-badge-sequence" />
      ) : null}
    </span>
  );
}
