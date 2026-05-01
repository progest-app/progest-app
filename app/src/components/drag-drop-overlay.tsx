import * as React from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { Import } from "lucide-react";

type DragDropState = {
  active: boolean;
  paths: string[];
  position: { x: number; y: number } | null;
};

type DragDropContextValue = {
  state: DragDropState;
};

const DragDropCtx = React.createContext<DragDropContextValue>({
  state: { active: false, paths: [], position: null },
});

/**
 * Convert Tauri DragDropEvent position to CSS logical pixels.
 *
 * Tauri types the position as `PhysicalPosition`, but on macOS the
 * coordinates are already in logical pixels (NSEvent reports points).
 * Blindly dividing by devicePixelRatio on Retina displays halves the
 * values, causing the offset to grow with distance from the origin.
 *
 * Heuristic: if either coordinate exceeds the CSS viewport dimensions,
 * the values must be in device pixels (Windows / Linux with scaling) and
 * need conversion. Otherwise they are already logical and are returned
 * as-is.
 */
function toLogical(pos: { x: number; y: number }): { x: number; y: number } {
  const dpr = window.devicePixelRatio || 1;
  if (dpr > 1 && (pos.x > window.innerWidth || pos.y > window.innerHeight)) {
    return { x: pos.x / dpr, y: pos.y / dpr };
  }
  return pos;
}

export function useDropZone(ref: React.RefObject<HTMLElement | null>): {
  isOver: boolean;
  fileCount: number;
} {
  const { state } = React.useContext(DragDropCtx);

  const isOver = React.useMemo(() => {
    if (!state.active || !state.position || !ref.current) return false;
    const rect = ref.current.getBoundingClientRect();
    const pos = state.position;
    return pos.x >= rect.left && pos.x <= rect.right && pos.y >= rect.top && pos.y <= rect.bottom;
  }, [state.active, state.position, ref]);

  return { isOver, fileCount: state.paths.length };
}

export function useDragActive(): DragDropState {
  return React.useContext(DragDropCtx).state;
}

export function DragDropProvider(props: {
  children: React.ReactNode;
  onDrop: (paths: string[], position: { x: number; y: number }) => void;
}) {
  const [state, setState] = React.useState<DragDropState>({
    active: false,
    paths: [],
    position: null,
  });

  const stateRef = React.useRef(state);
  stateRef.current = state;

  const onDropRef = React.useRef(props.onDrop);
  onDropRef.current = props.onDrop;

  React.useEffect(() => {
    let unlisten: (() => void) | undefined;

    const setup = async () => {
      const appWindow = getCurrentWebviewWindow();
      unlisten = await appWindow.onDragDropEvent((event) => {
        const p = event.payload;
        if (p.type === "enter") {
          setState({
            active: true,
            paths: p.paths,
            position: toLogical(p.position),
          });
        } else if (p.type === "over") {
          setState((prev) => ({
            ...prev,
            position: toLogical(p.position),
          }));
        } else if (p.type === "drop") {
          // Fire the callback SYNCHRONOUSLY before setState so that
          // elementFromPoint (called by handleDrop → dirPathAtPoint)
          // runs while the drag-over DOM state is still intact.
          const prev = stateRef.current;
          const paths = prev.paths.length > 0 ? prev.paths : p.paths;
          const dropPos = prev.position ?? toLogical(p.position);
          onDropRef.current(paths, dropPos);
          setState({ active: false, paths: [], position: null });
        } else {
          setState({ active: false, paths: [], position: null });
        }
      });
    };

    void setup();

    return () => {
      unlisten?.();
    };
  }, []);

  const ctxValue = React.useMemo(() => ({ state }), [state]);

  return <DragDropCtx.Provider value={ctxValue}>{props.children}</DragDropCtx.Provider>;
}

export function DropOverlay(props: { isOver: boolean; fileCount: number; label?: string }) {
  if (!props.isOver) return null;
  return (
    <div className="absolute inset-0 z-40 flex items-center justify-center bg-background/60 backdrop-blur-sm rounded-md border-2 border-dashed border-primary/50 transition-all">
      <div className="flex flex-col items-center gap-2">
        <Import className="size-8 text-primary animate-pulse" />
        <div className="text-xs font-medium">
          Drop to import {props.fileCount} file{props.fileCount === 1 ? "" : "s"}
        </div>
        {props.label ? (
          <div className="text-[0.625rem] text-muted-foreground">{props.label}</div>
        ) : null}
      </div>
    </div>
  );
}
