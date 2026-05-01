import * as React from "react";
import { Pencil, Trash2 } from "lucide-react";
import { toast } from "sonner";

import { fileDeleteApply, fileDeletePreview, fsRename, type DeletePreview } from "@/lib/ipc";
import { useProject } from "@/lib/project-context";
import { DotmSquare1 } from "@/components/ui/dotm-square-1";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

type FileContextMenuProps = {
  children: React.ReactNode;
  path: string;
  onDeleted?: () => void;
};

export function FileContextMenu(props: FileContextMenuProps) {
  const { bumpRefresh } = useProject();

  // ── Rename ──────────────────────────────────────────────────────
  const [renameOpen, setRenameOpen] = React.useState(false);
  const [renameBusy, setRenameBusy] = React.useState(false);
  const [renameError, setRenameError] = React.useState<string | null>(null);
  const [newName, setNewName] = React.useState("");
  const renameInputRef = React.useRef<HTMLInputElement>(null);

  const filename = props.path.split("/").pop() ?? props.path;

  const openRename = () => {
    setNewName(filename);
    setRenameError(null);
    setRenameOpen(true);
    setTimeout(() => {
      const input = renameInputRef.current;
      if (!input) return;
      input.focus();
      const dot = filename.lastIndexOf(".");
      input.setSelectionRange(0, dot > 0 ? dot : filename.length);
    }, 50);
  };

  const handleRename = async () => {
    const trimmed = newName.trim();
    if (!trimmed || trimmed === filename) {
      setRenameOpen(false);
      return;
    }
    setRenameBusy(true);
    setRenameError(null);
    try {
      await fsRename(props.path, trimmed);
      setRenameOpen(false);
      bumpRefresh();
      toast.success(`Renamed to ${trimmed}`);
    } catch (e) {
      setRenameError(String(e));
    } finally {
      setRenameBusy(false);
    }
  };

  // ── Delete ──────────────────────────────────────────────────────
  const [confirmOpen, setConfirmOpen] = React.useState(false);
  const [preview, setPreview] = React.useState<DeletePreview | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  const openConfirm = async () => {
    setError(null);
    try {
      const p = await fileDeletePreview(props.path);
      setPreview(p);
      setConfirmOpen(true);
    } catch (e) {
      setError(String(e));
    }
  };

  const handleDelete = async () => {
    setBusy(true);
    setError(null);
    try {
      await fileDeleteApply(props.path);
      setConfirmOpen(false);
      bumpRefresh();
      toast.success("Moved to Trash", { description: filename });
      props.onDeleted?.();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>{props.children}</ContextMenuTrigger>
        <ContextMenuContent>
          <ContextMenuItem onClick={openRename}>
            <Pencil className="mr-2 size-3.5" />
            Rename
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem
            className="text-destructive focus:text-destructive"
            onClick={() => void openConfirm()}
          >
            <Trash2 className="mr-2 size-3.5" />
            Move to Trash
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      {/* Rename dialog */}
      <Dialog open={renameOpen} onOpenChange={setRenameOpen}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>Rename</DialogTitle>
            <DialogDescription>Enter a new name for {filename}.</DialogDescription>
          </DialogHeader>
          <Input
            ref={renameInputRef}
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                void handleRename();
              }
            }}
            className="font-mono text-xs"
          />
          {renameError ? <div className="text-xs text-destructive">{renameError}</div> : null}
          <DialogFooter>
            <Button variant="outline" onClick={() => setRenameOpen(false)}>
              Cancel
            </Button>
            <Button onClick={() => void handleRename()} disabled={renameBusy}>
              {renameBusy ? (
                <>
                  <DotmSquare1 size={16} dotSize={2} animated className="mr-1.5" />
                  Renaming…
                </>
              ) : (
                "Rename"
              )}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Delete confirm dialog */}
      <Dialog open={confirmOpen} onOpenChange={setConfirmOpen}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>Move to Trash?</DialogTitle>
            <DialogDescription>
              This will move the file to the OS trash. You can restore it from the trash later.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-0.5 rounded border bg-muted/30 p-2 font-mono text-xs">
            <div className="truncate">{filename}</div>
            {preview?.has_sidecar ? (
              <div className="text-muted-foreground">+ .meta sidecar</div>
            ) : null}
          </div>
          {error ? <div className="text-xs text-destructive">{error}</div> : null}
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirmOpen(false)}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={() => void handleDelete()} disabled={busy}>
              {busy ? (
                <>
                  <DotmSquare1 size={16} dotSize={2} animated className="mr-1.5" />
                  Deleting…
                </>
              ) : (
                "Move to Trash"
              )}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
