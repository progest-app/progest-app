import * as React from "react";
import { FolderOpen, FolderPlus, FolderTree, Layers, Sparkles } from "lucide-react";

import { AppMenubar } from "@/components/app-menubar";
import { CommandPalette } from "@/components/command-palette";
import { DirectoryInspector } from "@/components/directory-inspector";
import { DragDropProvider, DropOverlay, useDropZone } from "@/components/drag-drop-overlay";
import { isDragOutActive } from "@/lib/use-drag-out";
import { dirPathAtPoint } from "@/components/tree-view";
import { FileInspector, type FileInspectorHandle } from "@/components/file-inspector";
import { FlatView } from "@/components/flat-view";
import { ImportModal } from "@/components/import-modal";
import { InitProjectDialog } from "@/components/init-project-dialog";
import { SettingsDialog } from "@/components/settings-dialog";
import { StatusBar } from "@/components/status-bar";
import { SequenceView } from "@/components/sequence-view";
import { TreeView } from "@/components/tree-view";
import {
  ALL_PANELS_VISIBLE,
  TitleBar,
  type PanelKey,
  type PanelVisibility,
} from "@/components/title-bar";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { TooltipProvider } from "@/components/ui/tooltip";
import { BackgroundTaskProvider, useBackgroundTasks } from "@/lib/background-task-context";
import { FlatViewSummaryProvider } from "@/lib/flat-view-context";
import { ProjectProvider, useProject } from "@/lib/project-context";
import { SettingsProvider, useSettings } from "@/lib/settings-context";
import { ThemeProvider } from "next-themes";
import type { DirEntry, RichSearchHit } from "@/lib/ipc";
import { historyUndo, historyRedo, rescanProject, IpcError } from "@/lib/ipc";
import { Toaster } from "@/components/ui/sonner";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { useMenuEvents } from "@/lib/use-menu-events";

import "./App.css";

const PANEL_VISIBILITY_KEY = "progest:panel-visibility";

/**
 * Either a directory the user is inspecting (TreeView click on a dir)
 * or a single file (TreeView / FlatView / palette click on a file).
 * Mutually exclusive — picking one clears the other so the inspector
 * pane never tries to render both at once.
 */
type Selection = { kind: "dir"; path: string } | { kind: "file"; hit: RichSearchHit } | null;

function loadPanelVisibility(): PanelVisibility {
  try {
    const raw = localStorage.getItem(PANEL_VISIBILITY_KEY);
    if (!raw) return ALL_PANELS_VISIBLE;
    const parsed = JSON.parse(raw) as Partial<PanelVisibility>;
    return {
      tree: parsed.tree ?? true,
      flat: parsed.flat ?? true,
      inspector: parsed.inspector ?? true,
    };
  } catch {
    return ALL_PANELS_VISIBLE;
  }
}

export function App() {
  return (
    <ThemeProvider
      attribute="class"
      defaultTheme="system"
      enableSystem
      storageKey="progest:theme"
      disableTransitionOnChange
    >
      <TooltipProvider delayDuration={150}>
        <ProjectProvider>
          <SettingsProvider>
            <BackgroundTaskProvider>
              <FlatViewSummaryProvider>
                <Shell />
              </FlatViewSummaryProvider>
            </BackgroundTaskProvider>
          </SettingsProvider>
        </ProjectProvider>
        <Toaster position="bottom-right" />
      </TooltipProvider>
    </ThemeProvider>
  );
}

