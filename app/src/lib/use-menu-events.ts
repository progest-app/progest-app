import * as React from "react";
import { listen } from "@tauri-apps/api/event";

export function useMenuEvents(handlers: Record<string, () => void>) {
  const handlersRef = React.useRef(handlers);
  handlersRef.current = handlers;

  React.useEffect(() => {
    const unlisten = listen<string>("menu-action", (event) => {
      const handler = handlersRef.current[event.payload];
      if (handler) handler();
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);
}
