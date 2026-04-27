import * as React from "react";
import { LayoutGrid, List as ListIcon, Save, Trash2, FileIcon } from "lucide-react";

import {
  IpcError,
  filesListAll,
  searchExecute,
  viewDelete,
  viewList,
  viewSave,
  type RichSearchHit,
  type SearchResponse,
  type View,
  type ViewDisplay,
} from "@/lib/ipc";
import { useProject } from "@/lib/project-context";
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
import { ViolationBadges } from "@/components/violation-badges";

const DEBOUNCE_MS = 200;

export function FlatView(props: { onPickHit?: (hit: RichSearchHit) => void }) {
  const { project } = useProject();
  const [query, setQuery] = React.useState("");
  const [display, setDisplay] = React.useState<ViewDisplay>("list");
  const [response, setResponse] = React.useState<SearchResponse | null>(null);
  const [loading, setLoading] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [views, setViews] = React.useState<View[]>([]);
  const [activeViewId, setActiveViewId] = React.useState<string | null>(null);
  const [saveOpen, setSaveOpen] = React.useState(false);

  const refreshViews = React.useCallback(async () => {
    try {
      setViews(await viewList());
    } catch (e) {
      if (e instanceof IpcError && !e.isNoProject) setError(e.raw);
    }
  }, []);

  // Reset all per-project state when the attached project changes.
  // Without this, switching projects via the picker / recent list
  // would leave the old query, response, saved-views list, and
  // active-view selection in place — the panel would look stale
  // until the user typed something.
  React.useEffect(() => {
    setQuery("");
    setResponse(null);
    setError(null);
    setActiveViewId(null);
    setDisplay("list");
    void refreshViews();
  }, [project?.root, refreshViews]);

  // Debounced search whenever query changes. Empty query falls
  // through to `files_list_all` so the panel always shows *something*
  // — name-sorted full project — instead of an empty placeholder.
  React.useEffect(() => {
    const trimmed = query.trim();
    let cancelled = false;
    if (trimmed.length === 0) {
      setLoading(true);
      filesListAll()
        .then((hits) => {
          if (cancelled) return;
          setResponse({
            query: "",
            hits,
            warnings: [],
            parse_error: null,
          });
          setError(null);
        })
        .catch((e) => {
          if (cancelled) return;
          if (e instanceof IpcError && e.isNoProject) {
            setResponse(null);
          } else {
            setError(e instanceof IpcError ? e.raw : String(e));
            setResponse(null);
          }
        })
        .finally(() => {
          if (!cancelled) setLoading(false);
        });
      return () => {
        cancelled = true;
      };
    }
    setLoading(true);
    const handle = setTimeout(async () => {
      try {
        if (cancelled) return;
        setResponse(await searchExecute(trimmed));
        setError(null);
      } catch (e) {
        if (cancelled) return;
        setError(e instanceof IpcError ? e.raw : String(e));
        setResponse(null);
      } finally {
        if (!cancelled) setLoading(false);
      }
    }, DEBOUNCE_MS);
    return () => {
      cancelled = true;
      clearTimeout(handle);
    };
    // `project?.root` is in the deps so a project switch retriggers
    // the empty-query files_list_all (or the saved view's loaded
    // query) against the new index even when the query string itself
    // is identical between projects.
  }, [query, project?.root]);

  const onSelectView = (id: string) => {
    if (id === "") {
      setActiveViewId(null);
      return;
    }
    const v = views.find((x) => x.id === id);
    if (!v) return;
    setActiveViewId(id);
    setQuery(v.query);
    setDisplay(v.display);
  };

  const onUserEdit = (next: string) => {
    setQuery(next);
    // Editing the query manually decouples from the saved view so a
    // subsequent "Save as view" doesn't silently overwrite a different
    // view by id.
    if (activeViewId !== null) setActiveViewId(null);
  };

  const onDeleteActive = async () => {
    if (!activeViewId) return;
    try {
      await viewDelete(activeViewId);
      setActiveViewId(null);
      await refreshViews();
    } catch (e) {
      setError(e instanceof IpcError ? e.raw : String(e));
    }
  };

  return (
    <section className="flex h-full flex-col">
      <header className="flex flex-wrap items-center gap-2 border-b px-3 py-2">
        <Input
          value={query}
          onChange={(e) => onUserEdit(e.target.value)}
          placeholder="tag:wip type:psd is:violation …"
          className="h-8 max-w-md text-xs"
        />
        <ViewSelect
          views={views}
          active={activeViewId}
          onSelect={onSelectView}
        />
        <DisplayToggle value={display} onChange={setDisplay} />
        <div className="ml-auto flex items-center gap-1">
          {activeViewId ? (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void onDeleteActive()}
              title="Delete current view"
            >
              <Trash2 />
            </Button>
          ) : null}
          <Button
            variant="outline"
            size="sm"
            onClick={() => setSaveOpen(true)}
            disabled={query.trim().length === 0}
          >
            <Save /> Save as view
          </Button>
        </div>
      </header>
      <div className="border-b px-3 py-1 text-[0.625rem]">
        {loading ? <span className="text-muted-foreground">searching…</span> : null}
        {response?.parse_error ? (
          <span className="text-destructive">
            parse error: {response.parse_error.message}
          </span>
        ) : null}
        {response?.warnings && response.warnings.length > 0 ? (
          <span className="text-warning">
            {response.warnings.length} warning
            {response.warnings.length === 1 ? "" : "s"}: {response.warnings.join("; ")}
          </span>
        ) : null}
        {error ? <span className="text-destructive">{error}</span> : null}
        {response && !response.parse_error ? (
          <span className="ml-auto text-muted-foreground">
            {response.hits.length} hit{response.hits.length === 1 ? "" : "s"}
          </span>
        ) : null}
      </div>
      <div className="flex-1 overflow-auto">
        {response && !response.parse_error ? (
          display === "list" ? (
            <HitList hits={response.hits} onPick={props.onPickHit} />
          ) : (
            <HitGrid hits={response.hits} onPick={props.onPickHit} />
          )
        ) : null}
      </div>
      <SaveAsDialog
        open={saveOpen}
        onOpenChange={setSaveOpen}
        query={query}
        display={display}
        existing={views}
        onSaved={async (id) => {
          await refreshViews();
          setActiveViewId(id);
        }}
      />
    </section>
  );
}

