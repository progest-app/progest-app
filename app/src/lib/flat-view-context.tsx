import * as React from "react";

import type { View } from "@/lib/ipc";

/**
 * Cross-component summary of the FlatView's current state, lifted
 * out of `<FlatView>` so the bottom status bar can render the same
 * "searching… / 1234 hits / warnings" indicators without duplicating
 * the search effect.
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
  /** True while the search effect is in flight. */
  loading: boolean;
  /** Total hits in the current response. `null` when no response is
   *  loaded (e.g. before the first fetch on a fresh project). */
  hitCount: number | null;
  /** Validate-stage warnings (`unknown_key`, `type_and_multi`, …). */
  warnings: string[];
  /** Parse error message, if the current query failed to parse. */
  parseError: string | null;
  /** IPC-level error (e.g. `no_project`, sqlite errors). */
  error: string | null;
  /** Saved view currently driving the FlatView, if any. */
  activeView: View | null;
};

const DEFAULT_SUMMARY: FlatViewSummary = {
  loading: false,
  hitCount: null,
  warnings: [],
  parseError: null,
  error: null,
  activeView: null,
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
