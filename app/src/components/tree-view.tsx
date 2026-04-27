import * as React from "react";
import { ChevronRight, ChevronDown, Folder, FolderOpen, FileIcon } from "lucide-react";

import { filesListDir, IpcError, type DirEntry, type FileEntry } from "@/lib/ipc";
import { useProject } from "@/lib/project-context";

import { ViolationDots } from "@/components/violation-badges";

type LoadState = "idle" | "loading" | "loaded" | "error";

type DirState = {
  state: LoadState;
  children: DirEntry[];
  error?: string;
};

export function TreeView(props: { onPickFile?: (entry: DirEntry) => void }) {
  const { project } = useProject();
  // path "" = root; cache keeps loaded children + error per path so
  // collapsing/re-expanding doesn't re-fetch.
  const [cache, setCache] = React.useState<Record<string, DirState>>({});
  const [expanded, setExpanded] = React.useState<Set<string>>(() => new Set([""]));

  const fetchDir = React.useCallback(
    async (path: string) => {
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
    },
    [],
  );

  // Reset cache + expanded set when the attached project changes,
  // then refetch the new root. Without this, the tree would keep
  // showing the old project's directory snapshot until the user
  // collapsed and re-expanded each branch.
  React.useEffect(() => {
    setCache({});
    setExpanded(new Set([""]));
    void fetchDir("");
  }, [project?.root, fetchDir]);

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
}) {
  const { path, name, depth, expanded, cache, toggle, onPickFile } = props;
  const isOpen = expanded.has(path);
  const entry = cache[path];
  const indent = depth * 12;
  return (
    <div>
      <button
        type="button"
        className="flex w-full items-center gap-1 rounded px-1 py-0.5 hover:bg-accent"
        style={{ paddingLeft: indent + 4 }}
        onClick={() => void toggle(path)}
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
            <div
              className="px-1 py-0.5 text-muted-foreground"
              style={{ paddingLeft: indent + 24 }}
            >
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
                />
              ) : (
                <FileNode
                  key={child.path}
                  entry={child}
                  depth={depth + 1}
                  onPick={onPickFile}
                />
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
  const file: FileEntry | undefined = entry.file;
  const indent = depth * 12;
  return (
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
  );
}
