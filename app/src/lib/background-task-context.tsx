import * as React from "react";

export type BackgroundTask = {
  id: string;
  label: string;
  current: number;
  total: number;
};

type BackgroundTaskContextValue = {
  tasks: BackgroundTask[];
  startTask: (id: string, label: string, total?: number) => void;
  updateTask: (id: string, current: number, total: number, label?: string) => void;
  finishTask: (id: string) => void;
};

const Ctx = React.createContext<BackgroundTaskContextValue>({
  tasks: [],
  startTask: () => {},
  updateTask: () => {},
  finishTask: () => {},
});

export function useBackgroundTasks() {
  return React.useContext(Ctx);
}

export function BackgroundTaskProvider(props: { children: React.ReactNode }) {
  const [tasks, setTasks] = React.useState<BackgroundTask[]>([]);

  const startTask = React.useCallback((id: string, label: string, total = 0) => {
    setTasks((prev) => {
      const existing = prev.find((t) => t.id === id);
      if (existing) return prev.map((t) => (t.id === id ? { ...t, label, current: 0, total } : t));
      return [...prev, { id, label, current: 0, total }];
    });
  }, []);

  const updateTask = React.useCallback(
    (id: string, current: number, total: number, label?: string) => {
      setTasks((prev) =>
        prev.map((t) => (t.id === id ? { ...t, current, total, ...(label ? { label } : {}) } : t)),
      );
    },
    [],
  );

  const finishTask = React.useCallback((id: string) => {
    setTasks((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const value = React.useMemo(
    () => ({ tasks, startTask, updateTask, finishTask }),
    [tasks, startTask, updateTask, finishTask],
  );

  return <Ctx.Provider value={value}>{props.children}</Ctx.Provider>;
}
