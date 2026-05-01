/**
 * Aggregator entry point for the `>`-prefixed command palette mode.
 *
 * Every command source is a hook returning `PaletteCommand[]`. To
 * add a new category:
 *
 *   1. Write `app/src/lib/palette-commands/<topic>-commands.ts`
 *      exporting `use<Topic>Commands(): PaletteCommand[]`.
 *   2. Import + concat it inside [`usePaletteCommands`] below.
 *   3. Document the new commands in `docs/COMMAND_PALETTE.md`
 *      (the canonical user-facing list).
 *
 * Sources can rely on any context (`useProject`, `useTheme`, …); the
 * aggregator just composes them. Keep each source focused on a
 * single subsystem so the dependency surface stays narrow.
 */

import * as React from "react";

import { useAiCommands } from "./ai-commands";
import { useProjectCommands } from "./project-commands";
import { useThemeCommands } from "./theme-commands";
import type { PaletteCommand } from "./types";

export type { PaletteCommand } from "./types";
export { fuzzyMatch } from "./types";

export function usePaletteCommands(): PaletteCommand[] {
  const project = useProjectCommands();
  const theme = useThemeCommands();
  const ai = useAiCommands();
  return React.useMemo(() => [...project, ...theme, ...ai], [project, theme, ai]);
}

/**
 * Group commands by their `group` field, preserving insertion order
 * within each bucket and across buckets (first appearance wins).
 * Commands without a group fall under the empty-string key, which
 * the renderer treats as "no header".
 */
export function groupCommands(commands: PaletteCommand[]): Map<string, PaletteCommand[]> {
  const out = new Map<string, PaletteCommand[]>();
  for (const cmd of commands) {
    const key = cmd.group ?? "";
    const list = out.get(key);
    if (list) {
      list.push(cmd);
    } else {
      out.set(key, [cmd]);
    }
  }
  return out;
}
