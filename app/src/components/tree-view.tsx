import * as React from "react";
import {
  ChevronRight,
  ChevronDown,
  ClipboardCopy,
  ExternalLink,
  Folder,
  FolderOpen,
  FolderSearch,
  FileIcon,
  FilePlus,
  FolderPlus,
  Pencil,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";

import { useDragActive } from "@/components/drag-drop-overlay";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
  filesListDir,
  fsCreateDir,
  fsCreateFile,
  fsAbsPath,
  fsOpen,
  fsRename,
  fsReveal,
  fileDeleteApply,
  dirDeleteApply,
  IpcError,
  type DirEntry,
  type FileEntry,
} from "@/lib/ipc";
import { useProject } from "@/lib/project-context";
import { ViolationDots } from "@/components/violation-badges";
import { cn } from "@/lib/utils";

type LoadState = "idle" | "loading" | "loaded" | "error";

type DirState = {
  state: LoadState;
  children: DirEntry[];
  error?: string;
};

type EditState =
  | { mode: "rename"; path: string; name: string }
  | { mode: "create"; parentDir: string; kind: "file" | "dir" }
  | null;

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
  const { project, refreshTick, bumpRefresh } = useProject();
  const [cache, setCache] = React.useState<Record<string, DirState>>({});
  const [expanded, setExpanded] = React.useState<Set<string>>(() => new Set([""]));
  const [edit, setEdit] = React.useState<EditState>(null);

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
    setEdit(null);
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

  const startCreate = React.useCallback(
    (parentDir: string, kind: "file" | "dir") => {
      setExpanded((prev) => new Set([...prev, parentDir]));
      if (!cache[parentDir] || cache[parentDir].state !== "loaded") {
        void fetchDir(parentDir);
      }
      setEdit({ mode: "create", parentDir, kind });
    },
    [cache, fetchDir],
  );

  const startRename = React.useCallback((path: string, name: string) => {
    setEdit({ mode: "rename", path, name });
  }, []);

  const cancelEdit = React.useCallback(() => setEdit(null), []);

  const commitCreate = React.useCallback(
    async (name: string) => {
      if (!edit || edit.mode !== "create") return;
      const fullPath = edit.parentDir ? `${edit.parentDir}/${name}` : name;
      try {
        if (edit.kind === "dir") {
          await fsCreateDir(fullPath);
          toast.success(`Created folder: ${name}`);
        } else {
          await fsCreateFile(fullPath);
          toast.success(`Created file: ${name}`);
        }
        bumpRefresh();
      } catch (e) {
        toast.error(String(e));
      }
      setEdit(null);
    },
    [edit, bumpRefresh],
  );

  const commitRename = React.useCallback(
    async (newName: string) => {
      if (!edit || edit.mode !== "rename") return;
      if (newName === edit.name || !newName.trim()) {
        setEdit(null);
        return;
      }
      try {
        const result = await fsRename(edit.path, newName);
        window.dispatchEvent(
          new CustomEvent("progest:renamed", { detail: { from: edit.path, to: result.to } }),
        );
        toast.success(`Renamed to ${newName}`);
        bumpRefresh();
      } catch (e) {
        toast.error(String(e));
      }
      setEdit(null);
    },
    [edit, bumpRefresh],
  );

  const dragState = useDragActive();
  const dragOverPath = React.useMemo(() => {
    if (!dragState.active || !dragState.position) return null;
    return dirPathAtPoint(dragState.position);
  }, [dragState.active, dragState.position]);

  // Listen for palette "New file / New folder" commands
  React.useEffect(() => {
    function onCreateEvent(e: Event) {
      const detail = (e as CustomEvent<{ kind: "file" | "dir" }>).detail;
      const dir = props.selectedDir ?? "";
      startCreate(dir, detail.kind);
    }
    window.addEventListener("progest:create", onCreateEvent);
    return () => window.removeEventListener("progest:create", onCreateEvent);
  }, [props.selectedDir, startCreate]);

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
        edit={edit}
        onStartCreate={startCreate}
        onStartRename={startRename}
        onCancelEdit={cancelEdit}
        onCommitCreate={commitCreate}
        onCommitRename={commitRename}
      />
    </nav>
  );
}

// ── Inline input ────────────────────────────────────────────────────