function Shell() {
  const { project, openPicker, bumpRefresh, openInitDialog } = useProject();
  const { startTask, updateTask, finishTask } = useBackgroundTasks();
  const [selection, setSelection] = React.useState<Selection>(null);
  const [pendingConfirm, setPendingConfirm] = React.useState<{
    next: Selection;
  } | null>(null);
  const inspectorRef = React.useRef<FileInspectorHandle>(null);
  const settings = useSettings();

  const guardedSetSelection = React.useCallback(
    (next: Selection) => {
      if (
        selection?.kind === "file" &&
        inspectorRef.current?.hasPendingSuggestions() &&
        next !== selection
      ) {
        setPendingConfirm({ next });
        return;
      }
      setSelection(next);
    },
    [selection],
  );
  // Panel visibility lives at the shell level so the titlebar toggles
  // can drive the Resizable layout. Persisted to localStorage so user
  // preferences survive a reload.
  const [panels, setPanels] = React.useState<PanelVisibility>(() => loadPanelVisibility());
  React.useEffect(() => {
    localStorage.setItem(PANEL_VISIBILITY_KEY, JSON.stringify(panels));
  }, [panels]);
  const togglePanel = React.useCallback((key: PanelKey) => {
    setPanels((p) => {
      // Don't let the user hide every panel — there'd be nothing left
      // to interact with except the titlebar itself.
      const next = { ...p, [key]: !p[key] };
      if (!next.tree && !next.flat && !next.inspector) return p;
      return next;
    });
  }, []);

  // Reset selection when the user swaps projects — otherwise the
  // inspector keeps trying to read state for a path that may not
  // exist in the new project.
  React.useEffect(() => {
    setSelection(null);
  }, [project?.root]);

  const onPickFlatHit = React.useCallback(
    (hit: RichSearchHit) => {
      guardedSetSelection({ kind: "file", hit });
    },
    [guardedSetSelection],
  );

  const onPickTreeFile = React.useCallback(
    (entry: DirEntry) => {
      const hit = treeEntryToHit(entry);
      if (hit) guardedSetSelection({ kind: "file", hit });
    },
    [guardedSetSelection],
  );

  const onSelectDir = React.useCallback(
    (path: string) => {
      guardedSetSelection({ kind: "dir", path });
    },
    [guardedSetSelection],
  );

  const onFileDeleted = React.useCallback(() => {
    setSelection(null);
  }, []);

  // Clear file selection when any rename happens so the inspector
  // doesn't show stale data. bumpRefresh (fired by the rename caller)
  // will update the tree/flat views.
  React.useEffect(() => {
    function onRenamed() {
      setSelection(null);
    }
    window.addEventListener("progest:renamed", onRenamed);
    return () => window.removeEventListener("progest:renamed", onRenamed);
  }, []);

  const onSelectionUpdate = React.useCallback((updatedHit: RichSearchHit) => {
    setSelection({ kind: "file", hit: updatedHit });
  }, []);

  const selectedDir = selection?.kind === "dir" ? selection.path : "";

  const doUndo = React.useCallback(async () => {
    try {
      const entries = await historyUndo();
      if (entries.length === 0) {
        toast.info("Nothing to undo");
        return;
      }
      const first = entries[0]!;
      toast.success(
        entries.length === 1
          ? `Undo: ${first.summary}`
          : `Undo: ${entries.length} operations (${first.op_kind})`,
      );
      bumpRefresh();
      setSelection(null);
    } catch (e) {
      if (e instanceof IpcError && e.isNoProject) return;
      toast.error(`Undo failed: ${e instanceof IpcError ? e.raw : String(e)}`);
    }
  }, [bumpRefresh]);

  const doRedo = React.useCallback(async () => {
    try {
      const entries = await historyRedo();
      if (entries.length === 0) {
        toast.info("Nothing to redo");
        return;
      }
      const first = entries[0]!;
      toast.success(
        entries.length === 1
          ? `Redo: ${first.summary}`
          : `Redo: ${entries.length} operations (${first.op_kind})`,
      );
      bumpRefresh();
      setSelection(null);
    } catch (e) {
      if (e instanceof IpcError && e.isNoProject) return;
      toast.error(`Redo failed: ${e instanceof IpcError ? e.raw : String(e)}`);
    }
  }, [bumpRefresh]);

  React.useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (!(e.metaKey || e.ctrlKey) || e.key.toLowerCase() !== "z") return;
      const target = e.target as HTMLElement | null;
      if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) return;
      e.preventDefault();
      if (e.shiftKey) {
        void doRedo();
      } else {
        void doUndo();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [doUndo, doRedo]);

  const doRescan = React.useCallback(async () => {
    startTask("rescan", "Rescanning project");
    try {
      const result = await rescanProject((e) => {
        updateTask("rescan", e.current, e.total, e.message);
      });
      toast.success(
        `Rescan: ${result.added} added, ${result.updated} updated, ${result.removed} removed`,
      );
      bumpRefresh();
    } catch (e) {
      if (e instanceof IpcError && e.isNoProject) return;
      toast.error(`Rescan failed: ${e instanceof IpcError ? e.raw : String(e)}`);
    } finally {
      finishTask("rescan");
    }
  }, [bumpRefresh, startTask, updateTask, finishTask]);

  useMenuEvents({
    "menu:new-project": () => openInitDialog("new"),
    "menu:open-project": () => void openPicker(),
    "menu:new-file": () =>
      window.dispatchEvent(new CustomEvent("progest:create", { detail: { kind: "file" } })),
    "menu:new-folder": () =>
      window.dispatchEvent(new CustomEvent("progest:create", { detail: { kind: "dir" } })),
    "menu:settings": () => settings.openSettings(),
    "menu:settings-app": () => settings.openSettings(),
    "menu:toggle-tree": () => togglePanel("tree"),
    "menu:toggle-flat": () => togglePanel("flat"),
    "menu:toggle-inspector": () => togglePanel("inspector"),
    "menu:palette": () => window.dispatchEvent(new CustomEvent("progest:toggle-palette")),
    "menu:rescan": () => void doRescan(),
    "menu:import": () => void pickAndImport(),
    "menu:undo": () => void doUndo(),
    "menu:redo": () => void doRedo(),
  });

  const pickAndImport = React.useCallback(async () => {
    const picked = await openFileDialog({ multiple: true });
    if (!picked || (Array.isArray(picked) && picked.length === 0)) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    setImportSources(paths);
    setImportDest(undefined);
    setImportOpen(true);
  }, []);

  // --- import via drag & drop -----------------------------------------------
  const [importSources, setImportSources] = React.useState<string[]>([]);
  const [importDest, setImportDest] = React.useState<string | undefined>();
  const [importOpen, setImportOpen] = React.useState(false);

  const treeRef = React.useRef<HTMLElement>(null);

  const handleDrop = React.useCallback(
    (paths: string[], position: { x: number; y: number }) => {
      if (!project || paths.length === 0 || isDragOutActive()) return;

      // Check if the drop landed on a TreeView folder button by
      // inspecting the DOM at the drop point.
      const dirPath = dirPathAtPoint(position);
      const dest = dirPath != null ? dirPath : undefined;

      setImportSources(paths);
      setImportDest(dest);
      setImportOpen(true);
    },
    [project],
  );

  const isMac = navigator.platform.includes("Mac");
  const showShadcnMenu = !isMac || localStorage.getItem("progest:show-menubar") === "true";

  const menuActions = {
    onNewProject: () => openInitDialog("new"),
    onOpenProject: () => void openPicker(),
    onNewFile: () =>
      window.dispatchEvent(new CustomEvent("progest:create", { detail: { kind: "file" } })),
    onNewFolder: () =>
      window.dispatchEvent(new CustomEvent("progest:create", { detail: { kind: "dir" } })),
    onImport: () => void pickAndImport(),
    onSettings: () => settings.openSettings(),
    onUndo: () => void doUndo(),
    onRedo: () => void doRedo(),
    onToggleTree: () => togglePanel("tree"),
    onToggleFlat: () => togglePanel("flat"),
    onToggleInspector: () => togglePanel("inspector"),
    onPalette: () => window.dispatchEvent(new CustomEvent("progest:toggle-palette")),
    onRescan: () => void doRescan(),
  };

  return (
    <DragDropProvider onDrop={handleDrop}>
      <div className="grid h-screen grid-rows-[auto_1fr_auto] bg-background">
        <div>
          <TitleBar panels={panels} onTogglePanel={togglePanel} />
          {showShadcnMenu ? <AppMenubar {...menuActions} /> : null}
        </div>
        {project ? (
          <MainShell
            onPickHit={onPickFlatHit}
            onPickTreeFile={onPickTreeFile}
            selection={selection}
            selectedDir={selectedDir}
            onSelectDir={onSelectDir}
            panels={panels}
            treeRef={treeRef}
            onFileDeleted={onFileDeleted}
            onSelectionUpdate={onSelectionUpdate}
            inspectorRef={inspectorRef}
          />
        ) : (
          <Welcome />
        )}
        <StatusBar />
      </div>
      <CommandPalette onPickHit={onPickFlatHit} />
      <InitProjectDialog />
      <SettingsDialog
        open={settings.open}
        onOpenChange={(v) => {
          if (!v) settings.closeSettings();
        }}
        initialTab={settings.tab}
      />
      <PendingSuggestionsDialog
        open={pendingConfirm !== null}
        onDiscard={() => {
          if (pendingConfirm) setSelection(pendingConfirm.next);
          setPendingConfirm(null);
        }}
        onCancel={() => setPendingConfirm(null)}
      />
      <ImportModal
        open={importOpen}
        onOpenChange={setImportOpen}
        sources={importSources}
        initialDest={importDest}
      />
    </DragDropProvider>
  );
}

