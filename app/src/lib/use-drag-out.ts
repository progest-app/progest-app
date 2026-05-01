import * as React from "react";
import { resolveResource } from "@tauri-apps/api/path";
import { startDrag } from "@crabnebula/tauri-plugin-drag";

import { fsAbsPath } from "@/lib/ipc";

const DRAG_THRESHOLD = 5;

let dragOutActive = false;
export function isDragOutActive(): boolean {
  return dragOutActive;
}

let dragIconPath: string | null = null;
async function getDragIcon(): Promise<string> {
  if (dragIconPath) return dragIconPath;
  dragIconPath = await resolveResource("resources/drag-icon.png");
  return dragIconPath;
}

export function useDragOut(path: string) {
  const down = React.useRef<{ x: number; y: number } | null>(null);
  const dragging = React.useRef(false);

  const onMouseDown = React.useCallback(
    (e: React.MouseEvent) => {
      if (e.button !== 0) return;
      down.current = { x: e.clientX, y: e.clientY };
      dragging.current = false;

      const onMove = async (me: MouseEvent) => {
        if (dragging.current || !down.current) return;
        const dx = me.clientX - down.current.x;
        const dy = me.clientY - down.current.y;
        if (dx * dx + dy * dy < DRAG_THRESHOLD * DRAG_THRESHOLD) return;
        dragging.current = true;
        cleanup();
        try {
          const [abs, icon] = await Promise.all([fsAbsPath(path), getDragIcon()]);
          dragOutActive = true;
          await startDrag({ item: [abs], icon });
        } catch {
          // drag cancelled or failed
        } finally {
          window.addEventListener(
            "mousedown",
            () => {
              dragOutActive = false;
            },
            { once: true },
          );
        }
      };

      const onUp = () => {
        cleanup();
      };

      const cleanup = () => {
        down.current = null;
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
      };

      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    [path],
  );

  return { onMouseDown };
}
