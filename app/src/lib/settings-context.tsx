import * as React from "react";

import { aiGetConfig, type AiConfigResponse } from "@/lib/ipc";
import { useProject } from "@/lib/project-context";

type SettingsContextValue = {
  open: boolean;
  openSettings: (tab?: string) => void;
  closeSettings: () => void;
  tab: string;
  aiConfig: AiConfigResponse | null;
  aiConfigVersion: number;
  bumpAiConfig: () => void;
};

const SettingsContext = React.createContext<SettingsContextValue>({
  open: false,
  openSettings: () => {},
  closeSettings: () => {},
  tab: "ai",
  aiConfig: null,
  aiConfigVersion: 0,
  bumpAiConfig: () => {},
});

export function SettingsProvider({ children }: { children: React.ReactNode }) {
  const [open, setOpen] = React.useState(false);
  const [tab, setTab] = React.useState("ai");
  const { project } = useProject();

  const [aiConfig, setAiConfig] = React.useState<AiConfigResponse | null>(null);
  const [aiConfigVersion, setAiConfigVersion] = React.useState(0);
  const bumpAiConfig = React.useCallback(() => {
    setAiConfigVersion((n) => n + 1);
  }, []);

  React.useEffect(() => {
    if (!project) {
      setAiConfig(null);
      return;
    }
    let cancelled = false;
    aiGetConfig()
      .then((c) => {
        if (!cancelled) setAiConfig(c);
      })
      .catch(() => {
        if (!cancelled) setAiConfig(null);
      });
    return () => {
      cancelled = true;
    };
  }, [project?.root, aiConfigVersion]);

  const openSettings = React.useCallback((t = "ai") => {
    setTab(t);
    setOpen(true);
  }, []);

  const closeSettings = React.useCallback(() => {
    setOpen(false);
  }, []);

  const value = React.useMemo(
    () => ({ open, openSettings, closeSettings, tab, aiConfig, aiConfigVersion, bumpAiConfig }),
    [open, openSettings, closeSettings, tab, aiConfig, aiConfigVersion, bumpAiConfig],
  );

  return <SettingsContext value={value}>{children}</SettingsContext>;
}

export function useSettings() {
  return React.useContext(SettingsContext);
}
