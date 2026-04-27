/**
 * One actionable entry in the `>`-prefixed command palette mode.
 *
 * Sources contribute commands via custom hooks (see `index.ts`); each
 * source is a small `useFooCommands()` that returns
 * `PaletteCommand[]` and only depends on its own context. New
 * categories slot in by:
 *   1. writing a new `use<Foo>Commands()` hook,
 *   2. adding it to `usePaletteCommands()` in `index.ts`,
 *   3. listing the resulting commands in `docs/COMMAND_PALETTE.md`.
 */
export interface PaletteCommand {
  /** Stable identifier (`project.open`, `theme.dark`). Used as the
   *  cmdk `value` so navigation order is deterministic; never shown
   *  to the user. */
  id: string;
  /** The line the user reads in the palette. */
  title: string;
  /** Optional bucket header (`Project`, `Theme`, …). Commands sharing
   *  the same group render under one CommandGroup. */
  group?: string | undefined;
  /** Right-aligned secondary label (`active`, `~/Code/foo`, …). */
  hint?: string | undefined;
  /** Extra fuzzy-match keywords beyond the title. Useful for synonyms
   *  (e.g. `"folder picker"` for the Open Project command). */
  keywords?: string[] | undefined;
  /** Action invoked when the command is selected. Errors should be
   *  caught internally; the palette will not show them. */
  run: () => void | Promise<void>;
}

/**
 * Lightweight AND-match: every whitespace-separated needle must
 * appear (case-insensitive) somewhere in the command's searchable
 * surface (title + id + keywords). Empty needle list matches every
 * command.
 *
 * Kept here instead of leaning on cmdk's built-in filter so the
 * palette can keep `shouldFilter={false}` for the search-mode path
 * — flipping cmdk's filter behavior between modes is fragile.
 */
export function fuzzyMatch(query: string, command: PaletteCommand): boolean {
  const needles = query
    .toLowerCase()
    .split(/\s+/)
    .filter((s) => s.length > 0);
  if (needles.length === 0) return true;
  const haystack = [command.title, command.id, ...(command.keywords ?? [])].join(" ").toLowerCase();
  return needles.every((n) => haystack.includes(n));
}
