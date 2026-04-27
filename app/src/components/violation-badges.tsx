import { Triangle, Hash } from "lucide-react";

import type { RichViolationCounts } from "@/lib/ipc";

/**
 * Full violation badges with category icon + count. Designed for
 * inline rows where horizontal space is plentiful (palette results,
 * flat list).
 */
export function ViolationBadges({ counts }: { counts: RichViolationCounts }) {
  const total = counts.naming + counts.placement + counts.sequence;
  if (total === 0) return null;
  return (
    <span className="ml-auto flex items-center gap-1 text-[0.625rem] tracking-wide">
      {counts.naming > 0 ? (
        <span className="rounded bg-amber-500/15 px-1 py-0.5 text-amber-600 dark:text-amber-300">
          <Triangle className="inline size-2.5" /> {counts.naming}
        </span>
      ) : null}
      {counts.placement > 0 ? (
        <span className="rounded bg-sky-500/15 px-1 py-0.5 text-sky-600 dark:text-sky-300">
          <Hash className="inline size-2.5" /> {counts.placement}
        </span>
      ) : null}
      {counts.sequence > 0 ? (
        <span className="rounded bg-violet-500/15 px-1 py-0.5 text-violet-600 dark:text-violet-300">
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
      {counts.naming > 0 ? <span className="size-1.5 rounded-full bg-amber-500" /> : null}
      {counts.placement > 0 ? <span className="size-1.5 rounded-full bg-sky-500" /> : null}
      {counts.sequence > 0 ? <span className="size-1.5 rounded-full bg-violet-500" /> : null}
    </span>
  );
}
