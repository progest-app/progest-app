import * as React from "react";

import { thumbnailPaths } from "@/lib/ipc";
import { useProject } from "@/lib/project-context";

/**
 * Batch-fetch thumbnail data URLs for a list of file IDs.
 *
 * Returns a map of `file_id → data:image/webp;base64,...` that can be
 * used directly as `<img src>`.  Files without a cached thumbnail are
 * omitted. Re-fetches when the file ID set changes or when
 * `refreshTick` increments (e.g. after thumbnail generation).
 */
export function useThumbnails(fileIds: string[]): Record<string, string> {
  const { refreshTick } = useProject();
  const [urls, setUrls] = React.useState<Record<string, string>>({});
  const key = fileIds.join(",");

  React.useEffect(() => {
    if (fileIds.length === 0) {
      setUrls({});
      return;
    }

    let cancelled = false;

    thumbnailPaths(fileIds)
      .then((resp) => {
        if (!cancelled) setUrls(resp.urls);
      })
      .catch(() => {
        if (!cancelled) setUrls({});
      });

    return () => {
      cancelled = true;
    };
  }, [key, refreshTick]);

  return urls;
}
