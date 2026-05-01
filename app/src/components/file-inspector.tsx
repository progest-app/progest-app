import * as React from "react";
import { Plus, RefreshCw, Settings, Sparkles, Trash2, X } from "lucide-react";
import { toast } from "sonner";

import {
  IpcError,
  aiApplyRename,
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
import { DotmSquare5 } from "@/components/ui/dotm-square-5";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { ViolationBadges } from "@/components/violation-badges";
import { cn } from "@/lib/utils";

const NOTES_DEBOUNCE_MS = 600;

export type FileInspectorHandle = {
  hasPendingSuggestions: () => boolean;
};

// ── AI suggestion hook ─────────────────────────────────────────────

type AiType = "naming" | "tags" | "notes";

function useAiSuggestion(hit: RichSearchHit, type: AiType) {
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [suggestions, setSuggestions] = React.useState<AiSuggestionWire[]>([]);
  const [includeNotes, setIncludeNotes] = React.useState(false);

  const generate = React.useCallback(async () => {
    setBusy(true);
    setError(null);
    setSuggestions([]);
    try {
      const resp = await aiSuggest(hit.path, type, type === "notes" && includeNotes);
      setSuggestions(resp.suggestions);
    } catch (e) {
      setError(e instanceof IpcError ? e.raw : String(e));
    } finally {
      setBusy(false);
    }
  }, [hit.path, type, includeNotes]);

  React.useEffect(() => {
    setSuggestions([]);
    setError(null);
  }, [hit.path]);

  return { busy, error, suggestions, setSuggestions, generate, includeNotes, setIncludeNotes };
}

// ── Shared AI UI components ────────────────────────────────────────

function AiButton(props: {
  aiConfig: AiConfigResponse | null;
  busy: boolean;
  onGenerate: () => void;
}) {
  const { openSettings } = useSettings();
  if (!props.aiConfig) return null;
  if (!props.aiConfig.has_key) {
    return (
      <Button
        size="icon-xs"
        variant="ghost"
        onClick={() => openSettings("ai")}
        title="Configure AI"
      >
        <Settings className="size-3" />
      </Button>
    );
  }
  return (
    <Button
      size="icon-xs"
      variant="ghost"
      onClick={props.onGenerate}
      disabled={props.busy}
      title="AI suggest"
    >
      <Sparkles className="size-3" />
    </Button>
  );
}

function AiSuggestionsList(props: {
  ai: ReturnType<typeof useAiSuggestion>;
  renderAction: (s: AiSuggestionWire) => React.ReactNode;
  notesCheckbox?: boolean;
}) {
  const { ai } = props;
  return (
    <>
      {props.notesCheckbox ? (
        <label className="flex items-center gap-1.5 text-muted-foreground">
          <Checkbox
            checked={ai.includeNotes}
            onCheckedChange={(v) => ai.setIncludeNotes(v === true)}
          />
          Include existing notes
        </label>
      ) : null}
      {ai.busy ? (
        <div className="flex items-center gap-1.5 text-muted-foreground">
          <DotmSquare5 size={16} dotSize={2} animated />
          Generating…
        </div>
      ) : null}
      {ai.error ? <div className="text-destructive">{ai.error}</div> : null}
      {ai.suggestions.length > 0 ? (
        <>
          <div className="flex items-center justify-between">
            <span className="text-[0.625rem] text-muted-foreground">
              {ai.suggestions.length} suggestion{ai.suggestions.length > 1 ? "s" : ""}
            </span>
            <Button size="xs" variant="ghost" onClick={() => void ai.generate()} disabled={ai.busy}>
              <RefreshCw className="mr-1 size-3" />
              Regenerate
            </Button>
          </div>
          <ul className="grid gap-1">
            {ai.suggestions.map((s) => (
              <li key={s.value} className="rounded-md border bg-muted/30 px-2 py-1.5">
                <div className="flex items-start justify-between gap-2">
                  <span className="break-words font-mono">{s.value}</span>
                  {props.renderAction(s)}
                </div>
                {s.explanation ? (
                  <div className="mt-0.5 text-muted-foreground">{s.explanation}</div>
                ) : null}
              </li>
            ))}
          </ul>
        </>
      ) : null}
    </>
  );
}

// ── FileInspector ──────────────────────────────────────────────────

export const FileInspector = React.forwardRef<
  FileInspectorHandle,
  {
    hit: RichSearchHit;
    onDeleted?: (() => void) | undefined;
    onSelectionUpdate?: ((hit: RichSearchHit) => void) | undefined;
  }
>(function FileInspector(props, ref) {
  const { hit, onDeleted, onSelectionUpdate } = props;
  const { bumpRefresh } = useProject();
  const { aiConfig } = useSettings();
  const [localHit, setLocalHit] = React.useState(hit);
  React.useEffect(() => {
    setLocalHit(hit);
  }, [hit]);

  const isIndexed = localHit.file_id.length > 0;
  const pendingRef = React.useRef(false);
  const [notesVersion, setNotesVersion] = React.useState(0);

  const namingAi = useAiSuggestion(localHit, "naming");
  const tagsAi = useAiSuggestion(localHit, "tags");
  const notesAi = useAiSuggestion(localHit, "notes");

  React.useEffect(() => {
    pendingRef.current =
      namingAi.suggestions.length > 0 ||
      tagsAi.suggestions.length > 0 ||
      notesAi.suggestions.length > 0;
  }, [namingAi.suggestions, tagsAi.suggestions, notesAi.suggestions]);

  React.useImperativeHandle(ref, () => ({
    hasPendingSuggestions: () => pendingRef.current,
  }));

  const handleApplyRename = React.useCallback(
    async (newName: string) => {
      try {
        const result = await aiApplyRename(localHit.path, newName);
        namingAi.setSuggestions([]);
        const newHit: RichSearchHit = {
          ...localHit,
          path: result.new_path,
          name: result.new_path.split("/").pop() ?? null,
        };
        setLocalHit(newHit);
        onSelectionUpdate?.(newHit);
        bumpRefresh();
        toast.success(`Renamed to ${result.new_path.split("/").pop()}`);
      } catch (e) {
        toast.error(e instanceof IpcError ? e.raw : String(e));
      }
    },
    [localHit, onSelectionUpdate, bumpRefresh, namingAi],
  );

  const handleApplyTag = React.useCallback(
    async (tag: string) => {
      try {
        await tagAdd(localHit.file_id, tag);
        tagsAi.setSuggestions((prev) => prev.filter((s) => s.value !== tag));
        setLocalHit((prev) => ({
          ...prev,
          tags: [...prev.tags, tag].toSorted((a, b) => a.localeCompare(b)),
        }));
        bumpRefresh();
        toast.success(`Tag "${tag}" added`);
      } catch (e) {
        toast.error(e instanceof IpcError ? e.raw : String(e));
      }
    },
    [localHit.file_id, bumpRefresh, tagsAi],
  );

  const handleApplyNotes = React.useCallback(
    async (notes: string) => {
      try {
        await notesWrite(localHit.path, notes);
        notesAi.setSuggestions([]);
        setNotesVersion((n) => n + 1);
        bumpRefresh();
        toast.success("Notes updated");
      } catch (e) {
        toast.error(e instanceof IpcError ? e.raw : String(e));
      }
    },
    [localHit.path, bumpRefresh, notesAi],
  );

  return (
    <div className="grid h-full grid-rows-[auto_1fr] overflow-hidden">
      <header className="border-b px-3 py-2">
        <div className="text-[0.625rem] uppercase tracking-wide text-muted-foreground">File</div>
        <div className="break-all font-mono text-xs/relaxed">{localHit.path}</div>
      </header>
      <div className="grid auto-rows-min gap-3 overflow-y-auto px-3 py-3 text-xs">
        <NameSection
          hit={localHit}
          aiConfig={isIndexed ? aiConfig : null}
          ai={namingAi}
          onApplyRename={handleApplyRename}
        />
        <StaticFields hit={localHit} />
        <TagsSection
          hit={localHit}
          disabled={!isIndexed}
          aiConfig={isIndexed ? aiConfig : null}
          ai={tagsAi}
          onApplyTag={handleApplyTag}
        />
        <NotesSection
          hit={localHit}
          disabled={!isIndexed}
          reloadKey={notesVersion}
          aiConfig={isIndexed ? aiConfig : null}
          ai={notesAi}
          onApplyNotes={handleApplyNotes}
        />
        <CustomFieldsBlock hit={localHit} />
        {!isIndexed ? (
          <div className="rounded-md border border-warning/40 bg-warning/10 px-2 py-1.5 text-warning">
            This file isn&apos;t indexed yet — run <code>progest scan</code> (or wait for the next
            reconcile) before editing tags or notes.
          </div>
        ) : null}
        {isIndexed ? <DeleteSection path={localHit.path} onDeleted={onDeleted} /> : null}
      </div>
    </div>
  );
});

// ── Name section ───────────────────────────────────────────────────

function NameSection(props: {
  hit: RichSearchHit;
  aiConfig: AiConfigResponse | null;
  ai: ReturnType<typeof useAiSuggestion>;
  onApplyRename: (newName: string) => void;
}) {
  const name = props.hit.name ?? props.hit.path.split("/").pop() ?? "";
  return (
    <section className="grid gap-1.5">
      <div className="flex items-center justify-between">
        <Label className="text-muted-foreground">Name</Label>
        <AiButton
          aiConfig={props.aiConfig}
          busy={props.ai.busy}
          onGenerate={() => void props.ai.generate()}
        />
      </div>
      <div className="min-w-0 break-words font-mono">{name}</div>
      <AiSuggestionsList
        ai={props.ai}
        renderAction={(s) => (
          <Button size="xs" variant="ghost" onClick={() => void props.onApplyRename(s.value)}>
            Rename
          </Button>
        )}
      />
    </section>
  );
}

// ── Static fields (kind / ext / violations / file_id) ──────────────

function StaticFields(props: { hit: RichSearchHit }) {
  const { hit } = props;
  return (
    <div className="grid gap-1.5">
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

// ── Tags section ───────────────────────────────────────────────────

function TagsSection(props: {
  hit: RichSearchHit;
  disabled: boolean;
  aiConfig: AiConfigResponse | null;
  ai: ReturnType<typeof useAiSuggestion>;
  onApplyTag: (tag: string) => void;
}) {
  const { hit, disabled } = props;
  const { bumpRefresh } = useProject();
  const [tags, setTags] = React.useState<string[]>(hit.tags);
  const [draft, setDraft] = React.useState("");
  const [error, setError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  const pendingAdd = React.useRef<string | null>(null);

  React.useEffect(() => {
    setTags(hit.tags);
    setDraft("");
    setError(null);
    pendingAdd.current = null;
  }, [hit.file_id, hit.path, hit.tags]);

  const submitDraft = async () => {
    const tag = draft.trim();
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
      <div className="flex items-center justify-between">
        <Label className="text-muted-foreground">Tags</Label>
        <AiButton
          aiConfig={props.aiConfig}
          busy={props.ai.busy}
          onGenerate={() => void props.ai.generate()}
        />
      </div>
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
      <AiSuggestionsList
        ai={props.ai}
        renderAction={(s) => (
          <Button size="xs" variant="ghost" onClick={() => void props.onApplyTag(s.value)}>
            Apply
          </Button>
        )}
      />
    </section>
  );
}

// ── Notes section ──────────────────────────────────────────────────

function NotesSection(props: {
  hit: RichSearchHit;
  disabled: boolean;
  reloadKey?: number;
  aiConfig: AiConfigResponse | null;
  ai: ReturnType<typeof useAiSuggestion>;
  onApplyNotes: (notes: string) => void;
}) {
  const { hit, disabled } = props;
  const { bumpRefresh } = useProject();
  const [body, setBody] = React.useState("");
  const persisted = React.useRef("");
  const [loading, setLoading] = React.useState(false);
  const [saving, setSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

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
  }, [hit.path, props.reloadKey]);

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
        <div className="flex items-center gap-1">
          <span className="text-[0.625rem] text-muted-foreground">
            {loading ? "loading…" : saving ? "saving…" : null}
          </span>
          <AiButton
            aiConfig={props.aiConfig}
            busy={props.ai.busy}
            onGenerate={() => void props.ai.generate()}
          />
        </div>
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
      <AiSuggestionsList
        ai={props.ai}
        notesCheckbox
        renderAction={(s) => (
          <Button size="xs" variant="ghost" onClick={() => void props.onApplyNotes(s.value)}>
            Apply
          </Button>
        )}
      />
    </section>
  );
}

// ── Custom fields ──────────────────────────────────────────────────

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

// ── Delete section ─────────────────────────────────────────────────

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
                  <DotmSquare5 size={16} dotSize={2} animated className="mr-1.5" />
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
