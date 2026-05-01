import * as React from "react";
import { useProject } from "@/lib/project-context";
import type { PaletteCommand } from "./types";

export function useFileCommands(): PaletteCommand[] {
  const { project } = useProject();

  return React.useMemo<PaletteCommand[]>(() => {
    if (!project) return [];
    return [
      {
        id: "file.new-file",
        title: "New file…",
        group: "File",
        keywords: ["create", "touch"],
        run: () => {
          window.dispatchEvent(new CustomEvent("progest:create", { detail: { kind: "file" } }));
        },
      },
      {
        id: "file.new-folder",
        title: "New folder…",
        group: "File",
        keywords: ["create", "directory", "mkdir"],
        run: () => {
          window.dispatchEvent(new CustomEvent("progest:create", { detail: { kind: "dir" } }));
        },
      },
    ];
  }, [project]);
}
