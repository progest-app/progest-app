import * as React from "react";
import { Folder, Loader2, Eye, AlertTriangle, AlertCircle } from "lucide-react";

import { useProject } from "@/lib/project-context";
import { useFlatViewSummary } from "@/lib/flat-view-context";

/**
 * Tiny inline badge used in the status bar for warning / error
 * counts. Styled like the violation chips in `ViolationBadges` —
 * rounded pill, semantic-token tinted background. Kept local to
 * the status bar; if a third caller appears, hoist into the shared
 * components directory.
 */
function Badge(props: {
  tone: "warning" | "destructive";
  title?: string;
  children: React.ReactNode;
}) {
  const tones: Record<typeof props.tone, string> = {
    warning: "bg-warning/15 text-warning",
    destructive: "bg-destructive/15 text-destructive",
  };
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-full px-1.5 py-0.5 ${tones[props.tone]}`}
      title={props.title}
    >
      {props.children}
    </span>
  );
}

/**
 * Bottom-of-window status row. Always visible — no project shows
 * "No project attached", a loaded project shows root + active view +
 * hit summary + warnings. Read-only; actions live on the TopBar /
 * inside the palette.
 */
export function StatusBar() {
  const { project } = useProject();
  const summary = useFlatViewSummary();

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

      {/* Right section never shrinks — badges + counts stay visible
          even when the left side is wide. `gap-2` between badges,
          `shrink-0` on the row so flex doesn't squeeze counts to 0. */}
      <span className="ml-auto flex shrink-0 items-center gap-2">
        {summary.warnings.length > 0 ? (
          <Badge tone="warning" title={summary.warnings.join("\n")}>
            <AlertTriangle className="inline size-2.5" /> {summary.warnings.length}
          </Badge>
        ) : null}
        {summary.parseError ? (
          <Badge tone="destructive" title={summary.parseError}>
            <AlertCircle className="inline size-2.5" /> parse error
          </Badge>
        ) : null}
        {summary.error ? (
          <Badge tone="destructive" title={summary.error}>
            <AlertCircle className="inline size-2.5" /> error
          </Badge>
        ) : null}
        {summary.loading ? (
          <span className="flex items-center gap-1">
            <Loader2 className="size-3 animate-spin" /> searching…
          </span>
        ) : summary.hitCount !== null ? (
          <span>
            {summary.hitCount.toLocaleString()} hit
            {summary.hitCount === 1 ? "" : "s"}
          </span>
        ) : null}
      </span>
    </footer>
  );
}
