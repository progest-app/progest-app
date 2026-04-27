import { Folder, Eye } from "lucide-react";

import { useProject } from "@/lib/project-context";
import { useFlatViewSummary } from "@/lib/flat-view-context";
import { ViolationBadges } from "@/components/violation-badges";

/**
 * Bottom-of-window status row. Always visible — no project shows
 * "No project attached", a loaded project shows root + active view +
 * aggregate violation badges across the current FlatView result set.
 * Read-only; actions live on the TopBar / inside the palette, and
 * per-query feedback (parse error, warnings, IPC error, hit count,
 * loading spinner) lives in the FlatView header next to the input
 * that produced it.
 */
export function StatusBar() {
  const { project } = useProject();
  const summary = useFlatViewSummary();
  const totals = summary.violationTotals;
  const hasViolations = totals.naming + totals.placement + totals.sequence > 0;

  return (
    <footer className="flex h-6 items-center gap-3 overflow-hidden border-t bg-card px-3 text-[0.625rem] text-muted-foreground">
      {/* Left section can shrink (min-w-0 + shrink) so a long project
          root doesn't push the right-side badges out of view. */}
      {project ? (
        <span
          className="flex min-w-0 shrink items-center gap-1 truncate"
          title={project.root}
        >
          <Folder className="size-3 shrink-0" />
          <span className="truncate font-medium text-foreground">{project.name}</span>
          <span className="hidden truncate sm:inline">— {project.root}</span>
        </span>
      ) : (
        <span className="flex shrink-0 items-center gap-1 italic">
          <Folder className="size-3" /> No project attached
        </span>
      )}

      {summary.activeView ? (
        <span
          className="flex min-w-0 shrink items-center gap-1 truncate"
          title={summary.activeView.query}
        >
          <Eye className="size-3 shrink-0" />
          <span className="truncate">view: {summary.activeView.name}</span>
        </span>
      ) : null}

      {/* Right section never shrinks — badges stay visible even when
          the left side is wide. Reuses <ViolationBadges> so the colour
          palette (naming amber / placement sky / sequence violet)
          matches the per-row chips in the result list. */}
      <span className="ml-auto flex shrink-0 items-center gap-2">
        {hasViolations ? (
          <span className="flex items-center">
            <ViolationBadges counts={totals} />
          </span>
        ) : (
          <span className="text-foreground/60">no violations</span>
        )}
      </span>
    </footer>
  );
}
