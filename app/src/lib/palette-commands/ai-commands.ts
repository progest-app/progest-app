import * as React from "react";

import type { PaletteCommand } from "./types";

export function useAiCommands(): PaletteCommand[] {
  return React.useMemo<PaletteCommand[]>(
    () => [
      {
        id: "ai.suggest.naming",
        title: "AI: Suggest filename",
        group: "AI",
        keywords: ["ai", "name", "rename", "suggest"],
        hint: "select a file first",
        run: () => {},
      },
      {
        id: "ai.suggest.tags",
        title: "AI: Suggest tags",
        group: "AI",
        keywords: ["ai", "tag", "suggest"],
        hint: "select a file first",
        run: () => {},
      },
      {
        id: "ai.suggest.notes",
        title: "AI: Generate notes",
        group: "AI",
        keywords: ["ai", "notes", "generate", "describe"],
        hint: "select a file first",
        run: () => {},
      },
      {
        id: "ai.suggest.placement",
        title: "AI: Suggest placement",
        group: "AI",
        keywords: ["ai", "move", "placement", "directory"],
        hint: "select a file first",
        run: () => {},
      },
      {
        id: "ai.configure",
        title: "AI: Configure API key",
        group: "AI",
        keywords: ["ai", "key", "config", "setup", "byok", "openai", "anthropic"],
        run: () => {},
      },
    ],
    [],
  );
}
