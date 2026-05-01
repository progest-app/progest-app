import { Channel, invoke } from "@tauri-apps/api/core";

// Wire types mirror crates/progest-tauri/src/commands.rs.
// Keep field names in sync with the Rust Serialize derives.

export type ProgressEvent = {
  current: number;
  total: number;
  message: string;
};

function makeChannel(onProgress?: (e: ProgressEvent) => void): Channel<ProgressEvent> {
  const ch = new Channel<ProgressEvent>();
  if (onProgress) {
    // eslint-disable-next-line unicorn/prefer-add-event-listener -- Tauri Channel API uses onmessage
    ch.onmessage = onProgress;
  }
  return ch;
}

export type ProjectInfo = {
  root: string;
  name: string;
};

export type AppInfo = {
  project: ProjectInfo | null;
};

export type RichViolationCounts = {
  naming: number;
  placement: number;
  sequence: number;
};

export type RichCustomField = {
  key: string;
  // Tagged union: { type: "text", value: string } | { type: "integer", value: number }
  type: "text" | "integer";
  value: string | number;
};

export type RichSearchHit = {
  file_id: string;
  path: string;
  name: string | null;
  kind: string;
  ext: string | null;
  tags: string[];
  violations: RichViolationCounts;
  custom_fields: RichCustomField[];
};

export type ParseErrorPayload = {
  message: string;
  column: number | null;
};

export type SearchResponse = {
  query: string;
  hits: RichSearchHit[];
  warnings: string[];
  parse_error: ParseErrorPayload | null;
};

export type HistoryEntry = {
  query: string;
  ts: string; // RFC3339
};

export type RecentProject = {
  root: string;
  name: string;
  last_opened: string; // RFC3339
};

export type ViewDisplay = "list" | "grid";

export type View = {
  id: string;
  name: string;
  query: string;
  description?: string | null;
  group_by?: string | null;
  sort?: string | null;
  display: ViewDisplay;
};

export type DirEntryKind = "dir" | "file";

export type FileEntry = {
  file_id: string | null;
  kind: string;
  ext: string | null;
  tags: string[];
  violations: RichViolationCounts;
  custom_fields: RichCustomField[];
};

export type DirEntry = {
  name: string;
  path: string;
  kind: DirEntryKind;
  file?: FileEntry;
};

// IPC errors are plain strings on the JS side. The backend prefixes the
// no-project case with the discriminator `no_project:`; surface that as
// a typed flag so callers can branch without string-matching elsewhere.
export class IpcError extends Error {
  constructor(public readonly raw: string) {
    super(raw);
    this.name = "IpcError";
  }
  get isNoProject(): boolean {
    return this.raw.startsWith("no_project:");
  }
}

function toIpcError(e: unknown): IpcError {
  if (e instanceof IpcError) return e;
  if (typeof e === "string") return new IpcError(e);
  if (e instanceof Error) return new IpcError(e.message);
  return new IpcError(String(e));
}

export async function appInfo(): Promise<AppInfo> {
  try {
    return await invoke<AppInfo>("app_info");
  } catch (e) {
    throw toIpcError(e);
  }
}

export async function searchExecute(query: string): Promise<SearchResponse> {
  try {
    return await invoke<SearchResponse>("search_execute", { query });
  } catch (e) {
    throw toIpcError(e);
  }
}

export async function searchHistoryList(): Promise<HistoryEntry[]> {
  try {
    return await invoke<HistoryEntry[]>("search_history_list");
  } catch (e) {
    throw toIpcError(e);
  }
}

export async function searchHistoryClear(): Promise<void> {
  try {
    await invoke<void>("search_history_clear");
  } catch (e) {
    throw toIpcError(e);
  }
}

export async function searchHistoryRecord(query: string): Promise<void> {
  try {
    await invoke<void>("search_history_record", { query });
  } catch (e) {
    throw toIpcError(e);
  }
}

export async function projectOpen(path: string): Promise<AppInfo> {
  try {
    return await invoke<AppInfo>("project_open", { path });
  } catch (e) {
    throw toIpcError(e);
  }
}

export async function projectRecentList(): Promise<RecentProject[]> {
  try {
    return await invoke<RecentProject[]>("project_recent_list");
  } catch (e) {
    throw toIpcError(e);
  }
}

export async function projectRecentClear(): Promise<void> {
  try {
    await invoke<void>("project_recent_clear");
  } catch (e) {
    throw toIpcError(e);
  }
}

