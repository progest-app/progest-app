import { Eye, Folder } from "lucide-react";

import { useBackgroundTasks } from "@/lib/background-task-context";
import { useFlatViewSummary } from "@/lib/flat-view-context";
import { useProject } from "@/lib/project-context";
import { ViolationBadges } from "@/components/violation-badges";
import { DotmSquare20 } from "@/components/ui/dotm-square-20";

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
  const { tasks } = useBackgroundTasks();
  const totals = summary.violationTotals;
  const files = summary.violationFiles;
  const hasViolations = totals.naming + totals.placement + totals.sequence > 0;

  const activeTask = tasks[0];

  return (
    <footer className="flex h-6 items-center gap-3 overflow-hidden border-t bg-card px-3 text-[0.625rem] text-muted-foreground">
      {activeTask ? (
        <span className="flex shrink-0 items-center gap-1.5 text-foreground">
          <DotmSquare20 size={12} dotSize={1.5} animated />
          <span className="truncate">
            {activeTask.label}
            {activeTask.total > 0 && ` ${activeTask.current}/${activeTask.total}`}
          </span>
        </span>
      ) : hasViolations ? (
        <span className="flex shrink-0 items-center">
          <ViolationBadges
            counts={totals}
            titles={{
              naming: listForTooltip("naming violations", files.naming),
              placement: listForTooltip("placement violations", files.placement),
              sequence: listForTooltip("sequence violations", files.sequence),
            }}
          />
        </span>
      ) : (
        <span className="flex shrink-0 items-center">
          <span className="text-foreground/60">no violations</span>
        </span>
      )}

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
