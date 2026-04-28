import * as React from "react";
import { ChevronRight, FolderOpen, History, X } from "lucide-react";

import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
  CommandShortcut,
} from "@/components/ui/command";
import {
  IpcError,
  searchExecute,
  searchHistoryClear,
  searchHistoryList,
  type HistoryEntry,
  type RichSearchHit,
  type SearchResponse,
} from "@/lib/ipc";
import { useProject } from "@/lib/project-context";
import {
  fuzzyMatch,
  groupCommands,
  usePaletteCommands,
  type PaletteCommand,
} from "@/lib/palette-commands";
import { ResultDetailDialog } from "@/components/result-detail-dialog";
import { ViolationBadges } from "@/components/violation-badges";

/** Prefix that flips the palette into "system commands" mode. */
const COMMAND_PREFIX = ">";

const SEARCH_DEBOUNCE_MS = 200;

export function CommandPalette(props: { onPickHit?: (hit: RichSearchHit) => void }) {
  const { project, recent, openPicker, pickRecent, clearRecent } = useProject();
  const [open, setOpen] = React.useState(false);
  const [query, setQuery] = React.useState("");
  const [response, setResponse] = React.useState<SearchResponse | null>(null);
  const [loading, setLoading] = React.useState(false);
  const [history, setHistory] = React.useState<HistoryEntry[]>([]);
  const [error, setError] = React.useState<string | null>(null);
  const [selected, setSelected] = React.useState<RichSearchHit | null>(null);

  const allCommands = usePaletteCommands();
  const isCommandMode = query.startsWith(COMMAND_PREFIX);
  const commandQuery = isCommandMode ? query.slice(COMMAND_PREFIX.length).trim() : "";
  const filteredCommands = React.useMemo(
    () => allCommands.filter((c) => fuzzyMatch(commandQuery, c)),
    [allCommands, commandQuery],
  );

  const onRunCommand = React.useCallback((cmd: PaletteCommand) => {
    setOpen(false);
    setQuery("");
    // Defer to next tick so the dialog close animation isn't competing
    // with whatever the command does (open another dialog, swap project,
    // toggle theme).
    queueMicrotask(() => {
      void cmd.run();
    });
  }, []);

  // Cmd+K / Ctrl+K toggle, and a programmatic toggle event so the
  // titlebar button (and any other shell affordance) can open the
  // palette without synthesizing keyboard events.
  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key.toLowerCase() === "k" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setOpen((v) => !v);
      }
    };
    const onToggle = () => setOpen((v) => !v);
    window.addEventListener("keydown", onKey);
    window.addEventListener("progest:toggle-palette", onToggle);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("progest:toggle-palette", onToggle);
    };
  }, []);

  const refreshHistory = React.useCallback(async () => {
    try {
      setHistory(await searchHistoryList());
    } catch (e) {
      if (e instanceof IpcError && !e.isNoProject) {
        setError(e.raw);
      }
    }
  }, []);

  // Reload history every time the palette opens so it always reflects
  // the most-recent submissions. Cheap (atomic read of a small JSON).
  React.useEffect(() => {
    if (!open) return;
    void refreshHistory();
  }, [open, refreshHistory]);

  // Reset palette query state when the attached project changes so a
  // stale ad-hoc query doesn't carry over to a different project.
  React.useEffect(() => {
    setQuery("");
    setResponse(null);
  }, [project?.root]);

  // Debounced search. Skipped when the user is in command mode
  // (`>`-prefixed) — the query would otherwise hit search_execute as
  // a literal `>foo` and confuse the parser.
  React.useEffect(() => {
    const trimmed = query.trim();
    if (!open || isCommandMode || trimmed.length === 0) {
      setResponse(null);
      setLoading(false);
      return;
    }
    setLoading(true);
    const handle = setTimeout(async () => {
      try {
        const res = await searchExecute(trimmed);
        setResponse(res);
        setError(null);
      } catch (e) {
        const msg = e instanceof IpcError ? e.raw : String(e);
        setError(msg);
        setResponse(null);
      } finally {
        setLoading(false);
      }
    }, SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(handle);
  }, [query, open, isCommandMode]);

  const onPickHit = (hit: RichSearchHit) => {
    setOpen(false);
    if (props.onPickHit) {
      props.onPickHit(hit);
    } else {
      setSelected(hit);
    }
  };

  const onClearHistory = async () => {
    try {
      await searchHistoryClear();
      setHistory([]);
    } catch (e) {
      const msg = e instanceof IpcError ? e.raw : String(e);
      setError(msg);
    }
  };

  return (
    <>
      <CommandDialog
        open={open}
        onOpenChange={setOpen}
        title="Search"
        description="Find files by tag, type, name, or arbitrary DSL query."
        // Override the shadcn `sm:max-w-sm` default — palette hits and
        // long DSL queries need more horizontal room than a confirm
        // dialog. `2xl` ≈ 672 px matches the Linear / Raycast feel.
        className="sm:max-w-2xl"
      >
        <Command shouldFilter={false}>
          <CommandInput
            value={query}
            onValueChange={setQuery}
            placeholder={
              isCommandMode
                ? "Type a command (Open project, Set theme, …)"
                : project
                  ? "tag:wip type:psd is:violation …  (>command for actions)"
                  : "Open a project to search, or type > for commands"
            }
            autoFocus
          />
          <CommandList>
            {isCommandMode ? (
              <CommandsBody
                commands={filteredCommands}
                emptyHint={commandQuery}
                onPick={onRunCommand}
              />
            ) : project === null ? (
              <NoProjectBody
                recent={recent}
                onOpenPicker={() => {
                  setOpen(false);
                  void openPicker();
                }}
                onPickRecent={(entry) => {
                  setOpen(false);
                  void pickRecent(entry);
                }}
                onClearRecent={() => void clearRecent()}
              />
            ) : query.trim().length === 0 ? (
              history.length > 0 ? (
                <CommandGroup heading="Recent">
                  {history.map((entry) => (
                    <CommandItem
                      key={entry.query}
                      value={entry.query}
                      onSelect={() => setQuery(entry.query)}
                    >
                      <History className="opacity-60" />
                      <span className="truncate">{entry.query}</span>
                      <CommandShortcut>{relTime(entry.ts)}</CommandShortcut>
                    </CommandItem>
                  ))}
                  <CommandSeparator />
                  <CommandItem value="__clear" onSelect={onClearHistory}>
                    <span className="text-muted-foreground">Clear recent queries</span>
                  </CommandItem>
                </CommandGroup>
              ) : (
                <CommandEmpty>
                  Start typing a query. e.g. <code>tag:wip</code>, <code>type:psd</code>,{" "}
                  <code>is:misplaced</code>.
                </CommandEmpty>
              )
            ) : (
              <SearchBody response={response} loading={loading} onPick={onPickHit} />
            )}
          </CommandList>
        </Command>
        <PaletteStatus
          response={response}
          error={error}
          loading={loading}
          projectName={project?.name ?? null}
        />
      </CommandDialog>
      <ResultDetailDialog
        hit={selected}
        open={selected !== null}
        onOpenChange={(o) => {
          if (!o) setSelected(null);
        }}
      />
    </>
  );
}

