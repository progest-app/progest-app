import * as React from "react";
import { useBackgroundTasks } from "@/lib/background-task-context";
import type { ProgressEvent } from "@/lib/ipc";

/**
 * Returns a wrapper that runs an async IPC call with progress tracking
 * in the BackgroundTaskContext (shown in StatusBar).
 *
 * Usage:
 * ```ts
 * const tracked = useTrackedIpc();
 * await tracked("thumbnails", "Generating thumbnails", () =>
 *   thumbnailGenerate((e) => tracked.progress("thumbnails", e))
 * );
 * ```
 */
export function useTrackedIpc() {
  const { startTask, updateTask, finishTask } = useBackgroundTasks();

  const run = React.useCallback(
    async <T>(id: string, label: string, fn: () => Promise<T>): Promise<T> => {
      startTask(id, label);
      try {
        return await fn();
      } finally {
        finishTask(id);
      }
    },
    [startTask, finishTask],
  );

  const progress = React.useCallback(
    (id: string, e: ProgressEvent) => {
      updateTask(id, e.current, e.total, e.message || undefined);
    },
    [updateTask],
  );

  return Object.assign(run, { progress });
}