function MainShell(props: {
  onPickHit: (hit: RichSearchHit) => void;
  onPickTreeFile: (entry: DirEntry) => void;
  selection: Selection;
  selectedDir: string;
  onSelectDir: (path: string) => void;
  panels: PanelVisibility;
  treeRef: React.RefObject<HTMLElement | null>;
  onFileDeleted: () => void;
  onSelectionUpdate: (hit: RichSearchHit) => void;
  inspectorRef: React.RefObject<FileInspectorHandle | null>;
}) {
  const flatRef = React.useRef<HTMLElement>(null);
  const flatDrop = useDropZone(flatRef);

  const panes: { key: PanelKey; node: React.ReactNode }[] = [];
  if (props.panels.tree) {
    panes.push({
      key: "tree",
      node: (
        <ResizablePanel id="tree" key="tree" defaultSize={22} minSize={12}>
          <SidePanel
            treeRef={props.treeRef}
            onPickTreeFile={props.onPickTreeFile}
            selectedDir={props.selectedDir}
            onSelectDir={props.onSelectDir}
            onPickHit={props.onPickHit}
          />
        </ResizablePanel>
      ),
    });
  }
  if (props.panels.flat) {
    panes.push({
      key: "flat",
      node: (
        <ResizablePanel id="flat" key="flat" defaultSize={40} minSize={20}>
          <main ref={flatRef} className="relative h-full overflow-hidden">
            <FlatView onPickHit={props.onPickHit} />
            <DropOverlay
              isOver={flatDrop.isOver}
              fileCount={flatDrop.fileCount}
              label="Auto-suggest destination"
            />
          </main>
        </ResizablePanel>
      ),
    });
  }
  if (props.panels.inspector) {
    panes.push({
      key: "inspector",
      node: (
        <ResizablePanel id="inspector" key="inspector" defaultSize={38} minSize={20}>
          <aside className="h-full overflow-hidden">
            <InspectorPane
              selection={props.selection}
              onFileDeleted={props.onFileDeleted}
              onSelectionUpdate={props.onSelectionUpdate}
              inspectorRef={props.inspectorRef}
            />
          </aside>
        </ResizablePanel>
      ),
    });
  }

  return (
    <div className="overflow-hidden">
      <ResizablePanelGroup orientation="horizontal" id="progest:main-shell" className="h-full">
        {panes.map((p, i) => (
          <React.Fragment key={p.key}>
            {i > 0 ? <ResizableHandle withHandle /> : null}
            {p.node}
          </React.Fragment>
        ))}
      </ResizablePanelGroup>
    </div>
  );
}

