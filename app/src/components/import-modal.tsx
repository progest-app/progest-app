import * as React from "react";
import {
  AlertTriangle,
  Check,
  ChevronsUpDown,
  Copy,
  FileWarning,
  Folder,
  FolderInput,
  Scissors,
  Star,
} from "lucide-react";

import { toast } from "sonner";

import { DotmSquare8 } from "@/components/ui/dotm-square-8";
import { Progress } from "@/components/ui/progress";

import {
  importApply,
  importPreview,
  importRanking,
  type ImportConflict,
  type ImportOp,
  type ImportPreview,
  type ImportRequestWire,
  type ProgressEvent,
  type SuggestedDestination,
} from "@/lib/ipc";
import { useProject } from "@/lib/project-context";
import { Button } from "@/components/ui/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
} from "@/components/ui/command";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";

type ImportModalProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  sources: string[];
  initialDest: string | undefined;
};

export function ImportModal(props: ImportModalProps) {
  const { open, onOpenChange, sources, initialDest } = props;
  const { bumpRefresh } = useProject();

  const [mode, setMode] = React.useState<"copy" | "move">("copy");
  const [dest, setDest] = React.useState("");
  const [suggestions, setSuggestions] = React.useState<SuggestedDestination[]>([]);
  const [allDirs, setAllDirs] = React.useState<string[]>([]);
  const [preview, setPreview] = React.useState<ImportPreview | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [progress, setProgress] = React.useState<ProgressEvent | null>(null);
  const [dirPickerOpen, setDirPickerOpen] = React.useState(false);

  React.useEffect(() => {
    if (!open || sources.length === 0) return;
    setDest(initialDest ?? "");
    setPreview(null);
    setError(null);
    setMode("copy");
    setSuggestions([]);
    setAllDirs([]);

    importRanking(sources)
      .then((resp) => {
        setSuggestions(resp.suggestions);
        setAllDirs(resp.all_dirs);
        const first = resp.suggestions[0];
        if (!initialDest && first) {
          setDest(first.path);
        }
      })
      .catch(() => {});
  }, [open, sources, initialDest]);

  React.useEffect(() => {
    if (!open || sources.length === 0) return;
    if (!dest && !initialDest) {
      setPreview(null);
      return;
    }

    const activeDest = dest || "";
    const requests: ImportRequestWire[] = sources.map((src) => {
      const filename = src.split("/").pop() ?? src;
      const destPath = activeDest ? `${activeDest}/${filename}` : filename;
      return { source: src, dest: destPath, mode };
    });

    let cancelled = false;
    importPreview(requests)
      .then((p) => {
        if (!cancelled) setPreview(p);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });

    return () => {
      cancelled = true;
    };
  }, [open, sources, dest, mode, initialDest]);

  const handleApply = async () => {
    if (!preview || !preview.clean || busy) return;
    setBusy(true);
    setError(null);
    setProgress(null);
    try {
      const requests: ImportRequestWire[] = sources.map((src) => {
        const filename = src.split("/").pop() ?? src;
        const destPath = dest ? `${dest}/${filename}` : filename;
        return { source: src, dest: destPath, mode };
      });
      const result = await importApply(requests, (e) => setProgress(e));
      bumpRefresh();
      onOpenChange(false);
      const count = result.imported.length;
      const warnCount = result.warnings.length;
      if (warnCount > 0) {
        toast.warning(
          `Imported ${count} file${count === 1 ? "" : "s"} with ${warnCount} warning${warnCount === 1 ? "" : "s"}`,
        );
      } else {
        toast.success(`Imported ${count} file${count === 1 ? "" : "s"}`);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const fileNames = React.useMemo(() => sources.map((s) => s.split("/").pop() ?? s), [sources]);

  const suggestedPaths = React.useMemo(
    () => new Set(suggestions.map((s) => s.path)),
    [suggestions],
  );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <FolderInput className="size-5" />
            Import {sources.length} file{sources.length === 1 ? "" : "s"}
          </DialogTitle>
          <DialogDescription>Choose a destination directory and import mode.</DialogDescription>
        </DialogHeader>

        <div className="grid gap-4">
          {/* File list preview */}
          <div className="max-h-32 overflow-auto rounded border bg-muted/30 p-2 text-xs font-mono space-y-0.5">
            {fileNames.map((name, i) => (
              <div key={`${name}-${i}`} className="truncate">
                {name}
              </div>
            ))}
          </div>

          {/* Destination — combobox with autocomplete */}
          <div className="grid gap-1.5">
            <Label>Destination directory</Label>
            <Popover open={dirPickerOpen} onOpenChange={setDirPickerOpen}>
              <PopoverTrigger asChild>
                <Button
                  variant="outline"
                  role="combobox"
                  aria-expanded={dirPickerOpen}
                  className="justify-between font-mono text-xs h-9"
                >
                  <span className="truncate">{dest || "(project root)"}</span>
                  <ChevronsUpDown className="ml-2 size-3.5 shrink-0 opacity-50" />
                </Button>
              </PopoverTrigger>
              <PopoverContent className="w-[--radix-popover-trigger-width] p-0" align="start">
                <Command>
                  <CommandInput placeholder="Search directories…" className="text-xs" />
                  <CommandEmpty>No directory found.</CommandEmpty>
                  {suggestions.length > 0 ? (
                    <CommandGroup heading="Suggested">
                      {suggestions.map((s) => (
                        <CommandItem
                          key={`suggest-${s.path}`}
                          value={s.path || "(root)"}
                          onSelect={() => {
                            setDest(s.path);
                            setDirPickerOpen(false);
                          }}
                          className="text-xs font-mono"
                        >
                          <Star className="mr-1.5 size-3 text-primary shrink-0" />
                          <span className="truncate">{s.path || "(project root)"}</span>
                          <span className="ml-auto text-[0.625rem] text-muted-foreground shrink-0">
                            {scoreLabel(s.score)}
                          </span>
                          {dest === s.path ? <Check className="ml-1 size-3 shrink-0" /> : null}
                        </CommandItem>
                      ))}
                    </CommandGroup>
                  ) : null}
                  <CommandGroup heading="All directories" className="max-h-48 overflow-auto">
                    <CommandItem
                      value="(root)"
                      onSelect={() => {
                        setDest("");
                        setDirPickerOpen(false);
                      }}
                      className="text-xs font-mono"
                    >
                      <Folder className="mr-1.5 size-3 shrink-0" />
                      (project root)
                      {dest === "" ? <Check className="ml-auto size-3 shrink-0" /> : null}
                    </CommandItem>
                    {allDirs
                      .filter((d) => !suggestedPaths.has(d))
                      .map((d) => (
                        <CommandItem
                          key={d}
                          value={d}
                          onSelect={() => {
                            setDest(d);
                            setDirPickerOpen(false);
                          }}
                          className="text-xs font-mono"
                        >
                          <Folder className="mr-1.5 size-3 shrink-0" />
                          <span className="truncate">{d}</span>
                          {dest === d ? <Check className="ml-auto size-3 shrink-0" /> : null}
                        </CommandItem>
                      ))}
                  </CommandGroup>
                </Command>
              </PopoverContent>
            </Popover>
          </div>

          {/* Mode toggle */}
          <div className="grid gap-1.5">
            <Label>Import mode</Label>
            <div className="flex gap-2">
              <Button
                variant={mode === "copy" ? "default" : "outline"}
                size="sm"
                onClick={() => setMode("copy")}
              >
                <Copy className="size-3.5 mr-1" /> Copy
              </Button>
              <Button
                variant={mode === "move" ? "default" : "outline"}
                size="sm"
                onClick={() => setMode("move")}
              >
                <Scissors className="size-3.5 mr-1" /> Move
              </Button>
            </div>
            {mode === "move" ? (
              <p className="text-xs text-warning">Move deletes the original files after import.</p>
            ) : null}
          </div>

          {/* Preview / conflicts */}
          {preview ? (
            preview.clean ? (
              <div className="flex items-center gap-1.5 text-xs text-green-600 dark:text-green-400">
                <Check className="size-3.5" />
                {preview.ops.length} file{preview.ops.length === 1 ? "" : "s"} ready to import
              </div>
            ) : (
              <div className="space-y-1.5">
                <div className="flex items-center gap-1.5 text-xs text-warning">
                  <AlertTriangle className="size-3.5" />
                  {preview.conflict_count} conflict{preview.conflict_count === 1 ? "" : "s"}
                </div>
                <div className="max-h-32 overflow-auto rounded border border-warning/30 bg-warning/5 p-2 text-xs space-y-1">
                  {preview.ops
                    .filter((op) => op.conflicts.length > 0)
                    .map((op) => (
                      <ConflictRow key={op.source} op={op} />
                    ))}
                </div>
              </div>
            )
          ) : null}

          {busy && progress ? (
            <div className="grid gap-1.5">
              <div className="text-xs text-muted-foreground">{progress.message}</div>
              <Progress
                value={progress.total > 0 ? (progress.current / progress.total) * 100 : undefined}
              />
              {progress.total > 0 ? (
                <div className="text-right text-xs text-muted-foreground">
                  {progress.current} / {progress.total}
                </div>
              ) : null}
            </div>
          ) : null}

          {error ? <div className="text-xs text-destructive">{error}</div> : null}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={() => void handleApply()} disabled={busy || !preview?.clean}>
            {busy ? (
              <>
                <DotmSquare8 size={16} dotSize={2} animated className="mr-1.5" />
                Importing…
              </>
            ) : (
              `Import ${sources.length} file${sources.length === 1 ? "" : "s"}`
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function scoreLabel(score: number): string {
  if (score === 3) return "exact";
  if (score === 2) return "alias";
  return "inherited";
}

function ConflictRow(props: { op: ImportOp }) {
  const filename = props.op.source.split("/").pop() ?? props.op.source;
  return (
    <div>
      <div className="font-mono truncate font-medium">{filename}</div>
      {props.op.conflicts.map((c, i) => (
        <ConflictDetail key={i} conflict={c} />
      ))}
    </div>
  );
}

function ConflictDetail(props: { conflict: ImportConflict }) {
  const c = props.conflict;
  switch (c.kind) {
    case "dest_exists":
      return (
        <div className="flex items-center gap-1 text-warning pl-2">
          <FileWarning className="size-3 shrink-0" />
          Destination exists: {c.existing_path}
        </div>
      );
    case "source_missing":
      return (
        <div className="flex items-center gap-1 text-destructive pl-2">
          <AlertTriangle className="size-3 shrink-0" />
          Source not found: {c.reason}
        </div>
      );
    case "source_is_project":
      return (
        <div className="flex items-center gap-1 text-warning pl-2">
          <AlertTriangle className="size-3 shrink-0" />
          Already in project — use rename instead
        </div>
      );
    case "placement_mismatch":
      return (
        <div className="flex items-center gap-1 text-warning pl-2">
          <AlertTriangle className="size-3 shrink-0" />
          Dir doesn't accept this extension
          {c.suggestion ? ` (try ${c.suggestion})` : ""}
        </div>
      );
    default:
      return null;
  }
}
