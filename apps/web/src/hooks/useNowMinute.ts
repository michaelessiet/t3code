import { useSyncExternalStore } from "react";

import { createNowTicker } from "./nowTicker";

/** Minute-quantized clock ("YYYY-MM-DDTHH:MM") for settled-state resolution.
    One module-level timer feeds every consumer through useSyncExternalStore,
    so all surfaces resolving effectiveSettled against it (sidebar partition,
    composer banner) share a single value by construction and tick on UTC
    minute boundaries together. Visibility-aware: paused while the window is
    hidden, refreshed on return to visible (see createNowTicker). */

function currentMinute(): string {
  return new Date().toISOString().slice(0, 16);
}

const ticker = createNowTicker({ read: currentMinute, intervalMs: 60_000 });

export function useNowMinute(): string {
  return useSyncExternalStore(ticker.subscribe, ticker.getSnapshot, ticker.getSnapshot);
}
