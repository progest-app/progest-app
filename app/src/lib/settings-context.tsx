import * as React from "react";

type SettingsContextValue = {
  open: boolean;
  openSettings: (tab?: string) => void;
  closeSettings: () => void;
  tab: string;
};

const SettingsContext = React.createContext<SettingsContextValue>({
  open: false,
  openSettings: () => {},
  closeSettings: () => {},
  tab: "ai",
});

export function SettingsProvider({ children }: { children: React.ReactNode }) {
  const [open, setOpen] = React.useState(false);
  const [tab, setTab] = React.useState("ai");

  const openSettings = React.useCallback((t = "ai") => {
    setTab(t);
    setOpen(true);
  }, []);

  const closeSettings = React.useCallback(() => {
    setOpen(false);
  }, []);

  const value = React.useMemo(
    () => ({ open, openSettings, closeSettings, tab }),
    [open, openSettings, closeSettings, tab],
  );

  return <SettingsContext value={value}>{children}</SettingsContext>;
}

export function useSettings() {
  return React.useContext(SettingsContext);
}
