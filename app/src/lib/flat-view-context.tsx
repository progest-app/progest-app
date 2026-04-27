import * as React from "react";

import type { RichViolationCounts, View } from "@/lib/ipc";

/**
 * Cross-component summary of project-level state surfaced by the
 * FlatView, lifted out of `<FlatView>` so the bottom status bar can
 * render the same "active view / total violations" indicators
 * without duplicating the search effect.
 *
 * Per-query feedback (parse error, validate warnings, IPC error,
 * hit count, loading spinner) lives inside `<FlatView>`'s own
 * header — those belong next to the input that produced them.
 * Cross-cutting health indicators (project info, saved view,
 * aggregate violation counts) live in the status bar so the user
 * can glance at them regardless of which panel has focus.
 *
 * `<FlatView>` still owns the underlying state — query, response,
 * saved-view selection, debouncer — and `<StatusBar>` is a passive
 * consumer. The contract is:
 *
 *   1. `<App>` wraps everything in `<FlatViewSummaryProvider>`.
 *   2. `<FlatView>` calls `useReportFlatView()` and pushes a fresh
 *      summary on every state change via `useEffect`.
 *   3. `<StatusBar>` calls `useFlatViewSummary()` to render.
 */
export type FlatViewSummary = {
  /** Saved view currently driving the FlatView, if any. */
  activeView: View | null;
  /** Sum of naming / placement / sequence violations across the
   *  current FlatView result set. When the query is empty (showing
   *  every file) this acts as the project-wide health summary;
   *  when filtered, it shows the violation count within that
   *  filter — useful for "how many psd files still have violations". */
  violationTotals: RichViolationCounts;
};

const DEFAULT_SUMMARY: FlatViewSummary = {
  activeView: null,
  violationTotals: { naming: 0, placement: 0, sequence: 0 },
};

const SummaryContext = React.createContext<FlatViewSummary>(DEFAULT_SUMMARY);
const ReportContext = React.createContext<(patch: Partial<FlatViewSummary>) => void>(
  () => {},
);

export function FlatViewSummaryProvider({ children }: { children: React.ReactNode }) {
  const [summary, setSummary] = React.useState<FlatViewSummary>(DEFAULT_SUMMARY);
  const report = React.useCallback(
    (patch: Partial<FlatViewSummary>) => {
      // Functional update so multiple reports inside one tick don't
      // clobber each other.
      setSummary((prev) => ({ ...prev, ...patch }));
    },
    [],
  );
  return (
    <SummaryContext.Provider value={summary}>
      <ReportContext.Provider value={report}>{children}</ReportContext.Provider>
    </SummaryContext.Provider>
  );
}

export function useFlatViewSummary(): FlatViewSummary {
  return React.useContext(SummaryContext);
}

export function useReportFlatView(): (patch: Partial<FlatViewSummary>) => void {
  return React.useContext(ReportContext);
}