export async function viewList(): Promise<View[]> {
  try {
    return await invoke<View[]>("view_list");
  } catch (e) {
    throw toIpcError(e);
  }
}

export async function viewSave(view: View): Promise<void> {
  try {
    await invoke<void>("view_save", { view });
  } catch (e) {
    throw toIpcError(e);
  }
}

export async function viewDelete(id: string): Promise<void> {
  try {
    await invoke<void>("view_delete", { id });
  } catch (e) {
    throw toIpcError(e);
  }
}

export async function filesListDir(path: string): Promise<DirEntry[]> {
  try {
    return await invoke<DirEntry[]>("files_list_dir", { path });
  } catch (e) {
    throw toIpcError(e);
  }
}

export async function filesListAll(): Promise<RichSearchHit[]> {
  try {
    return await invoke<RichSearchHit[]>("files_list_all");
  } catch (e) {
    throw toIpcError(e);
  }
}

export type ExtensionSummary = {
  ext: string;
  count: number;
};

export async function extensionsCatalog(): Promise<ExtensionSummary[]> {
  try {
    return await invoke<ExtensionSummary[]>("extensions_catalog");
  } catch (e) {
    throw toIpcError(e);
  }
}

// --- project init ---------------------------------------------------------

export type InitPreview = {
  target_path: string;
  target_exists: boolean;
  is_existing_project: boolean;
  predicted_file_count: number | null;
  artifacts: string[];
  gitignore_exists: boolean;
};

export type InitResult = {
  project: ProjectInfo;
  scanned: number;
  added: number;
  orphan_metas: number;
};

export async function projectInitPreview(path: string): Promise<InitPreview> {
  try {
    return await invoke<InitPreview>("project_init_preview", { path });
  } catch (e) {
    throw toIpcError(e);
  }
}

export async function projectInitNew(
  parent: string,
  name: string,
  onProgress?: (e: ProgressEvent) => void,
): Promise<InitResult> {
  try {
    return await invoke<InitResult>("project_init_new", {
      parent,
      name,
      onProgress: makeChannel(onProgress),
    });
  } catch (e) {
    throw toIpcError(e);
  }
}

export async function projectInitExisting(
  path: string,
  name: string | null,
  onProgress?: (e: ProgressEvent) => void,
): Promise<InitResult> {
  try {
    return await invoke<InitResult>("project_init_existing", {
      path,
      name,
      onProgress: makeChannel(onProgress),
    });
  } catch (e) {
    throw toIpcError(e);
  }
}

// IpcError discriminator for "this directory already has a .progest/" — the
// backend prefixes the string so the UI can route to "open" instead of
// blocking the user with a flat error.
export function isAlreadyInitialized(e: unknown): boolean {
  return e instanceof IpcError && e.raw.startsWith("already_initialized:");
}

// --- accepts (directory inspector) ----------------------------------------

// Tagged-union mirror of `AcceptsTokenWire` in
// crates/progest-tauri/src/accepts_commands.rs. The backend serializes
// with `#[serde(tag = "type", rename_all = "lowercase")]`.
export type AcceptsToken = { type: "alias"; name: string } | { type: "ext"; value: string };

export type AcceptsMode = "strict" | "warn" | "hint" | "off";

export type RawAccepts = {
  inherit: boolean;
  exts: AcceptsToken[];
  mode: AcceptsMode;
};

export type EffectiveExt = {
  ext: string;
  source: "own" | "inherited";
};

export type EffectiveAccepts = {
  exts: EffectiveExt[];
  mode: AcceptsMode;
};

export type ChainEntry = {
  dir: string;
  accepts: RawAccepts;
};

export type AliasEntry = {
  name: string;
  exts: string[];
  builtin: boolean;
};

export type AcceptsReadResponse = {
  dir: string;
  own: RawAccepts | null;
  effective: EffectiveAccepts | null;
  chain: ChainEntry[];
  aliases: AliasEntry[];
  warnings: string[];
};

export async function acceptsRead(dir: string): Promise<AcceptsReadResponse> {
  try {
    return await invoke<AcceptsReadResponse>("accepts_read", { dir });
  } catch (e) {
    throw toIpcError(e);
  }
}

export async function acceptsWrite(dir: string, accepts: RawAccepts | null): Promise<void> {
  try {
    await invoke<void>("accepts_write", { dir, accepts });
  } catch (e) {
    throw toIpcError(e);
  }
}

// --- file inspector (tag + notes mutation) -------------------------------

export type NotesReadResponse = {
  path: string;
  body: string;
  sidecar_exists: boolean;
};

