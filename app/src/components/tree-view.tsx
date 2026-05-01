import * as React from "react";
import { ChevronRight, ChevronDown, Folder, FolderOpen, FileIcon } from "lucide-react";

import { useDragActive } from "@/components/drag-drop-overlay";
import { FileContextMenu } from "@/components/file-context-menu";
import { filesListDir, IpcError, type DirEntry, type FileEntry } from "@/lib/ipc";
import { useProject } from "@/lib/project-context";

import { ViolationDots } from "@/components/violation-badges";
import { cn } from "@/lib/utils";

type LoadState = "idle" | "loading" | "loaded" | "error";

type DirState = {
  state: LoadState;
  children: DirEntry[];
  error?: string;
};

/**
 * Given a CSS-pixel position, find the closest `[data-dir-path]`
 * ancestor of the element under that point.  Returns the path string
 * or `null` if the cursor is not over any DirNode.
 */
export function dirPathAtPoint(pos: { x: number; y: number }): string | null {
  const el = document.elementFromPoint(pos.x, pos.y);
  const btn = el?.closest("[data-dir-path]");
  if (!btn) return null;
  return btn.getAttribute("data-dir-path");
}

export function TreeView(props: {
  onPickFile?: (entry: DirEntry) => void;
  selectedDir?: string;
  onSelectDir?: (path: string) => void;
}) {
  const { project, refreshTick } = useProject();
  const [cache, setCache] = React.useState<Record<string, DirState>>({});
  const [expanded, setExpanded] = React.useState<Set<string>>(() => new Set([""]));

  const fetchDir = React.useCallback(async (path: string) => {
    setCache((c) => ({
      ...c,
      [path]: { state: "loading", children: c[path]?.children ?? [] },
    }));
    try {
      const list = await filesListDir(path);
      setCache((c) => ({ ...c, [path]: { state: "loaded", children: list } }));
    } catch (e) {
      const msg = e instanceof IpcError ? e.raw : String(e);
      setCache((c) => ({
        ...c,
        [path]: { state: "error", children: [], error: msg },
      }));
    }
  }, []);

  React.useEffect(() => {
    setCache({});
    setExpanded(new Set([""]));
    void fetchDir("");
  }, [project?.root, fetchDir]);

  const expandedSnapshot = React.useRef(expanded);
  expandedSnapshot.current = expanded;
  React.useEffect(() => {
    if (refreshTick === 0) return;
    setCache({});
    for (const path of expandedSnapshot.current) {
      void fetchDir(path);
    }
  }, [refreshTick, fetchDir]);

  const toggle = React.useCallback(
    async (path: string) => {
      const next = new Set(expanded);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
        if (!cache[path] || cache[path].state === "error") {
          await fetchDir(path);
        }
      }
      setExpanded(next);
    },
    [expanded, cache, fetchDir],
  );

  const dragState = useDragActive();
  const dragOverPath = React.useMemo(() => {
    if (!dragState.active || !dragState.position) return null;
    return dirPathAtPoint(dragState.position);
  }, [dragState.active, dragState.position]);

  return (
    <nav className="h-full overflow-auto p-1 text-xs">
      <DirNode
        path=""
        name="(root)"
        depth={0}
        expanded={expanded}
        cache={cache}
        toggle={toggle}
        onPickFile={props.onPickFile}
        selectedDir={props.selectedDir}
        onSelectDir={props.onSelectDir}
        dragOverPath={dragOverPath}
      />
    </nav>
  );
}

function DirNode(props: {
  path: string;
  name: string;
  depth: number;
  expanded: Set<string>;
  cache: Record<string, DirState>;
  toggle: (path: string) => Promise<void>;
  onPickFile: ((entry: DirEntry) => void) | undefined;
  selectedDir: string | undefined;
  onSelectDir: ((path: string) => void) | undefined;
  dragOverPath: string | null;
}) {
  const {
    path,
    name,
    depth,
    expanded,
    cache,
    toggle,
    onPickFile,
    selectedDir,
    onSelectDir,
    dragOverPath,
  } = props;
  const isOpen = expanded.has(path);
  const isSelected = selectedDir === path;
  const entry = cache[path];
  const indent = depth * 12;

  const isDragOver = dragOverPath === path;

  return (
    <div>
      <button
        type="button"
        data-dir-path={path}
        className={cn(
          "flex w-full items-center gap-1 rounded px-1 py-0.5 hover:bg-accent",
          isSelected && "bg-accent text-accent-foreground",
          isDragOver && "bg-primary/20 ring-1 ring-primary/50",
        )}
        style={{ paddingLeft: indent + 4 }}
        onClick={() => {
          onSelectDir?.(path);
          void toggle(path);
        }}
      >
        {isOpen ? (
          <ChevronDown className="size-3 opacity-60" />
        ) : (
          <ChevronRight className="size-3 opacity-60" />
        )}
        {isOpen ? (
          <FolderOpen className="size-3.5 opacity-70" />
        ) : (
          <Folder className="size-3.5 opacity-70" />
        )}
        <span className="truncate">{name}</span>
      </button>
      {isOpen ? (
        <div>
          {entry?.state === "loading" && (
            <div className="px-1 py-0.5 text-muted-foreground" style={{ paddingLeft: indent + 24 }}>
              loading…
            </div>
          )}
          {entry?.state === "error" && (
            <div
              className="px-1 py-0.5 text-destructive"
              style={{ paddingLeft: indent + 24 }}
              title={entry.error}
            >
              load failed
            </div>
          )}
          {entry?.state === "loaded" &&
            entry.children.map((child) =>
              child.kind === "dir" ? (
                <DirNode
                  key={child.path}
                  path={child.path}
                  name={child.name}
                  depth={depth + 1}
                  expanded={expanded}
                  cache={cache}
                  toggle={toggle}
                  onPickFile={onPickFile}
                  selectedDir={selectedDir}
                  onSelectDir={onSelectDir}
                  dragOverPath={dragOverPath}
                />
              ) : (
                <FileNode key={child.path} entry={child} depth={depth + 1} onPick={onPickFile} />
              ),
            )}
        </div>
      ) : null}
    </div>
  );
}

function FileNode(props: {
  entry: DirEntry;
  depth: number;
  onPick: ((entry: DirEntry) => void) | undefined;
}) {
  const { entry, depth, onPick } = props;
  const { bumpRefresh } = useProject();
  const file: FileEntry | undefined = entry.file;
  const indent = depth * 12;
  return (
    <FileContextMenu path={entry.path} onDeleted={bumpRefresh}>
      <button
        type="button"
        className="flex w-full items-center gap-1 rounded px-1 py-0.5 text-left hover:bg-accent"
        style={{ paddingLeft: indent + 16 }}
        onClick={() => onPick?.(entry)}
      >
        <FileIcon className="size-3.5 opacity-60" />
        <span className="truncate">{entry.name}</span>
        {file ? <ViolationDots counts={file.violations} /> : null}
        {file && file.tags.length > 0 ? (
          <span className="ml-auto text-[0.625rem] text-muted-foreground">
            {file.tags.map((t) => `#${t}`).join(" ")}
          </span>
        ) : null}
      </button>
    </FileContextMenu>
  );
}