function Empty({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-full items-center justify-center text-xs text-muted-foreground">
      {children}
    </div>
  );
}

function ViewSelect(props: {
  views: View[];
  active: string | null;
  onSelect: (id: string) => void;
}) {
  // Native <select> for simplicity; a richer dropdown can land later.
  return (
    <select
      className="h-8 rounded-md border bg-card px-2 text-xs"
      value={props.active ?? ""}
      onChange={(e) => props.onSelect(e.target.value)}
    >
      <option value="">— ad-hoc —</option>
      {props.views.map((v) => (
        <option key={v.id} value={v.id}>
          {v.name} ({v.id})
        </option>
      ))}
    </select>
  );
}

function DisplayToggle(props: { value: ViewDisplay; onChange: (v: ViewDisplay) => void }) {
  return (
    <div className="inline-flex overflow-hidden rounded-md border">
      <button
        type="button"
        className={`flex h-8 items-center px-2 text-xs ${
          props.value === "list" ? "bg-accent" : "hover:bg-accent"
        }`}
        onClick={() => props.onChange("list")}
        title="List"
      >
        <ListIcon className="size-3.5" />
      </button>
      <button
        type="button"
        className={`flex h-8 items-center border-l px-2 text-xs ${
          props.value === "grid" ? "bg-accent" : "hover:bg-accent"
        }`}
        onClick={() => props.onChange("grid")}
        title="Grid"
      >
        <LayoutGrid className="size-3.5" />
      </button>
    </div>
  );
}

function HitList(props: {
  hits: RichSearchHit[];
  onPick: ((hit: RichSearchHit) => void) | undefined;
}) {
  if (props.hits.length === 0) return <Empty>No results.</Empty>;
  return (
    <ul className="divide-y">
      {props.hits.map((hit) => (
        <li key={hit.file_id}>
          <button
            type="button"
            className="flex w-full items-center gap-2 px-3 py-1.5 text-xs hover:bg-accent"
            onClick={() => props.onPick?.(hit)}
          >
            <FileIcon className="size-3.5 opacity-60" />
            <span className="truncate font-mono">{hit.path}</span>
            <ViolationBadges counts={hit.violations} />
            {hit.tags.length > 0 ? (
              <span className="ml-2 truncate text-[0.625rem] text-muted-foreground">
                {hit.tags.map((t) => `#${t}`).join(" ")}
              </span>
            ) : null}
          </button>
        </li>
      ))}
    </ul>
  );
}