function InlineInput(props: {
  defaultValue: string;
  onCommit: (value: string) => void;
  onCancel: () => void;
  selectStem?: boolean;
}) {
  const ref = React.useRef<HTMLInputElement>(null);

  React.useEffect(() => {
    const input = ref.current;
    if (!input) return;
    input.focus();
    if (props.selectStem) {
      const dot = props.defaultValue.lastIndexOf(".");
      input.setSelectionRange(0, dot > 0 ? dot : props.defaultValue.length);
    } else {
      input.select();
    }
  }, [props.defaultValue, props.selectStem]);

  return (
    <input
      ref={ref}
      type="text"
      defaultValue={props.defaultValue}
      className="h-5 w-full min-w-0 rounded border bg-background px-1 text-xs outline-none focus:ring-1 focus:ring-ring"
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          props.onCommit((e.target as HTMLInputElement).value.trim());
        } else if (e.key === "Escape") {
          e.preventDefault();
          props.onCancel();
        }
        e.stopPropagation();
      }}
      onBlur={(e) => {
        const value = e.target.value.trim();
        if (value && value !== props.defaultValue) {
          props.onCommit(value);
        } else {
          props.onCancel();
        }
      }}
      onClick={(e) => e.stopPropagation()}
    />
  );
}

// ── DirNode ─────────────────────────────────────────────────────────

type TreeEditProps = {
  edit: EditState;
  onStartCreate: (parentDir: string, kind: "file" | "dir") => void;
  onStartRename: (path: string, name: string) => void;
  onCancelEdit: () => void;
  onCommitCreate: (name: string) => void;
  onCommitRename: (newName: string) => void;
};

function DirNode(
  props: {
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
  } & TreeEditProps,
) {
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
    edit,
    onStartCreate,
    onStartRename,
    onCancelEdit,
    onCommitCreate,
    onCommitRename,
  } = props;
  const { bumpRefresh } = useProject();
  const isOpen = expanded.has(path);
  const isSelected = selectedDir === path;
  const entry = cache[path];
  const indent = depth * 12;
  const isDragOver = dragOverPath === path;
  const isRenaming = edit?.mode === "rename" && edit.path === path;
  const isCreatingHere = edit?.mode === "create" && edit.parentDir === path;

  const handleDelete = React.useCallback(async () => {
    if (!path) return;
    try {
      await dirDeleteApply(path);
      toast.success("Moved to Trash");
      bumpRefresh();
    } catch (e) {
      toast.error(String(e));
    }
  }, [path, bumpRefresh]);

  return (
    <div>
      <ContextMenu>
        <ContextMenuTrigger asChild>
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
              <ChevronDown className="size-3 shrink-0 opacity-60" />
            ) : (
              <ChevronRight className="size-3 shrink-0 opacity-60" />
            )}
            {isOpen ? (
              <FolderOpen className="size-3.5 shrink-0 opacity-70" />
            ) : (
              <Folder className="size-3.5 shrink-0 opacity-70" />
            )}
            {isRenaming ? (
              <InlineInput defaultValue={name} onCommit={onCommitRename} onCancel={onCancelEdit} />
            ) : (
              <span className="truncate">{name}</span>
            )}
          </button>
        </ContextMenuTrigger>
        <ContextMenuContent>
          <div className="truncate max-w-48 px-2 py-1 text-[0.625rem] font-medium text-muted-foreground">
            {name}
          </div>
          <ContextMenuSeparator />
          <ContextMenuItem onClick={() => onStartCreate(path, "file")}>
            <FilePlus className="mr-2 size-3.5" />
            New File
          </ContextMenuItem>
          <ContextMenuItem onClick={() => onStartCreate(path, "dir")}>
            <FolderPlus className="mr-2 size-3.5" />
            New Folder
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem onClick={() => void fsReveal(path || ".")}>
            <FolderSearch className="mr-2 size-3.5" />
            Reveal in Finder
          </ContextMenuItem>
          <ContextMenuItem
            onClick={async () => {
              const abs = await fsAbsPath(path || ".");
              await navigator.clipboard.writeText(abs);
              toast.success("Path copied");
            }}
          >
            <ClipboardCopy className="mr-2 size-3.5" />
            Copy Path
          </ContextMenuItem>
          {path !== "" && (
            <>
              <ContextMenuSeparator />
              <ContextMenuItem onClick={() => onStartRename(path, name)}>
                <Pencil className="mr-2 size-3.5" />
                Rename
              </ContextMenuItem>
              <ContextMenuItem
                className="text-destructive focus:text-destructive"
                onClick={() => void handleDelete()}
              >
                <Trash2 className="mr-2 size-3.5" />
                Move to Trash
              </ContextMenuItem>
            </>
          )}
        </ContextMenuContent>
      </ContextMenu>
      {isOpen ? (
        <div>
          {isCreatingHere && edit.mode === "create" && (
            <div
              className="flex items-center gap-1 rounded px-1 py-0.5"
              style={{ paddingLeft: (depth + 1) * 12 + (edit.kind === "dir" ? 4 : 16) }}
            >
              {edit.kind === "dir" ? (
                <Folder className="size-3.5 shrink-0 opacity-70" />
              ) : (
                <FileIcon className="size-3.5 shrink-0 opacity-60" />
              )}
              <InlineInput defaultValue="" onCommit={onCommitCreate} onCancel={onCancelEdit} />
            </div>
          )}
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
                  edit={edit}
                  onStartCreate={onStartCreate}
                  onStartRename={onStartRename}
                  onCancelEdit={onCancelEdit}
                  onCommitCreate={onCommitCreate}
                  onCommitRename={onCommitRename}
                />
              ) : (
                <FileNode
                  key={child.path}
                  entry={child}
                  depth={depth + 1}
                  onPick={onPickFile}
                  edit={edit}
                  onStartRename={onStartRename}
                  onCancelEdit={onCancelEdit}
                  onCommitRename={onCommitRename}
                />
              ),
            )}
        </div>
      ) : null}
    </div>
  );
}

