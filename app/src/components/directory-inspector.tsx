import * as React from "react";
import { Command as CommandPrimitive } from "cmdk";
import { Check, Plus, Trash2, X } from "lucide-react";

import {
  acceptsRead,
  acceptsWrite,
  extensionsCatalog,
  IpcError,
  lintRun,
  type AcceptsMode,
  type AcceptsReadResponse,
  type AcceptsToken,
  type AliasEntry,
  type ExtensionSummary,
  type RawAccepts,
} from "@/lib/ipc";
import { useProject } from "@/lib/project-context";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandItem,
  CommandList,
  CommandShortcut,
} from "@/components/ui/command";
import { Label } from "@/components/ui/label";
import { Popover, PopoverAnchor, PopoverContent } from "@/components/ui/popover";
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
  const [extensions, setExtensions] = React.useState<ExtensionSummary[]>([]);
  const [loading, setLoading] = React.useState(false);
  const [saving, setSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [savedAt, setSavedAt] = React.useState<number | null>(null);
  const [lintNote, setLintNote] = React.useState<string | null>(null);

  const refresh = React.useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [res, exts] = await Promise.all([acceptsRead(dir), extensionsCatalog()]);
      setData(res);
      setExtensions(exts);
      // Initialize draft from server. Cloning to a fresh object so the
      // chip input mutates a draft copy, not the snapshot we render the
      // "Effective" section from.
      setDraft(res.own ? cloneRaw(res.own) : null);
    } catch (e) {
      const msg = e instanceof IpcError ? e.raw : String(e);
      setError(msg);
      setData(null);
      setDraft(null);
      setExtensions([]);
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
        const stats = await lintRun((e) => {
          if (e.total > 0) {
            setLintNote(`Checking ${e.current}/${e.total} files\u{2026}`);
          }
        });
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
          <div className="select-text truncate font-medium" title={dir || "(root)"}>
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
          <div className="m-3 select-text rounded border border-destructive/40 bg-destructive/5 p-2 text-destructive">
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
              extensions={extensions}
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
  extensions: ExtensionSummary[];
  onDeclareEmpty: () => void;
  onClearAccepts: () => void;
}) {
  const { draft, setDraft, aliases, extensions, onDeclareEmpty, onClearAccepts } = props;

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
          extensions={extensions}
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
  extensions: ExtensionSummary[];
  onChange: (next: AcceptsToken[]) => void;
}) {
  const { tokens, aliases, extensions, onChange } = props;
  const [input, setInput] = React.useState("");
  const [open, setOpen] = React.useState(false);
  const inputRef = React.useRef<HTMLInputElement>(null);

  function add(token: AcceptsToken) {
    if (containsToken(tokens, token)) return;
    onChange([...tokens, token]);
    setInput("");
  }

  function remove(idx: number) {
    onChange(tokens.filter((_, i) => i !== idx));
  }

  // Project extensions that aren't already a token. The dedicated
  // no-extension item below covers `value: ""`, so we filter empties
  // out here too.
  const projectExtSuggestions = extensions
    .filter((e) => e.ext !== "")
    .filter((e) => !containsToken(tokens, { type: "ext", value: e.ext }));

  const trimmed = input.trim();
  const typedToken = parseTokenInput(trimmed);
  const hasNoExtToken = containsToken(tokens, { type: "ext", value: "" });
  // Only show "Add new" when the typed entry parses and isn't already
  // discoverable via the suggestion groups (otherwise it duplicates).
  const showCustom =
    typedToken !== null &&
    !containsToken(tokens, typedToken) &&
    !(typedToken.type === "alias" && aliases.some((a) => a.name === typedToken.name)) &&
    !(
      typedToken.type === "ext" &&
      typedToken.value !== "" &&
      projectExtSuggestions.some((e) => e.ext === typedToken.value)
    );

  return (
    <div className="grid gap-1">
      <span className="text-muted-foreground">Extensions</span>

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

      <Popover open={open} onOpenChange={setOpen}>
        <Command
          shouldFilter
          className="overflow-visible bg-transparent p-0"
          // Without this, cmdk swallows Escape — but Radix Popover
          // already handles Escape, and we want it to close the
          // dropdown rather than no-op.
          onKeyDown={(e) => {
            if (e.key === "Escape") setOpen(false);
          }}
        >
          <PopoverAnchor asChild>
            <CommandPrimitive.Input
              ref={inputRef}
              value={input}
              onValueChange={setInput}
              onFocus={() => setOpen(true)}
              onMouseDown={() => setOpen(true)}
              placeholder=".psd, :image, or pick from below"
              className={cn(
                "flex h-7 w-full rounded-md border border-input bg-transparent px-2 text-xs",
                "placeholder:text-muted-foreground focus-visible:outline-hidden",
                "focus-visible:ring-1 focus-visible:ring-ring",
              )}
            />
          </PopoverAnchor>
          <PopoverContent
            align="start"
            sideOffset={4}
            className="w-[var(--radix-popover-trigger-width)] min-w-64 p-0"
            // Keep focus on the input; the popover content is just a
            // dropdown surface.
            onOpenAutoFocus={(e) => e.preventDefault()}
            // Without this Radix treats clicks/focus on the anchored
            // input as "outside" the popover and closes it. The dropdown
            // is conceptually attached to the input, so suppress close
            // when the interaction target is the input itself. Covers
            // both pointerdown (mouse click) and focus (tab into input)
            // via the unified onInteractOutside hook.
            onInteractOutside={(e) => {
              const target = e.target;
              if (target instanceof Node && inputRef.current && inputRef.current.contains(target)) {
                e.preventDefault();
              }
            }}
          >
            <CommandList className="max-h-64">
              <CommandEmpty>No matching alias or extension.</CommandEmpty>

              {showCustom && typedToken ? (
                <CommandGroup heading="New">
                  <CommandItem
                    value={`__custom__:${trimmed}`}
                    keywords={[trimmed]}
                    onSelect={() => add(typedToken)}
                  >
                    <Plus />
                    <span>
                      Add <span className="font-medium">{tokenLabel(typedToken)}</span>
                    </span>
                  </CommandItem>
                </CommandGroup>
              ) : null}

              <CommandGroup heading="Special">
                <CommandItem
                  value="(no extension) noext nothing none"
                  disabled={hasNoExtToken}
                  onSelect={() => add({ type: "ext", value: "" })}
                >
                  <span>(no extension)</span>
                  <CommandShortcut>files without an extension</CommandShortcut>
                </CommandItem>
              </CommandGroup>

              {aliases.length > 0 ? (
                <CommandGroup heading="Aliases">
                  {aliases.map((a) => {
                    const already = tokens.some((t) => t.type === "alias" && t.name === a.name);
                    return (
                      <CommandItem
                        key={`alias-${a.name}`}
                        value={`:${a.name}`}
                        disabled={already}
                        onSelect={() => add({ type: "alias", name: a.name })}
                      >
                        <span>
                          :{a.name}
                          {a.builtin ? null : (
                            <span className="ml-1 text-[0.625rem] text-muted-foreground">
                              (project)
                            </span>
                          )}
                        </span>
                        <CommandShortcut>
                          {a.exts.length} ext{a.exts.length === 1 ? "" : "s"}
                        </CommandShortcut>
                      </CommandItem>
                    );
                  })}
                </CommandGroup>
              ) : null}

              {projectExtSuggestions.length > 0 ? (
                <CommandGroup heading="In project">
                  {projectExtSuggestions.map((e) => (
                    <CommandItem
                      key={`ext-${e.ext}`}
                      value={`.${e.ext}`}
                      onSelect={() => add({ type: "ext", value: e.ext })}
                    >
                      <span>.{e.ext}</span>
                      <CommandShortcut>{e.count}</CommandShortcut>
                    </CommandItem>
                  ))}
                </CommandGroup>
              ) : null}
            </CommandList>
          </PopoverContent>
        </Command>
      </Popover>
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
