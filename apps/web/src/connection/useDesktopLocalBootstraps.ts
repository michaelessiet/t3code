import type { DesktopEnvironmentBootstrap } from "@t3tools/contracts";
import { useEffect, useState } from "react";

import {
  readDesktopSecondaryBootstraps,
  subscribeDesktopLocalBootstrapsChanged,
} from "./desktopLocal";

/**
 * Reactively track the desktop's secondary local backends (e.g. a parallel WSL
 * backend). The bridge pushes a change ping whenever the topology may have
 * changed, so we read once on mount, re-read on each ping, and re-read when
 * the document becomes visible again as a cheap safety net (which also covers
 * older desktop mains that predate the change event). Failed reads retain the
 * latest successful snapshot, while a successful empty read clears it. Use
 * this instead of reading the bridge ad hoc so every renderer consumer reads
 * the same topology.
 */
export function useDesktopLocalBootstraps(): ReadonlyArray<DesktopEnvironmentBootstrap> {
  const [bootstraps, setBootstraps] = useState<ReadonlyArray<DesktopEnvironmentBootstrap>>(
    readDesktopSecondaryBootstraps,
  );

  useEffect(() => {
    const read = () => setBootstraps(readDesktopSecondaryBootstraps());
    read();
    const unsubscribe = subscribeDesktopLocalBootstrapsChanged(read);
    const onVisibilityChange = () => {
      if (document.visibilityState === "visible") {
        read();
      }
    };
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => {
      unsubscribe();
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
  }, []);

  return bootstraps;
}
