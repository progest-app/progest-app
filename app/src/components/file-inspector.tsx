import * as React from "react";
import { Plus, Trash2, X } from "lucide-react";
import { toast } from "sonner";

import { RefreshCw, Settings } from "lucide-react";

import {
  IpcError,
  aiApplyRename,
  aiGetConfig,
  aiSuggest,
  fileDeleteApply,
  fileDeletePreview,
  notesRead,
  notesWrite,
  tagAdd,
  tagRemove,
  type AiConfigResponse,
  type AiSuggestionWire,
  type DeletePreview,
  type RichSearchHit,
} from "@/lib/ipc";
import { useProject } from "@/lib/project-context";
import { useSettings } from "@/lib/settings-context";
import { DotmSquare1 } from "@/components/ui/dotm-square-1";
import { DotmSquare12 } from "@/components/ui/dotm-square-12";
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
import { Textarea } from "@/components/ui/textarea";
import { ViolationBadges } from "@/components/violation-badges";
import { cn } from "@/lib/utils";

const NOTES_DEBOUNCE_MS = 600;

/**
 * File-mode inspector. Renders the static fields the user used to see
 * in the result-detail dialog (path / kind / ext / file_id / violations
 * / custom fields) plus inline editors for tags and `[notes].body`.
 *
 * Mutations route through `tag_add` / `tag_remove` / `notes_write`,
 * and bump the project-wide `refreshTick` so FlatView and TreeView
 * pick up the new tag / notes state without manual refresh.
 *
 * Files that haven't been reconciled yet (no `file_id`) render the
 * read-only fields but disable the editors — there's nothing in the
 * index to attach the mutation to.
 */
export type FileInspectorHandle = {
  hasPendingSuggestions: () => boolean;
};

export const FileInspector = React.forwardRef<
  FileInspectorHandle,
  { hit: RichSearchHit; onDeleted?: (() => void) | undefined }
>(function FileInspector(props, ref) {
  const { hit, onDeleted } = props;
  const isIndexed = hit.file_id.length > 0;
  const pendingRef = React.useRef(false);

  React.useImperativeHandle(ref, () => ({
    hasPendingSuggestions: () => pendingRef.current,
  }));

  return (
    <div className="grid h-full grid-rows-[auto_1fr] overflow-hidden">
      <header className="border-b px-3 py-2">
        <div className="text-[0.625rem] uppercase tracking-wide text-muted-foreground">File</div>
        <div className="break-all font-mono text-xs/relaxed">{hit.path}</div>
      </header>
      <div className="grid auto-rows-min gap-3 overflow-y-auto px-3 py-3 text-xs">
        <FileSummary hit={hit} />
        <TagsEditor hit={hit} disabled={!isIndexed} />
        <NotesEditor hit={hit} disabled={!isIndexed} />
        <CustomFieldsBlock hit={hit} />
        {isIndexed ? <AiSuggestionsSection hit={hit} pendingRef={pendingRef} /> : null}
        {!isIndexed ? (
          <div className="rounded-md border border-warning/40 bg-warning/10 px-2 py-1.5 text-warning">
            This file isn&apos;t indexed yet — run <code>progest scan</code> (or wait for the next
            reconcile) before editing tags or notes.
          </div>
        ) : null}
        {isIndexed ? <DeleteSection path={hit.path} onDeleted={onDeleted} /> : null}
      </div>
    </div>
  );
});

