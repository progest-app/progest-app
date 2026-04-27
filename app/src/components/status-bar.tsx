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
/** Maximum entries listed inside a violation-badge tooltip. Extra
 *  files collapse into a `+N more` line so the OS-rendered tooltip
 *  doesn't try to show 10k paths. */
const TOOLTIP_FILE_LIMIT = 25;

function listForTooltip(label: string, paths: string[]): string {
  if (paths.length === 0) return "";
  const head = paths.slice(0, TOOLTIP_FILE_LIMIT).join("\n");
  const extra = paths.length - TOOLTIP_FILE_LIMIT;
  const tail = extra > 0 ? `\n+${extra} more` : "";
  return `${label} (${paths.length}):\n${head}${tail}`;
}

export function StatusBar() {
  const { project } = useProject();
  const summary = useFlatViewSummary();
  const totals = summary.violationTotals;
  const files = summary.violationFiles;
  const hasViolations = totals.naming + totals.placement + totals.sequence > 0;

  return (
    <footer className="flex h-6 items-center gap-3 overflow-hidden border-t bg-card px-3 text-[0.625rem] text-muted-foreground">
      {/* Violation badges go first — most prominent corner of the
          window, easiest to glance at. ViolationBadges' optional
          per-category title shows the contributing file paths on
          hover (capped at TOOLTIP_FILE_LIMIT to keep the OS tooltip
          manageable on huge projects). */}
      <span className="flex shrink-0 items-center">
        {hasViolations ? (
          <ViolationBadges
            counts={totals}
            titles={{
              naming: listForTooltip("naming violations", files.naming),
              placement: listForTooltip("placement violations", files.placement),
              sequence: listForTooltip("sequence violations", files.sequence),
            }}
          />
        ) : (
          <span className="text-foreground/60">no violations</span>
        )}
      </span>

      {/* Project info shrinks (min-w-0 + truncate) so a long root
          doesn't push the active-view chip off the right edge. */}
      {project ? (
        <span className="flex min-w-0 shrink items-center gap-1 truncate" title={project.root}>
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
          className="ml-auto flex min-w-0 shrink-0 items-center gap-1 truncate"
          title={summary.activeView.query}
        >
          <Eye className="size-3 shrink-0" />
          <span className="truncate">view: {summary.activeView.name}</span>
        </span>
      ) : null}
    </footer>
  );
}