// ── FileNode ────────────────────────────────────────────────────────

function FileNode(props: {
  entry: DirEntry;
  depth: number;
  onPick: ((entry: DirEntry) => void) | undefined;
  edit: EditState;
  onStartRename: (path: string, name: string) => void;
  onCancelEdit: () => void;
  onCommitRename: (newName: string) => void;
}) {
  const { entry, depth, onPick, edit, onStartRename, onCancelEdit, onCommitRename } = props;
  const { bumpRefresh } = useProject();
  const file: FileEntry | undefined = entry.file;
  const indent = depth * 12;
  const isRenaming = edit?.mode === "rename" && edit.path === entry.path;

  const handleDelete = React.useCallback(async () => {
    try {
      await fileDeleteApply(entry.path);
      toast.success("Moved to Trash", { description: entry.name });
      bumpRefresh();
    } catch (e) {
      toast.error(String(e));
    }
  }, [entry.path, entry.name, bumpRefresh]);

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <button
          type="button"
          className="flex w-full items-center gap-1 rounded px-1 py-0.5 text-left hover:bg-accent"
          style={{ paddingLeft: indent + 16 }}
          onClick={() => onPick?.(entry)}
          onDoubleClick={() => void fsOpen(entry.path)}
        >
          <FileIcon className="size-3.5 shrink-0 opacity-60" />
          {isRenaming ? (
            <InlineInput
              defaultValue={entry.name}
              onCommit={onCommitRename}
              onCancel={onCancelEdit}
              selectStem
            />
          ) : (
            <span className="truncate">{entry.name}</span>
          )}
          {!isRenaming && file ? <ViolationDots counts={file.violations} /> : null}
          {!isRenaming && file && file.tags.length > 0 ? (
            <span className="ml-auto text-[0.625rem] text-muted-foreground">
              {file.tags.map((t) => `#${t}`).join(" ")}
            </span>
          ) : null}
        </button>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <div className="truncate max-w-48 px-2 py-1 text-[0.625rem] font-medium text-muted-foreground">
          {entry.name}
        </div>
        <ContextMenuSeparator />
        <ContextMenuItem onClick={() => void fsOpen(entry.path)}>
          <ExternalLink className="mr-2 size-3.5" />
          Open
        </ContextMenuItem>
        <ContextMenuItem onClick={() => void fsReveal(entry.path)}>
          <FolderSearch className="mr-2 size-3.5" />
          Reveal in Finder
        </ContextMenuItem>
        <ContextMenuItem
          onClick={async () => {
            const abs = await fsAbsPath(entry.path);
            await navigator.clipboard.writeText(abs);
            toast.success("Path copied");
          }}
        >
          <ClipboardCopy className="mr-2 size-3.5" />
          Copy Path
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onClick={() => onStartRename(entry.path, entry.name)}>
          <Pencil className="mr-2 size-3.5" />
          Rename
        </ContextMenuItem>
        <ContextMenuItem
          className="text-destructive focus:text-destructive"
          onClick={() => void handleDelete()}
        >
          <Trash2 className="mr-2 size-3.5" />
          Move to Trash
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