function DeleteSection(props: { path: string; onDeleted?: (() => void) | undefined }) {
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
      <div className="mt-2 border-t pt-3">
        <Button
          variant="outline"
          size="sm"
          className="text-destructive hover:bg-destructive/10 hover:text-destructive"
          onClick={() => void openConfirm()}
        >
          <Trash2 className="size-3.5 mr-1" />
          Move to Trash
        </Button>
        {error ? <div className="mt-1 text-destructive">{error}</div> : null}
      </div>

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

function FileSummary(props: { hit: RichSearchHit }) {
  const { hit } = props;
  return (
    <div className="grid gap-1.5">
      <Row label="Name" value={hit.name ?? hit.path.split("/").pop() ?? ""} mono />
      <Row label="Kind" value={hit.kind} />
      {hit.ext ? <Row label="Extension" value={hit.ext} mono /> : null}
      <div className="grid grid-cols-[5.5rem_1fr] items-start gap-2">
        <span className="text-muted-foreground">Violations</span>
        {hit.violations.naming + hit.violations.placement + hit.violations.sequence > 0 ? (
          <ViolationBadges counts={hit.violations} className="" />
        ) : (
          <span className="text-muted-foreground">—</span>
        )}
      </div>
      {hit.file_id ? (
        <Row label="File ID" value={hit.file_id} mono className="text-muted-foreground" />
      ) : null}
    </div>
  );
}

function Row(props: { label: string; value: string; mono?: boolean; className?: string }) {
  return (
    <div className="grid grid-cols-[5.5rem_1fr] items-start gap-2">
      <span className="text-muted-foreground">{props.label}</span>
      <span className={cn("min-w-0 break-words", props.mono ? "font-mono" : null, props.className)}>
        {props.value}
      </span>
    </div>
  );
}

function TagsEditor(props: { hit: RichSearchHit; disabled: boolean }) {
  const { hit, disabled } = props;
  const { bumpRefresh } = useProject();
  const [tags, setTags] = React.useState<string[]>(hit.tags);
  const [draft, setDraft] = React.useState("");
  const [error, setError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  // Tracks the tag string for any in-flight `tagAdd` call. Prevents
  // double-fire when Enter and onBlur land back-to-back (the input
  // can blur right after Enter, queueing a second submission with
  // the same draft before React commits the first `setDraft("")`).
  const pendingAdd = React.useRef<string | null>(null);

  // Reset to the upstream value whenever the inspector switches files.
  React.useEffect(() => {
    setTags(hit.tags);
    setDraft("");
    setError(null);
    pendingAdd.current = null;
  }, [hit.file_id, hit.path, hit.tags]);

  const submitDraft = async () => {
    const tag = draft.trim();
    // Early-out paths that don't need the IPC round-trip. Critically,
    // `pendingAdd.current === tag` guards against the Enter→blur race.
    if (tag.length === 0 || tags.includes(tag) || pendingAdd.current === tag || disabled) {
      if (tag.length === 0 || tags.includes(tag)) setDraft("");
      return;
    }
    pendingAdd.current = tag;
    setDraft("");
    setBusy(true);
    setError(null);
    try {
      await tagAdd(hit.file_id, tag);
      // Dedupe at insertion time too — the backend is idempotent, but
      // the optimistic update would otherwise let two concurrent
      // submissions of the same tag append twice.
      setTags((prev) => {
        if (prev.includes(tag)) return prev;
        const next = [...prev, tag];
        next.sort((a, b) => a.localeCompare(b));
        return next;
      });
      bumpRefresh();
    } catch (e) {
      setError(e instanceof IpcError ? e.raw : String(e));
    } finally {
      pendingAdd.current = null;
      setBusy(false);
    }
  };

  const removeTag = async (tag: string) => {
    if (disabled) return;
    setBusy(true);
    setError(null);
    try {
      await tagRemove(hit.file_id, tag);
      setTags((prev) => prev.filter((t) => t !== tag));
      bumpRefresh();
    } catch (e) {
      setError(e instanceof IpcError ? e.raw : String(e));
    } finally {
      setBusy(false);
    }
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      void submitDraft();
    } else if (e.key === "Backspace" && draft.length === 0 && tags.length > 0) {
      e.preventDefault();
      void removeTag(tags[tags.length - 1]!);
    }
  };

  return (
    <section className="grid gap-1.5">
      <Label className="text-muted-foreground">Tags</Label>
      <div
        className={cn(
          "flex flex-wrap items-center gap-1 rounded-md border bg-input/20 px-2 py-1.5 dark:bg-input/30",
          disabled ? "opacity-50" : null,
        )}
      >
        {tags.map((tag) => (
          <span
            key={tag}
            className="inline-flex items-center gap-1 rounded-sm bg-muted px-1.5 py-0.5 font-mono"
          >
            #{tag}
            <button
              type="button"
              aria-label={`Remove tag ${tag}`}
              onClick={() => void removeTag(tag)}
              disabled={disabled || busy}
              className="rounded-sm text-muted-foreground hover:text-foreground disabled:cursor-not-allowed"
            >
              <X className="size-3" />
            </button>
          </span>
        ))}
        <Input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={onKeyDown}
          onBlur={() => void submitDraft()}
          disabled={disabled || busy}
          placeholder={tags.length === 0 ? "add tag…" : ""}
          className="h-6 min-w-24 flex-1 border-0 bg-transparent px-1 shadow-none focus-visible:ring-0 dark:bg-transparent"
        />
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          onClick={() => void submitDraft()}
          disabled={disabled || busy || draft.trim().length === 0}
          aria-label="Add tag"
        >
          <Plus />
        </Button>
      </div>
      {error ? <div className="text-destructive">{error}</div> : null}
    </section>
  );
}

function NotesEditor(props: { hit: RichSearchHit; disabled: boolean }) {
  const { hit, disabled } = props;
  const { bumpRefresh } = useProject();
  const [body, setBody] = React.useState("");
  // Track the last persisted body so we don't fire a write for changes
  // that just came back from the server.
  const persisted = React.useRef("");
  const [loading, setLoading] = React.useState(false);
  const [saving, setSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  // Load the sidecar's current body whenever the selected file
  // changes. `notes_read` returns `body=""` for files without a
  // sidecar yet, so the textarea starts empty.
  React.useEffect(() => {
    let cancelled = false;
    if (!hit.path) return;
    setLoading(true);
    notesRead(hit.path)
      .then((res) => {
        if (cancelled) return;
        setBody(res.body);
        persisted.current = res.body;
        setError(null);
      })
      .catch((e) => {
        if (cancelled) return;
        setError(e instanceof IpcError ? e.raw : String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [hit.path]);

  // Debounced save: cheap to keep the textarea responsive without
  // hammering the sidecar atomic-write path on every keystroke.
  React.useEffect(() => {
    if (disabled) return;
    if (body === persisted.current) return;
    const handle = setTimeout(() => {
      setSaving(true);
      setError(null);
      notesWrite(hit.path, body)
        .then(() => {
          persisted.current = body;
          bumpRefresh();
        })
        .catch((e) => {
          setError(e instanceof IpcError ? e.raw : String(e));
        })
        .finally(() => {
          setSaving(false);
        });
    }, NOTES_DEBOUNCE_MS);
    return () => clearTimeout(handle);
  }, [body, hit.path, disabled, bumpRefresh]);

  return (
    <section className="grid gap-1.5">
      <div className="flex items-center justify-between">
        <Label className="text-muted-foreground">Notes</Label>
        <span className="text-[0.625rem] text-muted-foreground">
          {loading ? "loading…" : saving ? "saving…" : null}
        </span>
      </div>
      <Textarea
        value={body}
        onChange={(e) => setBody(e.target.value)}
        disabled={disabled || loading}
        placeholder={disabled ? "" : "Free-form notes for this file…"}
        rows={6}
        className="resize-y"
      />
      {error ? <div className="text-destructive">{error}</div> : null}
    </section>
  );
}

function CustomFieldsBlock(props: { hit: RichSearchHit }) {
  const fields = props.hit.custom_fields;
  if (fields.length === 0) return null;
  return (
    <section className="grid gap-1.5">
      <Label className="text-muted-foreground">Custom fields</Label>
      <ul className="grid gap-0.5 rounded-md border bg-muted/30 px-2 py-1.5 font-mono">
        {fields.map((f) => (
          <li key={f.key} className="grid grid-cols-[max-content_1fr] gap-2">
            <span className="text-muted-foreground">{f.key}</span>
            <span className="break-words">{String(f.value)}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}

// ── AI Suggestions ─────────────────────────────────────────────────

const AI_TYPES = ["naming", "tags", "notes", "placement"] as const;
type AiType = (typeof AI_TYPES)[number];

function AiSuggestionsSection(props: {
  hit: RichSearchHit;
  pendingRef: React.MutableRefObject<boolean>;
}) {
  const { hit, pendingRef } = props;
  const { bumpRefresh } = useProject();
  const { openSettings } = useSettings();
  const [config, setConfig] = React.useState<AiConfigResponse | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [suggestions, setSuggestions] = React.useState<AiSuggestionWire[]>([]);
  const [activeType, setActiveType] = React.useState<AiType>("naming");
  const [includeNotes, setIncludeNotes] = React.useState(false);

  React.useEffect(() => {
    pendingRef.current = suggestions.length > 0;
  }, [suggestions, pendingRef]);

  React.useEffect(() => {
    let cancelled = false;
    aiGetConfig()
      .then((c) => {
        if (!cancelled) setConfig(c);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  React.useEffect(() => {
    setSuggestions([]);
    setError(null);
  }, [hit.path]);

  const handleSuggest = async (type_: AiType) => {
    setActiveType(type_);
    setBusy(true);
    setError(null);
    setSuggestions([]);
    try {
      const resp = await aiSuggest(hit.path, type_, includeNotes);
      setSuggestions(resp.suggestions);
    } catch (e) {
      setError(e instanceof IpcError ? e.raw : String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleRegenerate = () => void handleSuggest(activeType);

  const handleApplyTag = async (tag: string) => {
    try {
      await tagAdd(hit.file_id, tag);
      setSuggestions((prev) => prev.filter((s) => s.value !== tag));
      bumpRefresh();
      toast.success(`Tag "${tag}" added`);
    } catch (e) {
      toast.error(e instanceof IpcError ? e.raw : String(e));
    }
  };

  const handleApplyRename = async (newName: string) => {
    try {
      const result = await aiApplyRename(hit.path, newName);
      setSuggestions([]);
      bumpRefresh();
      toast.success(`Renamed to ${result.new_path.split("/").pop()}`);
    } catch (e) {
      toast.error(e instanceof IpcError ? e.raw : String(e));
    }
  };

  const handleApplyNotes = async (notes: string) => {
    try {
      await notesWrite(hit.path, notes);
      setSuggestions([]);
      bumpRefresh();
      toast.success("Notes updated");
    } catch (e) {
      toast.error(e instanceof IpcError ? e.raw : String(e));
    }
  };

  if (!config) return null;

  if (!config.has_key) {
    return (
      <section className="grid gap-1.5">
        <Label className="text-muted-foreground">AI suggestions</Label>
        <Button size="xs" variant="outline" onClick={() => openSettings("ai")}>
          <Settings className="mr-1 size-3" />
          Configure AI
        </Button>
      </section>
    );
  }

  return (
    <section className="grid gap-1.5">
      <div className="flex items-center justify-between">
        <Label className="text-muted-foreground">AI suggestions</Label>
        <button
          type="button"
          className="text-[0.625rem] text-muted-foreground underline underline-offset-2 hover:text-foreground"
          onClick={() => openSettings("ai")}
        >
          {config.provider}/{config.model.split("-").slice(0, 2).join("-")}
        </button>
      </div>
      <div className="flex flex-wrap gap-1">
        {AI_TYPES.map((t) => (
          <Button
            key={t}
            size="xs"
            variant={activeType === t && suggestions.length > 0 ? "default" : "outline"}
            onClick={() => void handleSuggest(t)}
            disabled={busy}
          >
            {t}
          </Button>
        ))}
      </div>
      {activeType === "notes" ? (
        <label className="flex items-center gap-1.5 text-muted-foreground">
          <input
            type="checkbox"
            checked={includeNotes}
            onChange={(e) => setIncludeNotes(e.target.checked)}
            className="accent-primary"
          />
          Include existing notes in AI context
        </label>
      ) : null}
      {busy ? (
        <div className="flex items-center gap-1.5 text-muted-foreground">
          <DotmSquare12 size={16} dotSize={2} animated />
          Generating…
        </div>
      ) : null}
      {error ? <div className="text-destructive">{error}</div> : null}
      {suggestions.length > 0 ? (
        <>
          <div className="flex items-center justify-between">
            <span className="text-[0.625rem] text-muted-foreground">
              {suggestions.length} suggestion{suggestions.length > 1 ? "s" : ""}
            </span>
            <Button size="xs" variant="ghost" onClick={handleRegenerate} disabled={busy}>
              <RefreshCw className="mr-1 size-3" />
              Regenerate
            </Button>
          </div>
          <ul className="grid gap-1">
            {suggestions.map((s, i) => (
              <li key={i} className="rounded-md border bg-muted/30 px-2 py-1.5">
                <div className="flex items-start justify-between gap-2">
                  <span className="break-words font-mono">{s.value}</span>
                  {activeType === "naming" ? (
                    <Button
                      size="xs"
                      variant="ghost"
                      onClick={() => void handleApplyRename(s.value)}
                    >
                      Rename
                    </Button>
                  ) : null}
                  {activeType === "tags" ? (
                    <Button size="xs" variant="ghost" onClick={() => void handleApplyTag(s.value)}>
                      Apply
                    </Button>
                  ) : null}
                  {activeType === "notes" ? (
                    <Button
                      size="xs"
                      variant="ghost"
                      onClick={() => void handleApplyNotes(s.value)}
                    >
                      Apply
                    </Button>
                  ) : null}
                </div>
                {s.explanation ? (
                  <div className="mt-0.5 text-muted-foreground">{s.explanation}</div>
                ) : null}
              </li>
            ))}
          </ul>
        </>
      ) : null}
    </section>
  );
}