export async function tagAdd(file_id: string, tag: string): Promise<void> {
  try {
    await invoke<void>("tag_add", { fileId: file_id, tag });
  } catch (e) {
    throw toIpcError(e);
  }
}

export async function tagRemove(file_id: string, tag: string): Promise<void> {
  try {
    await invoke<void>("tag_remove", { fileId: file_id, tag });
  } catch (e) {
    throw toIpcError(e);
  }
}

export async function notesRead(path: string): Promise<NotesReadResponse> {
  try {
    return await invoke<NotesReadResponse>("notes_read", { path });
  } catch (e) {
    throw toIpcError(e);
  }
}

export async function notesWrite(path: string, body: string): Promise<void> {
  try {
    await invoke<void>("notes_write", { path, body });
  } catch (e) {
    throw toIpcError(e);
  }
}

// --- import ----------------------------------------------------------------

export type SuggestedDestination = {
  path: string;
  score: number;
};

export type ImportRankingResponse = {
  suggestions: SuggestedDestination[];
  all_dirs: string[];
};

export type ImportRequestWire = {
  source: string;
  dest: string;
  mode?: string; // "copy" (default) | "move"
  group_id?: string | null;
};

export type ImportConflict =
  | { kind: "dest_exists"; existing_path: string }
  | { kind: "source_missing"; reason: string }
  | { kind: "source_is_project"; project_path: string }
  | {
      kind: "placement_mismatch";
      expected_exts: string[];
      suggestion: string | null;
    };

export type ImportOp = {
  source: string;
  dest: string;
  mode: string;
  group_id: string | null;
  conflicts: ImportConflict[];
};

export type ImportPreview = {
  ops: ImportOp[];
  clean: boolean;
  conflict_count: number;
};

export type ImportedFile = {
  source: string;
  dest: string;
  file_id: string;
  mode: string;
};

export type ImportWarning = {
  kind: string;
  dest: string;
  message: string;
};

export type ImportOutcome = {
  batch_id: string;
  group_id: string | null;
  imported: ImportedFile[];
  warnings: ImportWarning[];
};

export async function importRanking(sources: string[]): Promise<ImportRankingResponse> {
  try {
    return await invoke<ImportRankingResponse>("import_ranking", { sources });
  } catch (e) {
    throw toIpcError(e);
  }
}

export async function importPreview(requests: ImportRequestWire[]): Promise<ImportPreview> {
  try {
    return await invoke<ImportPreview>("import_preview", { requests });
  } catch (e) {
    throw toIpcError(e);
  }
}

export async function importApply(
  requests: ImportRequestWire[],
  onProgress?: (e: ProgressEvent) => void,
): Promise<ImportOutcome> {
  try {
    return await invoke<ImportOutcome>("import_apply", {
      requests,
      onProgress: makeChannel(onProgress),
    });
  } catch (e) {
    throw toIpcError(e);
  }
}

// --- thumbnail -------------------------------------------------------------

export type ThumbnailPathsResponse = {
  urls: Record<string, string>;
};

export async function thumbnailPaths(fileIds: string[]): Promise<ThumbnailPathsResponse> {
  try {
    return await invoke<ThumbnailPathsResponse>("thumbnail_paths", {
      fileIds,
    });
  } catch (e) {
    throw toIpcError(e);
  }
}

// --- file delete -----------------------------------------------------------

export type DeletePreview = {
  path: string;
  file_id: string;
  has_sidecar: boolean;
};

export type DeleteOutcome = {
  path: string;
  file_id: string;
  sidecar_trashed: boolean;
};

export async function fileDeletePreview(path: string): Promise<DeletePreview> {
  try {
    return await invoke<DeletePreview>("file_delete_preview", { path });
  } catch (e) {
    throw toIpcError(e);
  }
}

export async function fileDeleteApply(path: string): Promise<DeleteOutcome> {
  try {
    return await invoke<DeleteOutcome>("file_delete_apply", { path });
  } catch (e) {
    throw toIpcError(e);
  }
}

// --- lint refresh ----------------------------------------------------------

export type LintRunResponse = {
  naming: number;
  placement: number;
  sequence: number;
  scanned: number;
};

export async function lintRun(onProgress?: (e: ProgressEvent) => void): Promise<LintRunResponse> {
  try {
    return await invoke<LintRunResponse>("lint_run", {
      onProgress: makeChannel(onProgress),
    });
  } catch (e) {
    throw toIpcError(e);
  }
}
