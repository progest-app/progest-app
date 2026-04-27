import * as React from "react";
import { FolderOpen } from "lucide-react";

import { CommandPalette } from "@/components/command-palette";
import { TreeView } from "@/components/tree-view";
import { FlatView } from "@/components/flat-view";
import { ResultDetailDialog } from "@/components/result-detail-dialog";
import { StatusBar } from "@/components/status-bar";
import { ThemeToggle } from "@/components/theme-toggle";
import { Button } from "@/components/ui/button";
import { TooltipProvider } from "@/components/ui/tooltip";
import { FlatViewSummaryProvider } from "@/lib/flat-view-context";
import { ProjectProvider, useProject } from "@/lib/project-context";
import { ThemeProvider } from "next-themes";
import type { DirEntry, RichSearchHit } from "@/lib/ipc";

import "./App.css";

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
          <FlatViewSummaryProvider>
            <Shell />
          </FlatViewSummaryProvider>
        </ProjectProvider>
      </TooltipProvider>
    </ThemeProvider>
  );
}

function Shell() {
  const { project } = useProject();
  // Currently-selected file from the tree (DirEntry) or flat view
  // (RichSearchHit). Both feed a shared detail dialog so the user can
  // inspect a file without losing their place in the tree.
  const [hitDetail, setHitDetail] = React.useState<RichSearchHit | null>(null);
  const [treeDetail, setTreeDetail] = React.useState<DirEntry | null>(null);

  return (
    <>
      {project ? (
        <MainShell
          onPickHit={(h) => setHitDetail(h)}
          onPickTreeFile={(e) => setTreeDetail(e)}
        />
      ) : (
        <Welcome />
      )}
      {/*
        CommandPalette is mounted globally so Cmd+K works even from the
        Welcome screen. Its hit handler routes through the same detail
        dialog as the FlatView selection.
      */}
      <CommandPalette onPickHit={(h) => setHitDetail(h)} />
      <ResultDetailDialog
        hit={hitDetail}
        open={hitDetail !== null}
        onOpenChange={(o) => {
          if (!o) setHitDetail(null);
        }}
      />
      <TreeFileDetail
        entry={treeDetail}
        onOpenChange={(o) => {
          if (!o) setTreeDetail(null);
        }}
      />
    </>
  );
}

function MainShell(props: {
  onPickHit: (hit: RichSearchHit) => void;
  onPickTreeFile: (entry: DirEntry) => void;
}) {
  return (
    <div className="grid h-screen grid-rows-[auto_1fr_auto] bg-background">
      <TopBar />
      <div className="grid grid-cols-[260px_1fr] overflow-hidden border-t">
        <aside className="overflow-hidden border-r">
          <TreeView onPickFile={props.onPickTreeFile} />
        </aside>
        <main className="overflow-hidden">
          <FlatView onPickHit={props.onPickHit} />
        </main>
      </div>
      <StatusBar />
    </div>
  );
}

function TopBar() {
  const { project, openPicker } = useProject();
  return (
    <header className="flex items-center gap-3 px-3 py-2">
      <h1 className="text-sm font-semibold tracking-tight">Progest</h1>
      <Button
        variant="outline"
        size="sm"
        onClick={() => void openPicker()}
        title={project ? `Open another project (current: ${project.name})` : "Open project"}
      >
        <FolderOpen />
        {project ? project.name : "Open project…"}
      </Button>
      <span className="ml-auto text-xs text-muted-foreground">⌘K to search</span>
      <ThemeToggle />
    </header>
  );
}

function Welcome() {
  const { recent, openPicker, pickRecent, error } = useProject();
  return (
    <div className="grid h-screen grid-rows-[1fr_auto] bg-background">
      <div className="relative flex flex-col items-center justify-center gap-6 p-6">
        <div className="absolute top-3 right-3">
          <ThemeToggle />
        </div>
        <div className="text-center">
          <h1 className="text-2xl font-semibold tracking-tight">Progest</h1>
          <p className="text-xs text-muted-foreground">
            Open a project (a folder containing <code>.progest/</code>).
          </p>
        </div>
        <Button onClick={() => void openPicker()}>
          <FolderOpen /> Open project…
        </Button>
        {recent.length > 0 ? (
          <div className="grid w-full max-w-md gap-1 text-xs">
            <div className="text-muted-foreground">Recent</div>
            <ul className="grid gap-1">
              {recent.slice(0, 8).map((entry) => (
                <li key={entry.root}>
                  <button
                    type="button"
                    className="grid w-full grid-cols-[1fr_auto] items-center gap-2 rounded-md border px-2 py-1.5 text-left hover:bg-accent"
                    onClick={() => void pickRecent(entry)}
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
                  </button>
                </li>
              ))}
            </ul>
          </div>
        ) : null}
        {error ? <div className="text-xs text-destructive">{error}</div> : null}
      </div>
      <StatusBar />
    </div>
  );
}

function TreeFileDetail(props: {
  entry: DirEntry | null;
  onOpenChange: (open: boolean) => void;
}) {
  // The tree node carries the same FileEntry shape the flat view does,
  // but without the file_id-keyed `path` field. We synthesize a
  // RichSearchHit-shaped payload so ResultDetailDialog can render it.
  const hit = React.useMemo<RichSearchHit | null>(() => {
    const entry = props.entry;
    if (!entry || entry.kind !== "file" || !entry.file) return null;
    return {
      file_id: entry.file.file_id ?? "(unindexed)",
      path: entry.path,
      name: entry.name,
      kind: entry.file.kind,
      ext: entry.file.ext,
      tags: entry.file.tags,
      violations: entry.file.violations,
      custom_fields: entry.file.custom_fields,
    };
  }, [props.entry]);
  return (
    <ResultDetailDialog
      hit={hit}
      open={hit !== null}
      onOpenChange={props.onOpenChange}
    />
  );
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