/**
 * Route the inspector pane between directory-mode and file-mode based
 * on the current selection. Empty selection falls back to the
 * directory inspector at project root, matching the previous default.
 */
function InspectorPane(props: {
  selection: Selection;
  onFileDeleted?: (() => void) | undefined;
  onSelectionUpdate?: (hit: RichSearchHit) => void;
  inspectorRef: React.RefObject<FileInspectorHandle | null>;
}) {
  if (props.selection?.kind === "file") {
    return (
      <FileInspector
        ref={props.inspectorRef}
        hit={props.selection.hit}
        onDeleted={props.onFileDeleted}
        onSelectionUpdate={props.onSelectionUpdate}
      />
    );
  }
  const dir = props.selection?.kind === "dir" ? props.selection.path : "";
  return <DirectoryInspector dir={dir} />;
}

function Welcome() {
  const { recent, openPicker, pickRecent, openInitDialog, error } = useProject();
  return (
    <div className="flex h-full flex-col items-center justify-center gap-6 overflow-auto p-6">
      <div className="text-center">
        <h1 className="text-2xl font-semibold tracking-tight">Progest</h1>
        <p className="text-xs text-muted-foreground">
          Open a project (a folder containing <code>.progest/</code>) or create a new one.
        </p>
      </div>
      <div className="flex flex-wrap items-center justify-center gap-2">
        <Button onClick={() => void openPicker()}>
          <FolderOpen /> Open project…
        </Button>
        <Button variant="outline" onClick={() => openInitDialog("new")}>
          <Sparkles /> New project…
        </Button>
        <Button variant="outline" onClick={() => openInitDialog("existing")}>
          <FolderPlus /> Initialize folder…
        </Button>
      </div>
      {recent.length > 0 ? (
        <div className="grid w-full max-w-md gap-1 text-xs">
          <div className="text-muted-foreground">Recent</div>
          <ul className="grid gap-1">
            {recent.slice(0, 8).map((entry) => (
              <li key={entry.root}>
                <Button
                  variant="outline"
                  onClick={() => void pickRecent(entry)}
                  className="grid h-auto w-full grid-cols-[1fr_auto] items-center gap-2 px-2 py-1.5 text-left font-normal"
                >
                  <div className="min-w-0">
                    <div className="truncate">{entry.name || entry.root}</div>
                    <div className="truncate text-[0.625rem] text-muted-foreground">
                      {entry.root}
                    </div>
                  </div>
                  <span className="text-[0.625rem] text-muted-foreground">
                    {relTime(entry.last_opened)}
                  </span>
                </Button>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
      {error ? <div className="text-xs text-destructive">{error}</div> : null}
    </div>
  );
}

function SidePanel(props: {
  treeRef: React.RefObject<HTMLElement | null>;
  onPickTreeFile: (entry: DirEntry) => void;
  selectedDir: string;
  onSelectDir: (path: string) => void;
  onPickHit: (hit: RichSearchHit) => void;
}) {
  const [tab, setTab] = React.useState<"tree" | "sequences">("tree");
  return (
    <aside ref={props.treeRef} className="grid h-full grid-rows-[auto_1fr] overflow-hidden">
      <div className="flex border-b">
        <button
          type="button"
          className={cn(
            "flex flex-1 items-center justify-center gap-1 px-2 py-1 text-[0.625rem] uppercase tracking-wide",
            tab === "tree"
              ? "border-b-2 border-primary font-medium"
              : "text-muted-foreground hover:text-foreground",
          )}
          onClick={() => setTab("tree")}
        >
          <FolderTree className="size-3" />
          Tree
        </button>
        <button
          type="button"
          className={cn(
            "flex flex-1 items-center justify-center gap-1 px-2 py-1 text-[0.625rem] uppercase tracking-wide",
            tab === "sequences"
              ? "border-b-2 border-primary font-medium"
              : "text-muted-foreground hover:text-foreground",
          )}
          onClick={() => setTab("sequences")}
        >
          <Layers className="size-3" />
          Sequences
        </button>
      </div>
      {tab === "tree" ? (
        <TreeView
          onPickFile={props.onPickTreeFile}
          selectedDir={props.selectedDir}
          onSelectDir={props.onSelectDir}
        />
      ) : (
        <SequenceView onPickHit={props.onPickHit} />
      )}
    </aside>
  );
}

/**
 * TreeView yields `DirEntry` for both directories and files; we only
 * route file rows into the selection slot, and reshape them into the
 * `RichSearchHit` envelope the inspector expects. Returns `null` for
 * directory entries or files that haven't been hydrated by the index
 * yet (the tree shows on-disk truth, the index lags behind reconcile).
 */
function treeEntryToHit(entry: DirEntry): RichSearchHit | null {
  if (entry.kind !== "file" || !entry.file) return null;
  return {
    file_id: entry.file.file_id ?? "",
    path: entry.path,
    name: entry.name,
    kind: entry.file.kind,
    ext: entry.file.ext,
    tags: entry.file.tags,
    violations: entry.file.violations,
    custom_fields: entry.file.custom_fields,
  };
}

function relTime(rfc3339: string): string {
  const t = Date.parse(rfc3339);
  if (Number.isNaN(t)) return "";
  const diff = Date.now() - t;
  const sec = Math.max(0, Math.floor(diff / 1000));
  if (sec < 60) return `${sec}s ago`;
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const day = Math.floor(hr / 24);
  return `${day}d ago`;
}

function PendingSuggestionsDialog(props: {
  open: boolean;
  onDiscard: () => void;
  onCancel: () => void;
}) {
  return (
    <Dialog open={props.open} onOpenChange={(v) => !v && props.onCancel()}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>Discard AI suggestions?</DialogTitle>
          <DialogDescription>
            You have unapplied AI suggestions. Switching files will discard them.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={props.onCancel}>
            Stay
          </Button>
          <Button variant="destructive" onClick={props.onDiscard}>
            Discard
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
