import * as React from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { FolderOpen, FolderTree, LayoutList, Search, SlidersHorizontal } from "lucide-react";

import { ThemeToggle } from "@/components/theme-toggle";
import { Button } from "@/components/ui/button";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useProject } from "@/lib/project-context";
import { cn } from "@/lib/utils";

export type PanelKey = "tree" | "flat" | "inspector";
export type PanelVisibility = Record<PanelKey, boolean>;

export const ALL_PANELS_VISIBLE: PanelVisibility = {
  tree: true,
  flat: true,
  inspector: true,
};

/**
 * Top-of-window titlebar. Sits *under* the macOS traffic lights via
 * `titleBarStyle: "Overlay"` (see `tauri.conf.json`), so the leftmost
 * 80 px is reserved padding for the close/minimize/zoom buttons. The
 * whole row is a `data-tauri-drag-region` so users can drag the
 * window from any empty area; child buttons are interactive because
 * their `pointerdown` is consumed before the drag region kicks in.
 */
export function TitleBar(props: {
  panels: PanelVisibility;
  onTogglePanel: (key: PanelKey) => void;
}) {
  const { project, openPicker } = useProject();
  const { panels, onTogglePanel } = props;

  const visibleKeys = React.useMemo(
    () => (Object.keys(panels) as PanelKey[]).filter((k) => panels[k]),
    [panels],
  );

  // Programmatic drag fallback. `data-tauri-drag-region` is supposed
  // to do this automatically, but the attribute hasn't worked
  // reliably for us in Tauri 2.10 + macOS — particularly inside
  // nested flex containers. Wiring `startDragging()` ourselves is
  // deterministic: walk up from the click target, and if we hit an
  // interactive element first (button / input / role=button / a /
  // [data-no-drag]) we bail; otherwise we start the window drag.
  const onMouseDown = React.useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    if (!(e.target instanceof HTMLElement)) return;
    if (e.target.closest("button, input, select, textarea, a, [role='button'], [data-no-drag]"))
      return;
    if (e.detail === 2) {
      // macOS double-click on titlebar zooms the window per system pref.
      void getCurrentWindow().toggleMaximize();
      return;
    }
    void getCurrentWindow().startDragging();
  }, []);

  return (
    <div
      data-tauri-drag-region
      onMouseDown={onMouseDown}
      className={cn(
        "grid h-10 select-none items-center gap-2 border-b bg-background px-2",
        // 3-column grid keeps the search bar visually centered even when
        // the left cluster (project name) varies in width. The center
        // column is `auto` (sized by the search button); the side
        // columns are equal-width via `1fr` so the search stays put.
        "grid-cols-[1fr_auto_1fr]",
        // Reserve room for macOS traffic lights. Other platforms get a
        // benign extra 80 px gutter — fine for v1.0 (macOS-first); a
        // platform-aware variant lands when Windows ships in v1.1.
        "pl-[80px]",
      )}
    >
      {/* Left cluster — project chip + separator + panel toggles.
          `data-tauri-drag-region` cascade is per-element: only divs
          that explicitly carry the attribute become drag regions.
          So every gap inside the titlebar that should be draggable
          needs its own marker (otherwise gaps inside flex containers
          would be dead zones). */}
      <div data-tauri-drag-region className="flex min-w-0 items-center gap-2">
        <Button
          variant="ghost"
          size="sm"
          onClick={() => void openPicker()}
          title={project ? `Open another project (current: ${project.name})` : "Open project"}
          className="-my-1 h-7 max-w-[40%] gap-1.5"
        >
          <FolderOpen className="size-3.5 shrink-0" />
          <span className="truncate text-xs font-medium">
            {project ? project.name : "Open project…"}
          </span>
        </Button>

        <Separator />

        {/* Panel visibility toggles. ToggleGroup `multiple` so each
            panel flips independently; the value array is derived from
            the panels prop so the UI is always in sync with shell
            state. */}
        <ToggleGroup
          type="multiple"
          size="sm"
          variant="outline"
          value={visibleKeys}
          onValueChange={() => {
            // We never actually trust ToggleGroup's array — the parent
            // owns visibility state. Instead, individual
            // ToggleGroupItem clicks dispatch through `onTogglePanel`.
          }}
        >
          {/* Icons reflect panel content rather than position so the
              affordance is readable without remembering which side
              each pane lives on:
                Tree     → FolderTree (directory hierarchy)
                Flat     → LayoutList (search results / file list)
                Inspector → SlidersHorizontal (accepts editor / config) */}
          <ToggleGroupItem
            value="tree"
            title="Toggle tree pane"
            aria-label="Toggle tree pane"
            onClick={() => onTogglePanel("tree")}
          >
            <FolderTree className="size-3.5" />
          </ToggleGroupItem>
          <ToggleGroupItem
            value="flat"
            title="Toggle flat / search pane"
            aria-label="Toggle flat pane"
            onClick={() => onTogglePanel("flat")}
          >
            <LayoutList className="size-3.5" />
          </ToggleGroupItem>
          <ToggleGroupItem
            value="inspector"
            title="Toggle directory inspector"
            aria-label="Toggle inspector pane"
            onClick={() => onTogglePanel("inspector")}
          >
            <SlidersHorizontal className="size-3.5" />
          </ToggleGroupItem>
        </ToggleGroup>
      </div>

      {/* Center — command palette opener. Click dispatches the same
          toggle event that Cmd+K fires. */}
      <Button
        variant="outline"
        size="sm"
        onClick={() => window.dispatchEvent(new CustomEvent("progest:toggle-palette"))}
        className="h-7 w-64 justify-start gap-2 text-muted-foreground"
        title="Open command palette (⌘K)"
      >
        <Search className="size-3.5" />
        <span className="flex-1 text-left text-xs">Search…</span>
        <kbd className="rounded bg-muted px-1.5 py-0.5 text-[0.625rem] text-muted-foreground">
          ⌘K
        </kbd>
      </Button>

      {/* Right cluster — theme toggle. `justify-end` keeps the icon
          flush against the window edge regardless of the left cluster
          width. Also a drag region so the empty area between the
          search bar and the toggle is grabbable. */}
      <div data-tauri-drag-region className="flex items-center justify-end gap-1">
        <ThemeToggle />
      </div>
    </div>
  );
}

function Separator() {
  return <div className="h-4 w-px bg-border" aria-hidden />;
}
