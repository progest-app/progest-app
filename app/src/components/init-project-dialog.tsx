import * as React from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FolderOpen, FolderPlus } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Progress } from "@/components/ui/progress";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useProject } from "@/lib/project-context";
import {
  IpcError,
  isAlreadyInitialized,
  projectInitPreview,
  type InitPreview,
  type ProgressEvent,
} from "@/lib/ipc";

/**
 * Confirmation dialog for `progest init` from the desktop app.
 *
 * Two modes share the dialog so the user can flip if they picked the
 * wrong button on the Welcome screen:
 *
 * - "new": pick a parent directory + name, mkdir + init.
 * - "existing": pick an existing directory, init in place.
 *
 * Both modes drive a live preview from `project_init_preview`: target
 * path, predicted file count (existing only), artifact list, and
 * already-initialized detection. When the target already contains a
 * `.progest/` we replace the primary action with "Open existing" so
 * the user gets out of the create flow without typing their way around
 * an error.
 */
export function InitProjectDialog() {
  const ctx = useProject();
  const open = ctx.initDialog.open;
  const onOpenChange = (next: boolean) => {
    if (!next) ctx.closeInitDialog();
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">{open ? <InitForm /> : null}</DialogContent>
    </Dialog>
  );
}

function InitForm() {
  const ctx = useProject();
  const [mode, setMode] = React.useState<"new" | "existing">(ctx.initDialog.mode);

  // "new" inputs
  const [parent, setParent] = React.useState<string>("");
  const [name, setName] = React.useState<string>("");
  // "existing" inputs
  const [existingPath, setExistingPath] = React.useState<string>("");
  const [existingName, setExistingName] = React.useState<string>("");

  // Compute the target path the preview should inspect.
  const target = React.useMemo(() => {
    if (mode === "new") {
      const trimmed = name.trim();
      if (!parent || !trimmed) return null;
      return joinPath(parent, trimmed);
    }
    return existingPath || null;
  }, [mode, parent, name, existingPath]);

  const [preview, setPreview] = React.useState<InitPreview | null>(null);
  const [previewError, setPreviewError] = React.useState<string | null>(null);
  const [previewLoading, setPreviewLoading] = React.useState(false);

  // Live preview with a small debounce — typing into the name field
  // triggers a fresh IPC call but only after the user pauses.
  React.useEffect(() => {
    if (!target) {
      setPreview(null);
      setPreviewError(null);
      setPreviewLoading(false);
      return;
    }
    let cancelled = false;
    setPreviewLoading(true);
    const timer = window.setTimeout(() => {
      void (async () => {
        try {
          const p = await projectInitPreview(target);
          if (cancelled) return;
          setPreview(p);
          setPreviewError(null);
        } catch (e) {
          if (cancelled) return;
          setPreview(null);
          setPreviewError(e instanceof IpcError ? e.raw : String(e));
        } finally {
          if (!cancelled) setPreviewLoading(false);
        }
      })();
    }, 150);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [target]);

  const [submitError, setSubmitError] = React.useState<string | null>(null);
  const [submitting, setSubmitting] = React.useState(false);
  const [progress, setProgress] = React.useState<ProgressEvent | null>(null);

  const pickParent = async () => {
    const picked = await openDialog({
      directory: true,
      multiple: false,
      title: "Pick a parent directory",
    });
    if (typeof picked === "string") setParent(picked);
  };

  const pickExisting = async () => {
    const picked = await openDialog({
      directory: true,
      multiple: false,
      title: "Pick a directory to initialize",
    });
    if (typeof picked === "string") {
      setExistingPath(picked);
      // Default name = basename, but only when the user hasn't typed
      // their own override yet.
      if (!existingName.trim()) {
        setExistingName(basename(picked));
      }
    }
  };

  const canSubmit = (() => {
    if (submitting) return false;
    if (mode === "new") {
      return parent.length > 0 && name.trim().length > 0;
    }
    return existingPath.length > 0;
  })();

  const handleSubmit = async () => {
    setSubmitting(true);
    setSubmitError(null);
    setProgress(null);
    try {
      if (mode === "new") {
        await ctx.initNew(parent, name.trim(), (e) => setProgress(e));
      } else {
        const trimmed = existingName.trim();
        await ctx.initExisting(existingPath, trimmed.length > 0 ? trimmed : null, (e) =>
          setProgress(e),
        );
      }
      ctx.closeInitDialog();
    } catch (e) {
      const msg = e instanceof IpcError ? e.raw : String(e);
      setSubmitError(msg);
    } finally {
      setSubmitting(false);
      setProgress(null);
    }
  };

  const openExisting = async () => {
    if (!target) return;
    setSubmitting(true);
    setSubmitError(null);
    try {
      await ctx.openByPath(target);
      ctx.closeInitDialog();
    } catch (e) {
      const msg = e instanceof IpcError ? e.raw : String(e);
      setSubmitError(msg);
    } finally {
      setSubmitting(false);
    }
  };

  // Preview dictates the primary CTA: when `.progest/` already lives at
  // the target we surface "Open existing" instead of "Initialize" so
  // the user doesn't bounce off `already_initialized:` errors.
  const isAlreadyProject = preview?.is_existing_project ?? false;
  const showOpenInstead = isAlreadyProject || isAlreadyInitialized(submitError);

  return (
    <>
      <DialogHeader>
        <DialogTitle>Create a Progest project</DialogTitle>
        <DialogDescription>
          Initialize a folder so Progest can index it. Existing files stay where they are.
        </DialogDescription>
      </DialogHeader>

      <ToggleGroup
        type="single"
        value={mode}
        onValueChange={(v) => {
          if (v === "new" || v === "existing") setMode(v);
        }}
        variant="outline"
        size="sm"
        className="w-full"
      >
        <ToggleGroupItem value="new" className="flex-1">
          <FolderPlus /> New folder
        </ToggleGroupItem>
        <ToggleGroupItem value="existing" className="flex-1">
          <FolderOpen /> Existing folder
        </ToggleGroupItem>
      </ToggleGroup>

      {submitting ? (
        <ProgressPanel progress={progress} />
      ) : (
        <div className="grid gap-3">
          {mode === "new" ? (
            <NewFields
              parent={parent}
              name={name}
              onPickParent={() => void pickParent()}
              onChangeName={setName}
            />
          ) : (
            <ExistingFields
              path={existingPath}
              name={existingName}
              onPickPath={() => void pickExisting()}
              onChangeName={setExistingName}
            />
          )}

          <PreviewPanel preview={preview} loading={previewLoading} error={previewError} />
        </div>
      )}

      {submitError && !showOpenInstead ? (
        <div className="rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5 text-xs text-destructive">
          {submitError}
        </div>
      ) : null}

      <DialogFooter>
        <Button variant="outline" onClick={ctx.closeInitDialog} disabled={submitting}>
          Cancel
        </Button>
        {showOpenInstead ? (
          <Button onClick={() => void openExisting()} disabled={submitting || !target}>
            Open existing project
          </Button>
        ) : (
          <Button onClick={() => void handleSubmit()} disabled={!canSubmit}>
            {submitting ? "Initializing…" : "Initialize"}
          </Button>
        )}
      </DialogFooter>
    </>
  );
}

function NewFields(props: {
  parent: string;
  name: string;
  onPickParent: () => void;
  onChangeName: (v: string) => void;
}) {
  return (
    <>
      <div className="grid gap-1.5">
        <Label htmlFor="init-parent">Parent directory</Label>
        <div className="flex items-center gap-2">
          <div
            id="init-parent"
            className="min-w-0 flex-1 truncate rounded-md border border-input bg-muted px-2 py-1.5 text-xs text-muted-foreground"
            title={props.parent}
          >
            {props.parent || "Pick a folder…"}
          </div>
          <Button variant="outline" size="sm" onClick={props.onPickParent}>
            <FolderOpen /> Choose…
          </Button>
        </div>
      </div>
      <div className="grid gap-1.5">
        <Label htmlFor="init-name">Project name</Label>
        <Input
          id="init-name"
          value={props.name}
          onChange={(e) => props.onChangeName(e.target.value)}
          placeholder="my-project"
          autoComplete="off"
          spellCheck={false}
        />
      </div>
    </>
  );
}

function ExistingFields(props: {
  path: string;
  name: string;
  onPickPath: () => void;
  onChangeName: (v: string) => void;
}) {
  return (
    <>
      <div className="grid gap-1.5">
        <Label htmlFor="init-existing-path">Directory</Label>
        <div className="flex items-center gap-2">
          <div
            id="init-existing-path"
            className="min-w-0 flex-1 truncate rounded-md border border-input bg-muted px-2 py-1.5 text-xs text-muted-foreground"
            title={props.path}
          >
            {props.path || "Pick a folder…"}
          </div>
          <Button variant="outline" size="sm" onClick={props.onPickPath}>
            <FolderOpen /> Choose…
          </Button>
        </div>
      </div>
      <div className="grid gap-1.5">
        <Label htmlFor="init-existing-name">Project name</Label>
        <Input
          id="init-existing-name"
          value={props.name}
          onChange={(e) => props.onChangeName(e.target.value)}
          placeholder="(directory basename)"
          autoComplete="off"
          spellCheck={false}
        />
      </div>
    </>
  );
}

function PreviewPanel(props: {
  preview: InitPreview | null;
  loading: boolean;
  error: string | null;
}) {
  if (props.error) {
    return (
      <div className="rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5 text-xs text-destructive">
        {props.error}
      </div>
    );
  }
  if (!props.preview) {
    return (
      <div className="rounded-md border border-dashed px-2 py-2 text-xs text-muted-foreground">
        {props.loading ? "Inspecting target…" : "Pick a folder to preview the result."}
      </div>
    );
  }
  const p = props.preview;
  return (
    <div className="grid gap-2 rounded-md border bg-muted/40 px-2 py-2 text-xs">
      <div className="grid gap-0.5">
        <div className="text-muted-foreground">Target path</div>
        <div className="break-all font-mono text-[0.7rem]">{p.target_path}</div>
      </div>

      {p.is_existing_project ? (
        <div className="rounded-sm bg-warning/10 px-2 py-1 text-warning">
          This directory is already a Progest project. Choose <em>Open existing</em> below.
        </div>
      ) : null}

      {!p.is_existing_project && p.predicted_file_count !== null ? (
        <div className="grid grid-cols-[auto_1fr] gap-x-2">
          <span className="text-muted-foreground">Files indexed on first scan</span>
          <span className="font-mono">~{p.predicted_file_count.toLocaleString()}</span>
        </div>
      ) : null}

      <div className="grid gap-1">
        <div className="text-muted-foreground">Will create</div>
        <ul className="grid grid-cols-2 gap-x-3 gap-y-0.5 font-mono text-[0.7rem]">
          {p.artifacts.map((a) => (
            <li key={a} className="flex items-center gap-1.5">
              <span>{a}</span>
              {a === ".gitignore" ? (
                <span className="text-muted-foreground">
                  ({p.gitignore_exists ? "append" : "create"})
                </span>
              ) : null}
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

function joinPath(parent: string, name: string): string {
  // Tauri picker returns OS-native paths — preserve the separator from
  // the parent rather than guessing. Trailing separators are normalized.
  const trimmedParent = parent.replace(/[\\/]+$/, "");
  if (trimmedParent.includes("\\") && !trimmedParent.includes("/")) {
    return `${trimmedParent}\\${name}`;
  }
  return `${trimmedParent}/${name}`;
}

function basename(path: string): string {
  const m = path.match(/[^\\/]+$/);
  return m ? m[0] : "";
}

function ProgressPanel(props: { progress: ProgressEvent | null }) {
  const p = props.progress;
  const pct = p && p.total > 0 ? (p.current / p.total) * 100 : undefined;
  return (
    <div className="grid gap-2 py-4">
      <div className="text-sm text-muted-foreground">{p?.message ?? "Initializing\u{2026}"}</div>
      <Progress value={pct} />
      {p && p.total > 0 ? (
        <div className="text-right text-xs text-muted-foreground">
          {p.current.toLocaleString()} / {p.total.toLocaleString()}
        </div>
      ) : null}
    </div>
  );
}