function HitGrid(props: {
  hits: RichSearchHit[];
  onPick: ((hit: RichSearchHit) => void) | undefined;
}) {
  if (props.hits.length === 0) return <Empty>No results.</Empty>;
  return (
    <div className="grid gap-2 p-3" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(160px, 1fr))" }}>
      {props.hits.map((hit) => (
        <button
          key={hit.file_id}
          type="button"
          className="flex flex-col gap-1 rounded-md border p-2 text-left hover:bg-accent"
          onClick={() => props.onPick?.(hit)}
        >
          <div className="flex aspect-square items-center justify-center rounded bg-muted/40">
            <FileIcon className="size-8 opacity-50" />
          </div>
          <div className="truncate text-xs font-mono" title={hit.path}>
            {hit.name ?? hit.path.split("/").pop()}
          </div>
          <div className="flex items-center justify-between text-[0.625rem] text-muted-foreground">
            <span>{hit.kind}</span>
            <ViolationBadges counts={hit.violations} />
          </div>
        </button>
      ))}
    </div>
  );
}

function SaveAsDialog(props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  query: string;
  display: ViewDisplay;
  existing: View[];
  onSaved: (id: string) => Promise<void>;
}) {
  const { open, onOpenChange, query, display, existing, onSaved } = props;
  const [id, setId] = React.useState("");
  const [name, setName] = React.useState("");
  const [description, setDescription] = React.useState("");
  const [error, setError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);

  React.useEffect(() => {
    if (open) {
      setId("");
      setName("");
      setDescription("");
      setError(null);
    }
  }, [open]);

  const conflictId = existing.some((v) => v.id === id);
  const idPattern = /^[a-z0-9_-]{1,64}$/;
  const idValid = id.length === 0 || idPattern.test(id);

  const onSubmit = async () => {
    if (!idPattern.test(id)) {
      setError("id must match [a-z0-9_-]{1,64}");
      return;
    }
    setBusy(true);
    try {
      await viewSave({
        id,
        name: name.trim() || id,
        query,
        description: description.trim() || null,
        group_by: null,
        sort: null,
        display,
      });
      await onSaved(id);
      onOpenChange(false);
    } catch (e) {
      setError(e instanceof IpcError ? e.raw : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Save view</DialogTitle>
          <DialogDescription>
            Persist the current query + display mode to{" "}
            <code>.progest/views.toml</code>.
          </DialogDescription>
        </DialogHeader>
        <div className="grid gap-3 text-xs">
          <Field label="id">
            <Input
              value={id}
              onChange={(e) => setId(e.target.value)}
              placeholder="violations-this-week"
              className="text-xs"
              autoFocus
            />
            {!idValid ? (
              <span className="text-destructive">
                id must match [a-z0-9_-]{`{1,64}`}
              </span>
            ) : null}
            {idValid && conflictId ? (
              <span className="text-warning">
                will replace existing view {id}
              </span>
            ) : null}
          </Field>
          <Field label="name">
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Violations this week"
              className="text-xs"
            />
          </Field>
          <Field label="query">
            <code className="rounded bg-muted px-2 py-1">{query || "(empty)"}</code>
          </Field>
          <Field label="display">
            <span className="text-muted-foreground">{display}</span>
          </Field>
          <Field label="description">
            <Input
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="(optional)"
              className="text-xs"
            />
          </Field>
          {error ? <div className="text-destructive">{error}</div> : null}
        </div>
        <DialogFooter>
          <Button variant="outline" size="sm" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            size="sm"
            onClick={() => void onSubmit()}
            disabled={busy || !idValid || id.length === 0}
          >
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function Field(props: { label: string; children: React.ReactNode }) {
  return (
    <label className="grid grid-cols-[6rem_1fr] items-center gap-2">
      <span className="text-muted-foreground">{props.label}</span>
      <div>{props.children}</div>
    </label>
  );
}
