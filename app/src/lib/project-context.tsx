import * as React from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

import {
  appInfo,
  IpcError,
  projectInitExisting,
  projectInitNew,
  projectOpen,
  projectRecentClear,
  projectRecentList,
  type InitResult,
  type ProgressEvent,
  type ProjectInfo,
  type RecentProject,
} from "@/lib/ipc";

type ProjectContextValue = {
  project: ProjectInfo | null;
  recent: RecentProject[];
  error: string | null;
  /** Re-probe the backend for the currently attached project. */
  refresh: () => Promise<void>;
  /** Native folder picker → project_open. No-op when the user cancels. */
  openPicker: () => Promise<void>;
  /** Open one of the recent entries (skips the picker). */
  pickRecent: (entry: RecentProject) => Promise<void>;
  clearRecent: () => Promise<void>;
  /**
   * Monotonic counter bumped whenever indexed state (violations, tags,
   * search projection) may have changed out-of-band — e.g. the
   * directory inspector ran `lint_run` after an `[accepts]` save.
   * Long-lived consumers (FlatView, TreeView) include this in their
   * effect deps so they re-fetch.
   */
  refreshTick: number;
  /** Bump [`refreshTick`]. */
  bumpRefresh: () => void;
  /** Open a project at `path` directly (skips the picker). Used by the
   *  init dialog when preview detects an existing `.progest/`. */
  openByPath: (path: string) => Promise<void>;
  /** Initialize a brand-new project (mkdir parent/name + init + scan). */
  initNew: (
    parent: string,
    name: string,
    onProgress?: (e: ProgressEvent) => void,
  ) => Promise<InitResult>;
  /** Initialize at an existing directory (init + scan). */
  initExisting: (
    path: string,
    name: string | null,
    onProgress?: (e: ProgressEvent) => void,
  ) => Promise<InitResult>;
  /** Open the create-project dialog from anywhere in the app. */
  openInitDialog: (mode: InitDialogMode) => void;
  /** Dialog open state, consumed by the dialog component. */
  initDialog: { open: boolean; mode: InitDialogMode };
  /** Dialog close handler. */
  closeInitDialog: () => void;
  /**
   * Submit a query to the FlatView. Used by the command palette when
   * the user presses Enter on a typed search — the palette closes and
   * the FlatView's input is populated with `query` so the result list
   * (including hover/click affordances and saved-view tooling) takes
   * over from the dropdown preview.
   *
   * Implemented as a versioned slot so the same query can be submitted
   * twice in a row (e.g. user types `tag:wip` → Enter, edits something
   * else, then submits the same query again to re-execute).
   */
  submitFlatViewQuery: (query: string) => void;
  /** FlatView subscribes via `useEffect` and re-runs whenever `version`
   *  changes; ignored if `null` (no submission yet this session). */
  flatViewSubmission: { query: string; version: number } | null;
};

export type InitDialogMode = "new" | "existing";

const Ctx = React.createContext<ProjectContextValue | null>(null);

export function ProjectProvider({ children }: { children: React.ReactNode }) {
  const [project, setProject] = React.useState<ProjectInfo | null>(null);
  const [recent, setRecent] = React.useState<RecentProject[]>([]);
  const [error, setError] = React.useState<string | null>(null);
  const [refreshTick, setRefreshTick] = React.useState(0);
  const bumpRefresh = React.useCallback(() => {
    setRefreshTick((n) => n + 1);
  }, []);

  const refresh = React.useCallback(async () => {
    try {
      const info = await appInfo();
      setProject(info.project);
      setError(null);
    } catch (e) {
      setProject(null);
      setError(e instanceof IpcError ? e.raw : String(e));
    }
  }, []);

  const refreshRecent = React.useCallback(async () => {
    try {
      setRecent(await projectRecentList());
    } catch (e) {
      console.warn("recent projects", e);
    }
  }, []);

  React.useEffect(() => {
    void refresh();
    void refreshRecent();
  }, [refresh, refreshRecent]);

  const attach = React.useCallback(
    async (path: string) => {
      try {
        const info = await projectOpen(path);
        setProject(info.project);
        setError(null);
        await refreshRecent();
      } catch (e) {
        setError(e instanceof IpcError ? e.raw : String(e));
      }
    },
    [refreshRecent],
  );

  const openPicker = React.useCallback(async () => {
    try {
      const picked = await openDialog({
        directory: true,
        multiple: false,
        title: "Open Progest project",
      });
      if (typeof picked !== "string") return;
      await attach(picked);
    } catch (e) {
      setError(e instanceof IpcError ? e.raw : String(e));
    }
  }, [attach]);

  const pickRecent = React.useCallback(
    async (entry: RecentProject) => {
      await attach(entry.root);
    },
    [attach],
  );

  const clearRecent = React.useCallback(async () => {
    try {
      await projectRecentClear();
      setRecent([]);
    } catch (e) {
      setError(e instanceof IpcError ? e.raw : String(e));
    }
  }, []);

  const openByPath = React.useCallback(
    async (path: string) => {
      await attach(path);
    },
    [attach],
  );

  // Init helpers do *not* swallow errors: the dialog wants to render
  // them inline next to the form fields. They still update the context
  // (project + recent) on success so callers don't need to refresh.
  const initNew = React.useCallback(
    async (parent: string, name: string, onProgress?: (e: ProgressEvent) => void) => {
      const result = await projectInitNew(parent, name, onProgress);
      setProject(result.project);
      setError(null);
      await refreshRecent();
      return result;
    },
    [refreshRecent],
  );

  const initExisting = React.useCallback(
    async (path: string, name: string | null, onProgress?: (e: ProgressEvent) => void) => {
      const result = await projectInitExisting(path, name, onProgress);
      setProject(result.project);
      setError(null);
      await refreshRecent();
      return result;
    },
    [refreshRecent],
  );

  const [initDialog, setInitDialog] = React.useState<{
    open: boolean;
    mode: InitDialogMode;
  }>({ open: false, mode: "new" });
  const openInitDialog = React.useCallback((mode: InitDialogMode) => {
    setInitDialog({ open: true, mode });
  }, []);
  const closeInitDialog = React.useCallback(() => {
    setInitDialog((d) => ({ ...d, open: false }));
  }, []);

  const [flatViewSubmission, setFlatViewSubmission] = React.useState<{
    query: string;
    version: number;
  } | null>(null);
  const submitFlatViewQuery = React.useCallback((query: string) => {
    setFlatViewSubmission((prev) => ({
      query,
      version: (prev?.version ?? 0) + 1,
    }));
  }, []);

  const value = React.useMemo<ProjectContextValue>(
    () => ({
      project,
      recent,
      error,
      refresh,
      openPicker,
      pickRecent,
      clearRecent,
      refreshTick,
      bumpRefresh,
      openByPath,
      initNew,
      initExisting,
      openInitDialog,
      initDialog,
      closeInitDialog,
      submitFlatViewQuery,
      flatViewSubmission,
    }),
    [
      project,
      recent,
      error,
      refresh,
      openPicker,
      pickRecent,
      clearRecent,
      refreshTick,
      bumpRefresh,
      openByPath,
      initNew,
      initExisting,
      openInitDialog,
      initDialog,
      closeInitDialog,
      submitFlatViewQuery,
      flatViewSubmission,
    ],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useProject(): ProjectContextValue {
  const v = React.useContext(Ctx);
  if (!v) throw new Error("useProject() outside ProjectProvider");
  return v;
}
