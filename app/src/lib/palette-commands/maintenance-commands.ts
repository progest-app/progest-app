import * as React from "react";
import { toast } from "sonner";

import { useBackgroundTasks } from "@/lib/background-task-context";
import { lintRun, rescanProject, thumbnailGenerate } from "@/lib/ipc";
import { useProject } from "@/lib/project-context";
import type { PaletteCommand } from "./types";

export function useMaintenanceCommands(): PaletteCommand[] {
  const { project, bumpRefresh } = useProject();
  const { startTask, updateTask, finishTask } = useBackgroundTasks();

  return React.useMemo<PaletteCommand[]>(() => {
    if (!project) return [];
    return [
      {
        id: "maintenance.rescan",
        title: "Rescan project",
        group: "Maintenance",
        keywords: ["rescan", "scan", "reconcile", "refresh", "sync"],
        run: async () => {
          startTask("rescan", "Rescanning project");
          try {
            const result = await rescanProject((e) => {
              updateTask("rescan", e.current, e.total, e.message);
            });
            toast.success(
              `Rescan: ${result.added} added, ${result.updated} updated, ${result.removed} removed — lint: ${result.lint_naming + result.lint_placement + result.lint_sequence} violations — thumbnails: ${result.thumb_generated} generated`,
            );
            bumpRefresh();
          } catch (e) {
            toast.error(String(e));
          } finally {
            finishTask("rescan");
          }
        },
      },
      {
        id: "maintenance.generate-thumbnails",
        title: "Generate thumbnails",
        group: "Maintenance",
        keywords: ["thumbnail", "thumb", "preview", "image"],
        run: async () => {
          startTask("thumbnails", "Generating thumbnails");
          try {
            const result = await thumbnailGenerate((e) => {
              updateTask("thumbnails", e.current, e.total, "Generating thumbnails");
            });
            toast.success(
              `Thumbnails: ${result.generated} generated, ${result.cached} cached, ${result.skipped} skipped`,
            );
            bumpRefresh();
          } catch (e) {
            toast.error(String(e));
          } finally {
            finishTask("thumbnails");
          }
        },
      },
      {
        id: "maintenance.lint",
        title: "Run lint check",
        group: "Maintenance",
        keywords: ["lint", "check", "validate", "rules"],
        run: async () => {
          startTask("lint", "Running lint");
          try {
            const result = await lintRun((e) => {
              updateTask("lint", e.current, e.total, "Checking files");
            });
            toast.success(
              `Lint: ${result.scanned} scanned — ${result.naming} naming, ${result.placement} placement, ${result.sequence} sequence`,
            );
            bumpRefresh();
          } catch (e) {
            toast.error(String(e));
          } finally {
            finishTask("lint");
          }
        },
      },
    ];
  }, [project, startTask, updateTask, finishTask, bumpRefresh]);
}
