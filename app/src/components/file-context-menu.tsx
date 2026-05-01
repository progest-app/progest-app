import * as React from "react";
import { Trash2 } from "lucide-react";
import { toast } from "sonner";

import { fileDeleteApply, fileDeletePreview, type DeletePreview } from "@/lib/ipc";
import { useProject } from "@/lib/project-context";
import { DotmSquare1 } from "@/components/ui/dotm-square-1";
import { Button } from "@/components/ui/button";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
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

  const filename = props.path.split("/").pop() ?? props.path;

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>{props.children}</ContextMenuTrigger>
        <ContextMenuContent>
          <ContextMenuItem
            className="text-destructive focus:text-destructive"
            onClick={() => void openConfirm()}
          >
            <Trash2 className="size-3.5 mr-2" />
            Move to Trash
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      <Dialog open={confirmOpen} onOpenChange={setConfirmOpen}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>Move to Trash?</DialogTitle>
            <DialogDescription>
              This will move the file to the OS trash. You can restore it from the trash later.
            </DialogDescription>
          </DialogHeader>
          <div className="rounded border bg-muted/30 p-2 text-xs font-mono space-y-0.5">
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
