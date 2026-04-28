import * as React from "react";
import { Check, ChevronUp, Plus, Trash2, X } from "lucide-react";

import {
  acceptsRead,
  acceptsWrite,
  IpcError,
  lintRun,
  type AcceptsMode,
  type AcceptsReadResponse,
  type AcceptsToken,
  type AliasEntry,
  type RawAccepts,
} from "@/lib/ipc";
import { useProject } from "@/lib/project-context";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";

const MODES: AcceptsMode[] = ["strict", "warn", "hint", "off"];

export function DirectoryInspector(props: { dir: string }) {
  const { project, bumpRefresh } = useProject();
  const dir = props.dir;
  const [data, setData] = React.useState<AcceptsReadResponse | null>(null);
  const [draft, setDraft] = React.useState<RawAccepts | null>(null);
  const [loading, setLoading] = React.useState(false);
  const [saving, setSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [savedAt, setSavedAt] = React.useState<number | null>(null);
  const [lintNote, setLintNote] = React.useState<string | null>(null);

  const refresh = React.useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await acceptsRead(dir);
      setData(res);
      // Initialize draft from server. Cloning to a fresh object so the
      // chip input mutates a draft copy, not the snapshot we render the
      // "Effective" section from.
      setDraft(res.own ? cloneRaw(res.own) : null);
    } catch (e) {
      const msg = e instanceof IpcError ? e.raw : String(e);
      setError(msg);
      setData(null);
      setDraft(null);
    } finally {
      setLoading(false);
    }
  }, [dir]);

  React.useEffect(() => {
    if (!project) {
      setData(null);
      setDraft(null);
      return;
    }
    void refresh();
  }, [project?.root, dir, refresh]);

  const aliases = data?.aliases ?? [];
  const dirty = !sameRaw(draft, data?.own ?? null);

  async function onSave() {
    setSaving(true);
    setError(null);
    setLintNote(null);
    try {
      await acceptsWrite(dir, draft);
      setSavedAt(Date.now());
      // Recompute placement violations against the new dirmeta. Without
      // this, badges in FlatView / TreeView and the `is:misplaced`
      // search would stay stale until the user shelled out to
      // `progest lint`. Failure here is non-fatal — the accepts edit
      // already landed; surface the error and let the user retry.
      try {
        const stats = await lintRun();
        setLintNote(
          `Re-lint: ${stats.scanned} files · naming ${stats.naming} · placement ${stats.placement} · sequence ${stats.sequence}`,
        );
        bumpRefresh();
      } catch (lintErr) {
        const msg = lintErr instanceof IpcError ? lintErr.raw : String(lintErr);
        setLintNote(`Saved, but re-lint failed: ${msg}`);
      }
      await refresh();
    } catch (e) {
      const msg = e instanceof IpcError ? e.raw : String(e);
      setError(msg);
    } finally {
      setSaving(false);
    }
  }

  function onDeclareEmpty() {
    setDraft({ inherit: false, exts: [], mode: "warn" });
  }

  function onClearAccepts() {
    setDraft(null);
  }

  return (
    <section className="flex h-full flex-col overflow-hidden text-xs">
      <header className="flex items-center gap-2 border-b px-3 py-2">
        <div className="min-w-0 flex-1">
          <div className="text-muted-foreground">Directory</div>
          <div className="truncate font-medium" title={dir || "(root)"}>
            {dir === "" ? "(project root)" : dir}
          </div>
        </div>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => void refresh()}
          disabled={loading || saving}
          title="Reload"
        >
          Reload
        </Button>
      </header>

      <div className="flex-1 overflow-auto">
        {loading && !data ? <div className="p-3 text-muted-foreground">Loading…</div> : null}
        {error ? (
          <div className="m-3 rounded border border-destructive/40 bg-destructive/5 p-2 text-destructive">
            {error}
          </div>
        ) : null}
        {data ? (
          <>
            <EffectiveSection data={data} />
            <OwnSection
              draft={draft}
              setDraft={setDraft}
              aliases={aliases}
              onDeclareEmpty={onDeclareEmpty}
              onClearAccepts={onClearAccepts}
            />
            <ChainSection data={data} />
            {data.warnings.length > 0 ? (
              <div className="border-t px-3 py-2">
                <div className="mb-1 text-muted-foreground">Warnings</div>
                <ul className="grid gap-1 text-warning">
                  {data.warnings.map((w, i) => (
                    <li key={i}>{w}</li>
                  ))}
                </ul>
              </div>
            ) : null}
          </>
        ) : null}
      </div>

      <footer className="flex flex-col gap-1 border-t px-3 py-2">
        <div className="flex items-center gap-2">
          <Button size="sm" onClick={() => void onSave()} disabled={!dirty || saving || loading}>
            <Check />
            {saving ? "Saving…" : "Save"}
          </Button>
          {dirty ? <span className="text-muted-foreground">unsaved changes</span> : null}
          {!dirty && savedAt ? <span className="text-muted-foreground">saved</span> : null}
        </div>
        {lintNote ? (
          <div className="text-[0.625rem] text-muted-foreground" title={lintNote}>
            {lintNote}
          </div>
        ) : null}
      </footer>
    </section>
  );
}

