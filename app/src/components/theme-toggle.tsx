import * as React from "react";
import { Monitor, Moon, Sun } from "lucide-react";
import { useTheme } from "next-themes";

import { Button } from "@/components/ui/button";

/**
 * Icon button that cycles `system → light → dark → system`. Lives in
 * the TopBar (and the Welcome screen header) so it's available with or
 * without an attached project.
 *
 * Backed by `next-themes` per the shadcn `customization.md §Dark Mode`
 * recommendation. The `attribute="class"` + `storageKey="progest:theme"`
 * provider props are wired in `App.tsx`; the inline boot script in
 * `index.html` reads the same key to prevent FOUC on hard reload.
 */
export function ThemeToggle() {
  const { theme, setTheme } = useTheme();
  // next-themes hydrates `theme` lazily — render a stable placeholder
  // until the provider has read storage to avoid an SSR-style mismatch
  // (Tauri runs as a static SPA so the first paint sees `undefined`).
  const [mounted, setMounted] = React.useState(false);
  React.useEffect(() => setMounted(true), []);

  const current: "system" | "light" | "dark" =
    !mounted || !theme ? "system" : (theme as "system" | "light" | "dark");
  const Icon = current === "system" ? Monitor : current === "light" ? Sun : Moon;
  const next: typeof current =
    current === "system" ? "light" : current === "light" ? "dark" : "system";

  return (
    <Button
      variant="ghost"
      size="icon-sm"
      onClick={() => setTheme(next)}
      title={`Theme: ${current} (click to switch to ${next})`}
      aria-label={`Theme ${current}, switch to ${next}`}
    >
      <Icon />
    </Button>
  );
}
