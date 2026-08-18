/** Shared module-level clock ticker for useSyncExternalStore time hooks.
    One timer per granularity feeds every consumer, so all surfaces rendering
    against the same clock tick together by construction. The timer is
    visibility-aware: it stops while the document is hidden (no wakeups in
    background windows) and, on return to visible, refreshes the snapshot so
    labels self-correct immediately before the aligned timer resumes. */

export type NowTicker<T> = {
  subscribe: (listener: () => void) => () => void;
  getSnapshot: () => T;
};

export function createNowTicker<T>(options: {
  /** Quantized clock read; listeners are notified when this value changes. */
  read: () => T;
  /** Tick period. The first tick is aligned to the next period boundary. */
  intervalMs: number;
}): NowTicker<T> {
  const { read, intervalMs } = options;

  let snapshot = read();
  let timerId: number | null = null;
  let timerIsInterval = false;
  const listeners = new Set<() => void>();

  function tick(): void {
    const next = read();
    if (!Object.is(next, snapshot)) {
      snapshot = next;
      for (const listener of listeners) listener();
    }
  }

  function documentIsHidden(): boolean {
    return typeof document !== "undefined" && document.visibilityState !== "visible";
  }

  function startTimer(): void {
    // Align to the next period boundary, then tick every `intervalMs`. Ticks
    // re-read the clock, so a throttled or late timer self-corrects when it
    // fires.
    timerIsInterval = false;
    timerId = window.setTimeout(
      () => {
        tick();
        timerIsInterval = true;
        timerId = window.setInterval(tick, intervalMs);
      },
      intervalMs - (Date.now() % intervalMs),
    );
  }

  function stopTimer(): void {
    if (timerId === null) {
      return;
    }
    if (timerIsInterval) window.clearInterval(timerId);
    else window.clearTimeout(timerId);
    timerId = null;
  }

  function handleVisibilityChange(): void {
    if (documentIsHidden()) {
      stopTimer();
      return;
    }
    // Catch the snapshot up immediately so labels self-correct the moment the
    // window becomes visible again, then resume the aligned timer.
    tick();
    if (timerId === null && listeners.size > 0) {
      startTimer();
    }
  }

  function subscribe(listener: () => void): () => void {
    if (listeners.size === 0) {
      if (typeof document !== "undefined") {
        document.addEventListener("visibilitychange", handleVisibilityChange);
      }
      if (!documentIsHidden()) {
        startTimer();
      }
    }
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
      if (listeners.size === 0) {
        if (typeof document !== "undefined") {
          document.removeEventListener("visibilitychange", handleVisibilityChange);
        }
        stopTimer();
      }
    };
  }

  function getSnapshot(): T {
    // With no timer running (no subscribers yet, or paused while hidden), the
    // stored value may be stale; re-read it so a fresh mount — or a render
    // while hidden — sees the current clock instead of waiting for a tick.
    // While the timer runs the cached value is returned untouched, as
    // useSyncExternalStore requires between change notifications.
    if (timerId === null) {
      snapshot = read();
    }
    return snapshot;
  }

  return { subscribe, getSnapshot };
}