// ----- Effective section ---------------------------------------------------

function EffectiveSection(props: { data: AcceptsReadResponse }) {
  const eff = props.data.effective;
  if (!eff) {
    return (
      <div className="border-b px-3 py-2">
        <div className="mb-1 text-muted-foreground">Effective accepts</div>
        <div className="text-muted-foreground">
          No placement constraint — every file is accepted here.
        </div>
      </div>
    );
  }
  return (
    <div className="border-b px-3 py-2">
      <div className="mb-1 flex items-center justify-between">
        <div className="text-muted-foreground">Effective accepts</div>
        <span
          className={cn(
            "rounded px-1.5 py-0.5 text-[0.625rem] uppercase tracking-wide",
            "bg-muted text-muted-foreground",
          )}
          title={`Severity for placement violations: ${eff.mode}`}
        >
          mode: {eff.mode}
        </span>
      </div>
      {eff.exts.length === 0 ? (
        <div className="text-muted-foreground">
          Empty set — this dir intentionally rejects every file.
        </div>
      ) : (
        <ul className="flex flex-wrap gap-1">
          {eff.exts.map((e) => (
            <li
              key={`${e.source}:${e.ext}`}
              className={cn(
                "rounded border px-1.5 py-0.5 text-[0.625rem]",
                e.source === "own"
                  ? "border-badge-placement/40 bg-badge-placement/10 text-badge-placement"
                  : "bg-muted text-muted-foreground",
              )}
              title={e.source === "own" ? "Own declaration" : "Inherited from ancestor"}
            >
              {e.ext === "" ? "(no extension)" : `.${e.ext}`}
              {e.source === "inherited" ? " ↑" : null}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

// ----- Own [accepts] editor ------------------------------------------------

function OwnSection(props: {
  draft: RawAccepts | null;
  setDraft: (next: RawAccepts | null) => void;
  aliases: AliasEntry[];
  onDeclareEmpty: () => void;
  onClearAccepts: () => void;
}) {
  const { draft, setDraft, aliases, onDeclareEmpty, onClearAccepts } = props;

  if (!draft) {
    return (
      <div className="border-b px-3 py-2">
        <div className="mb-1 text-muted-foreground">Own [accepts]</div>
        <div className="grid gap-2">
          <div className="text-muted-foreground">No declaration on this directory.</div>
          <div>
            <Button variant="outline" size="sm" onClick={onDeclareEmpty}>
              <Plus /> Declare [accepts]
            </Button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="border-b px-3 py-2">
      <div className="mb-2 flex items-center justify-between">
        <div className="text-muted-foreground">Own [accepts]</div>
        <Button
          variant="ghost"
          size="sm"
          onClick={onClearAccepts}
          title="Remove the entire [accepts] block"
        >
          <Trash2 /> Remove
        </Button>
      </div>

      <div className="grid gap-3">
        <ExtsEditor
          tokens={draft.exts}
          aliases={aliases}
          onChange={(next) => setDraft({ ...draft, exts: next })}
        />

        <div className="flex items-center gap-2">
          <Checkbox
            id="accepts-inherit"
            checked={draft.inherit}
            onCheckedChange={(c) => setDraft({ ...draft, inherit: c === true })}
          />
          <Label htmlFor="accepts-inherit" className="text-xs font-normal">
            Inherit from ancestors
            <span className="ml-1 text-muted-foreground">(union with parent chain)</span>
          </Label>
        </div>

        <div className="flex items-center gap-2">
          <Label htmlFor="accepts-mode" className="w-14 shrink-0 text-muted-foreground">
            Mode
          </Label>
          <Select
            value={draft.mode}
            onValueChange={(v) => setDraft({ ...draft, mode: v as AcceptsMode })}
          >
            <SelectTrigger id="accepts-mode" size="sm">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {MODES.map((m) => (
                <SelectItem key={m} value={m}>
                  {m}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>
    </div>
  );
}

// ----- Extensions chip input ----------------------------------------------

function ExtsEditor(props: {
  tokens: AcceptsToken[];
  aliases: AliasEntry[];
  onChange: (next: AcceptsToken[]) => void;
}) {
  const { tokens, aliases, onChange } = props;
  const [input, setInput] = React.useState("");
  const [error, setError] = React.useState<string | null>(null);
  const [aliasMenu, setAliasMenu] = React.useState(false);

  function add(token: AcceptsToken) {
    setError(null);
    if (containsToken(tokens, token)) return;
    onChange([...tokens, token]);
  }

  function remove(idx: number) {
    onChange(tokens.filter((_, i) => i !== idx));
  }

  function commit() {
    const raw = input.trim();
    if (raw === "") return;
    const parsed = parseTokenInput(raw);
    if (!parsed) {
      setError(`Invalid entry "${raw}". Use ".psd", ":image", or "" for no-extension.`);
      return;
    }
    add(parsed);
    setInput("");
  }

  return (
    <div className="grid gap-1">
      <div className="flex items-center justify-between">
        <span className="text-muted-foreground">Extensions</span>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setAliasMenu((v) => !v)}
          title="Add a category alias"
        >
          {aliasMenu ? <ChevronUp /> : <Plus />}
          Alias…
        </Button>
      </div>

      {tokens.length === 0 ? (
        <div className="text-muted-foreground">
          No tokens — empty set means this dir rejects every file.
        </div>
      ) : (
        <ul className="flex flex-wrap gap-1">
          {tokens.map((t, i) => (
            <li
              key={i}
              className="flex items-center gap-1 rounded border px-1.5 py-0.5"
              title={tokenTooltip(t, aliases)}
            >
              <span>{tokenLabel(t)}</span>
              <button
                type="button"
                className="text-muted-foreground hover:text-destructive"
                onClick={() => remove(i)}
                aria-label={`Remove ${tokenLabel(t)}`}
              >
                <X className="size-3" />
              </button>
            </li>
          ))}
        </ul>
      )}

      <div className="flex items-center gap-1">
        <Input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              commit();
            }
          }}
          placeholder=".psd, :image, or empty for no-extension"
          className="h-7 text-xs"
        />
        <Button variant="outline" size="sm" onClick={commit} disabled={input.trim() === ""}>
          Add
        </Button>
      </div>
      {error ? <div className="text-destructive">{error}</div> : null}

      {aliasMenu ? (
        <div className="rounded border bg-popover p-1">
          {aliases.length === 0 ? (
            <div className="px-2 py-1 text-muted-foreground">No aliases registered.</div>
          ) : (
            <ul className="grid max-h-40 gap-0.5 overflow-auto">
              {aliases.map((a) => {
                const already = tokens.some((t) => t.type === "alias" && t.name === a.name);
                return (
                  <li key={a.name}>
                    <button
                      type="button"
                      className="grid w-full grid-cols-[1fr_auto] items-center gap-2 rounded px-2 py-1 text-left hover:bg-accent disabled:opacity-50"
                      disabled={already}
                      onClick={() => add({ type: "alias", name: a.name })}
                    >
                      <span>
                        :{a.name}
                        {a.builtin ? null : (
                          <span className="ml-1 text-[0.625rem] text-muted-foreground">
                            (project)
                          </span>
                        )}
                      </span>
                      <span className="truncate text-[0.625rem] text-muted-foreground">
                        {a.exts.length} ext{a.exts.length === 1 ? "" : "s"}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      ) : null}
    </div>
  );
}

// ----- Chain summary -------------------------------------------------------

function ChainSection(props: { data: AcceptsReadResponse }) {
  const chain = props.data.chain;
  if (chain.length === 0) {
    return (
      <div className="border-b px-3 py-2">
        <div className="mb-1 text-muted-foreground">Inheritance chain</div>
        <div className="text-muted-foreground">No ancestors carry their own [accepts].</div>
      </div>
    );
  }
  return (
    <div className="border-b px-3 py-2">
      <div className="mb-1 text-muted-foreground">
        Inheritance chain ({chain.length} ancestor{chain.length === 1 ? "" : "s"})
      </div>
      <ul className="grid gap-1.5">
        {chain.map((entry) => (
          <li key={entry.dir} className="rounded border bg-muted/30 p-1.5">
            <div className="flex items-center justify-between">
              <span className="font-medium">{entry.dir === "" ? "(root)" : entry.dir}</span>
              <span className="text-[0.625rem] text-muted-foreground">
                {entry.accepts.inherit ? "inherit" : "no-inherit"} · {entry.accepts.mode}
              </span>
            </div>
            <ul className="mt-1 flex flex-wrap gap-1 text-[0.625rem]">
              {entry.accepts.exts.length === 0 ? (
                <li className="text-muted-foreground">(empty)</li>
              ) : (
                entry.accepts.exts.map((t, i) => (
                  <li key={i} className="rounded border px-1 py-0.5">
                    {tokenLabel(t)}
                  </li>
                ))
              )}
            </ul>
          </li>
        ))}
      </ul>
    </div>
  );
}

// ----- helpers -------------------------------------------------------------

function tokenLabel(t: AcceptsToken): string {
  if (t.type === "alias") return `:${t.name}`;
  if (t.value === "") return "(no-ext)";
  return `.${t.value}`;
}

function tokenTooltip(t: AcceptsToken, aliases: AliasEntry[]): string {
  if (t.type === "alias") {
    const found = aliases.find((a) => a.name === t.name);
    if (!found) return `:${t.name} (unknown alias)`;
    return `:${t.name} → ${found.exts.map((x) => `.${x}`).join(", ")}`;
  }
  return tokenLabel(t);
}

function parseTokenInput(raw: string): AcceptsToken | null {
  if (raw === "") return null;
  if (raw.startsWith(":")) {
    const name = raw.slice(1);
    if (!/^[a-z][a-z0-9_-]*$/.test(name) || name.length > 64) return null;
    return { type: "alias", name };
  }
  if (raw.startsWith(".")) {
    const value = raw.slice(1).toLowerCase();
    return { type: "ext", value };
  }
  return null;
}

function containsToken(tokens: AcceptsToken[], next: AcceptsToken): boolean {
  return tokens.some((t) => sameToken(t, next));
}

function sameToken(a: AcceptsToken, b: AcceptsToken): boolean {
  if (a.type !== b.type) return false;
  if (a.type === "alias" && b.type === "alias") return a.name === b.name;
  if (a.type === "ext" && b.type === "ext") return a.value === b.value;
  return false;
}

function cloneRaw(r: RawAccepts): RawAccepts {
  return {
    inherit: r.inherit,
    exts: r.exts.map((t) =>
      t.type === "alias" ? { type: "alias", name: t.name } : { type: "ext", value: t.value },
    ),
    mode: r.mode,
  };
}

function sameRaw(a: RawAccepts | null, b: RawAccepts | null): boolean {
  if (a === null && b === null) return true;
  if (a === null || b === null) return false;
  if (a.inherit !== b.inherit) return false;
  if (a.mode !== b.mode) return false;
  if (a.exts.length !== b.exts.length) return false;
  for (let i = 0; i < a.exts.length; i++) {
    if (!sameToken(a.exts[i]!, b.exts[i]!)) return false;
  }
  return true;
}
