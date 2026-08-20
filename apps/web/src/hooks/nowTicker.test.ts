import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";

import { createNowTicker } from "./nowTicker";

function currentSecondMs(): number {
  return Math.floor(Date.now() / 1_000) * 1_000;
}

function createSecondTicker() {
  return createNowTicker({ read: currentSecondMs, intervalMs: 1_000 });
}

type TestDocument = {
  visibilityState: DocumentVisibilityState;
  addEventListener: EventTarget["addEventListener"];
  removeEventListener: EventTarget["removeEventListener"];
  dispatchEvent: EventTarget["dispatchEvent"];
};

let testDocument: TestDocument;

function setVisibility(state: DocumentVisibilityState) {
  testDocument.visibilityState = state;
  testDocument.dispatchEvent(new Event("visibilitychange"));
}

beforeEach(() => {
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  });
  vi.setSystemTime(new Date("2026-08-19T10:00:00.000Z"));
  vi.stubGlobal("window", {
    setTimeout: (handler: () => void, timeout?: number) => globalThis.setTimeout(handler, timeout),
    clearTimeout: (id?: number) => globalThis.clearTimeout(id),
    setInterval: (handler: () => void, timeout?: number) =>
      globalThis.setInterval(handler, timeout),
    clearInterval: (id?: number) => globalThis.clearInterval(id),
  });
  const eventTarget = new EventTarget();
  testDocument = {
    visibilityState: "visible",
    addEventListener: eventTarget.addEventListener.bind(eventTarget),
    removeEventListener: eventTarget.removeEventListener.bind(eventTarget),
    dispatchEvent: eventTarget.dispatchEvent.bind(eventTarget),
  };
  vi.stubGlobal("document", testDocument);
});

afterEach(() => {
  vi.resetModules();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("createNowTicker", () => {
  it("notifies every subscriber from one boundary-aligned timer", () => {
    vi.setSystemTime(new Date("2026-08-19T10:00:00.250Z"));
    const ticker = createSecondTicker();
    const first = vi.fn();
    const second = vi.fn();
    ticker.subscribe(first);
    ticker.subscribe(second);
    expect(vi.getTimerCount()).toBe(1);

    vi.advanceTimersByTime(749);
    expect(first).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(1);
    expect(ticker.getSnapshot()).toBe(Date.parse("2026-08-19T10:00:01.000Z"));

    vi.advanceTimersByTime(1_000);
    expect(first).toHaveBeenCalledTimes(2);
    expect(ticker.getSnapshot()).toBe(Date.parse("2026-08-19T10:00:02.000Z"));
  });

  it("tears the timer down when the last subscriber leaves", () => {
    const ticker = createSecondTicker();
    const listener = vi.fn();
    const unsubscribe = ticker.subscribe(listener);
    expect(vi.getTimerCount()).toBe(1);

    unsubscribe();
    expect(vi.getTimerCount()).toBe(0);

    vi.advanceTimersByTime(5_000);
    expect(listener).not.toHaveBeenCalled();
  });

  it("re-reads a stale snapshot while no timer is running", () => {
    const ticker = createSecondTicker();
    expect(ticker.getSnapshot()).toBe(Date.parse("2026-08-19T10:00:00.000Z"));

    vi.advanceTimersByTime(90_000);
    expect(ticker.getSnapshot()).toBe(Date.parse("2026-08-19T10:01:30.000Z"));
  });

  it("pauses while the document is hidden and self-corrects on return", () => {
    const ticker = createSecondTicker();
    const listener = vi.fn();
    ticker.subscribe(listener);

    setVisibility("hidden");
    expect(vi.getTimerCount()).toBe(0);

    vi.advanceTimersByTime(5_000);
    expect(listener).not.toHaveBeenCalled();

    setVisibility("visible");
    // The catch-up tick fires immediately so labels self-correct...
    expect(listener).toHaveBeenCalledTimes(1);
    expect(ticker.getSnapshot()).toBe(Date.parse("2026-08-19T10:00:05.000Z"));
    // ...and the aligned timer resumes.
    expect(vi.getTimerCount()).toBe(1);
    vi.advanceTimersByTime(1_000);
    expect(listener).toHaveBeenCalledTimes(2);
  });

  it("does not start a timer when the first subscriber arrives hidden", () => {
    testDocument.visibilityState = "hidden";
    const ticker = createSecondTicker();
    const listener = vi.fn();
    ticker.subscribe(listener);
    expect(vi.getTimerCount()).toBe(0);

    // Renders while hidden still see a fresh clock via the stale-snapshot
    // re-read.
    vi.advanceTimersByTime(3_000);
    expect(ticker.getSnapshot()).toBe(Date.parse("2026-08-19T10:00:03.000Z"));

    setVisibility("visible");
    expect(vi.getTimerCount()).toBe(1);
    vi.advanceTimersByTime(1_000);
    expect(listener).toHaveBeenCalled();
  });

  it("stops listening for visibility changes after the last unsubscribe", () => {
    const ticker = createSecondTicker();
    const listener = vi.fn();
    const unsubscribe = ticker.subscribe(listener);
    unsubscribe();

    // A visibility flip with no subscribers must not restart anything.
    setVisibility("hidden");
    setVisibility("visible");
    expect(vi.getTimerCount()).toBe(0);
  });
});

describe("useNowSecond", () => {
  it("drives direct-DOM subscribers off one visibility-aware second timer", async () => {
    const { subscribeToNowSecond } = await import("./useNowSecond");
    const listener = vi.fn();
    const unsubscribe = subscribeToNowSecond(listener);

    vi.advanceTimersByTime(1_000);
    expect(listener).toHaveBeenCalledTimes(1);

    setVisibility("hidden");
    vi.advanceTimersByTime(10_000);
    expect(listener).toHaveBeenCalledTimes(1);

    setVisibility("visible");
    expect(listener).toHaveBeenCalledTimes(2);

    unsubscribe();
    expect(vi.getTimerCount()).toBe(0);
  });
});