function NoProjectBody(props: {
  recent: { root: string; name: string; last_opened: string }[];
  onOpenPicker: () => void;
  onPickRecent: (entry: { root: string; name: string; last_opened: string }) => void;
  onClearRecent: () => void;
}) {
  const { recent, onOpenPicker, onPickRecent, onClearRecent } = props;
  return (
    <>
      <CommandGroup heading="Project">
        <CommandItem value="__open" onSelect={onOpenPicker}>
          <FolderOpen className="opacity-60" />
          <span>Open project…</span>
          <CommandShortcut>folder picker</CommandShortcut>
        </CommandItem>
      </CommandGroup>
      {recent.length > 0 ? (
        <>
          <CommandSeparator />
          <CommandGroup heading="Recent projects">
            {recent.map((entry) => (
              <CommandItem key={entry.root} value={entry.root} onSelect={() => onPickRecent(entry)}>
                <FolderOpen className="opacity-60" />
                <div className="flex min-w-0 flex-col">
                  <span className="truncate">{entry.name || entry.root}</span>
                  {entry.name ? (
                    <span className="truncate text-[0.625rem] text-muted-foreground">
                      {entry.root}
                    </span>
                  ) : null}
                </div>
                <CommandShortcut>{relTime(entry.last_opened)}</CommandShortcut>
              </CommandItem>
            ))}
            <CommandSeparator />
            <CommandItem value="__clear-recent" onSelect={onClearRecent}>
              <X className="opacity-60" />
              <span className="text-muted-foreground">Clear recent projects</span>
            </CommandItem>
          </CommandGroup>
        </>
      ) : (
        <CommandEmpty>
          No project attached. Pick a folder containing <code>.progest/</code> to get started.
        </CommandEmpty>
      )}
    </>
  );
}

