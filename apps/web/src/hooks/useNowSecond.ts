import { useSyncExternalStore } from "react";

import { createNowTicker } from "./nowTicker";

/** Second-quantized epoch milliseconds for live elapsed/countdown labels.
    One module-level timer feeds every consumer (working-duration rows,
    connection/expiry labels) instead of one setInterval per label, and it is
    visibility-aware: paused while the window is hidden, refreshed on return
    to visible (see createNowTicker). */

function currentSecondMs(): number {
  return Math.floor(Date.now() / 1_000) * 1_000;
}

const ticker = createNowTicker({ read: currentSecondMs, intervalMs: 1_000 });

/** Subscribe to the shared second tick without a React render — for labels
    that write textContent directly (e.g. the streaming "Working for Xs"
    timer). The listener fires once per second while visible and once
    immediately when the window becomes visible again. */
export function subscribeToNowSecond(listener: () => void): () => void {
  return ticker.subscribe(listener);
}

export function useNowSecond(): number {
  return useSyncExternalStore(ticker.subscribe, ticker.getSnapshot, ticker.getSnapshot);
}