/**
 * Render the `>`-prefixed command-mode body. Commands are bucketed
 * by their `group` field (Project / Recent projects / Theme / …);
 * cmdk handles arrow-key navigation across groups for free.
 */
function CommandsBody(props: {
  commands: PaletteCommand[];
  emptyHint: string;
  onPick: (cmd: PaletteCommand) => void;
}) {
  const { commands, emptyHint, onPick } = props;
  if (commands.length === 0) {
    return (
      <CommandEmpty>
        {emptyHint.length > 0 ? `No commands matching "${emptyHint}".` : "No commands available."}
      </CommandEmpty>
    );
  }
  const groups = groupCommands(commands);
  return (
    <>
      {Array.from(groups.entries()).map(([group, items], idx) => (
        <React.Fragment key={group || "(default)"}>
          {idx > 0 ? <CommandSeparator /> : null}
          <CommandGroup heading={group || undefined}>
            {items.map((cmd) => (
              <CommandItem key={cmd.id} value={cmd.id} onSelect={() => onPick(cmd)}>
                <ChevronRight className="opacity-60" />
                {/* min-w-0 lets the truncate kick in inside cmdk's
                    flex row; without it the span ignores width and
                    pushes the shortcut off-screen / onto a new line. */}
                <span className="min-w-0 truncate">{cmd.title}</span>
                {cmd.hint ? (
                  <CommandShortcut className="min-w-0 max-w-[55%] truncate" title={cmd.hint}>
                    {cmd.hint}
                  </CommandShortcut>
                ) : null}
              </CommandItem>
            ))}
          </CommandGroup>
        </React.Fragment>
      ))}
    </>
  );
}

function SearchBody(props: {
  response: SearchResponse | null;
  loading: boolean;
  onPick: (hit: RichSearchHit) => void;
}) {
  const { response, loading, onPick } = props;
  if (loading && !response) {
    return <CommandEmpty>Searching…</CommandEmpty>;
  }
  if (!response) return <CommandEmpty>Type to search.</CommandEmpty>;
  if (response.parse_error) {
    return (
      <CommandEmpty>
        <div className="text-destructive">Parse error: {response.parse_error.message}</div>
      </CommandEmpty>
    );
  }
  if (response.hits.length === 0) {
    return <CommandEmpty>No results.</CommandEmpty>;
  }
  return (
    <CommandGroup heading={`${response.hits.length} hit${response.hits.length === 1 ? "" : "s"}`}>
      {response.hits.map((hit) => (
        <CommandItem key={hit.file_id} value={hit.file_id} onSelect={() => onPick(hit)}>
          <span className="truncate font-mono">{hit.path}</span>
          <ViolationBadges counts={hit.violations} />
          {hit.tags.length > 0 ? (
            <CommandShortcut>
              <span className="opacity-70">{hit.tags.map((t) => `#${t}`).join(" ")}</span>
            </CommandShortcut>
          ) : null}
        </CommandItem>
      ))}
    </CommandGroup>
  );
}

function PaletteStatus(props: {
  response: SearchResponse | null;
  error: string | null;
  loading: boolean;
  projectName: string | null;
}) {
  const { response, error, loading, projectName } = props;
  const lines: React.ReactNode[] = [];
  if (projectName) {
    lines.push(
      <span key="proj" className="text-muted-foreground">
        {projectName}
      </span>,
    );
  }
  if (loading) {
    lines.push(
      <span key="loading" className="text-muted-foreground">
        searching…
      </span>,
    );
  }
  if (response?.warnings.length) {
    lines.push(
      <span key="warn" className="text-warning">
        {response.warnings.length} warning{response.warnings.length === 1 ? "" : "s"}:{" "}
        {response.warnings.join("; ")}
      </span>,
    );
  }
  if (error) {
    lines.push(
      <span key="err" className="text-destructive">
        {error}
      </span>,
    );
  }
  if (lines.length === 0) return null;
  return (
    <div className="flex items-center gap-3 border-t px-3 py-1.5 text-[0.625rem]">{lines}</div>
  );
}

function relTime(rfc3339: string): string {
  const t = Date.parse(rfc3339);
  if (Number.isNaN(t)) return "";
  const diffMs = Date.now() - t;
  const sec = Math.max(0, Math.floor(diffMs / 1000));
  if (sec < 60) return `${sec}s ago`;
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const day = Math.floor(hr / 24);
  return `${day}d ago`;
}
