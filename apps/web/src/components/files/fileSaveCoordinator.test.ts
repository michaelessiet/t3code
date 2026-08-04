import { afterEach, describe, expect, it, vi } from "vite-plus/test";
import type { AtomCommandResult } from "@t3tools/client-runtime/state/runtime";
import * as Cause from "effect/Cause";
import { AsyncResult } from "effect/unstable/reactivity";

import { FileSaveCoordinator } from "./fileSaveCoordinator";

function deferred() {
  let resolve!: (result: AtomCommandResult<void, never>) => void;
  const promise = new Promise<AtomCommandResult<void, never>>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

describe("FileSaveCoordinator", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("debounces edits and persists only the latest contents", async () => {
    vi.useFakeTimers();
    const persist = vi
      .fn<(contents: string) => Promise<AtomCommandResult<void, never>>>()
      .mockResolvedValue(AsyncResult.success(undefined));
    const onPendingChange = vi.fn();
    const onConfirmed = vi.fn();
    const coordinator = new FileSaveCoordinator({
      getScheduling: () => ({ kind: "debounce", delayMs: 500 }) as const,
      getFlushOnDispose: () => true,
      persist,
      onPendingChange,
      onConfirmed,
    });

    coordinator.change("first");
    await vi.advanceTimersByTimeAsync(300);
    coordinator.change("latest");
    await vi.advanceTimersByTimeAsync(499);
    expect(persist).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    expect(persist).toHaveBeenCalledOnce();
    expect(persist).toHaveBeenCalledWith("latest");
    expect(onConfirmed).toHaveBeenCalledWith("latest");
    expect(onPendingChange.mock.calls).toEqual([[true], [true], [false]]);
  });

  it("keeps pending state until an edit made during a write is also saved", async () => {
    vi.useFakeTimers();
    const firstWrite = deferred();
    const persist = vi
      .fn<(contents: string) => Promise<AtomCommandResult<void, never>>>()
      .mockReturnValueOnce(firstWrite.promise)
      .mockResolvedValueOnce(AsyncResult.success(undefined));
    const onPendingChange = vi.fn();
    const coordinator = new FileSaveCoordinator({
      getScheduling: () => ({ kind: "debounce", delayMs: 500 }) as const,
      getFlushOnDispose: () => true,
      persist,
      onPendingChange,
      onConfirmed: vi.fn(),
    });

    coordinator.change("first");
    await vi.advanceTimersByTimeAsync(500);
    coordinator.change("latest");
    await vi.advanceTimersByTimeAsync(500);
    expect(persist).toHaveBeenCalledTimes(1);

    firstWrite.resolve(AsyncResult.success(undefined));
    await vi.runAllTimersAsync();
    expect(persist).toHaveBeenCalledTimes(2);
    expect(persist).toHaveBeenLastCalledWith("latest");
    expect(onPendingChange.mock.calls.at(-1)).toEqual([false]);
  });

  it("flush persists pending edits immediately without waiting out the debounce", async () => {
    vi.useFakeTimers();
    const persist = vi
      .fn<(contents: string) => Promise<AtomCommandResult<void, never>>>()
      .mockResolvedValue(AsyncResult.success(undefined));
    const onPendingChange = vi.fn();
    const coordinator = new FileSaveCoordinator({
      getScheduling: () => ({ kind: "debounce", delayMs: 500 }) as const,
      getFlushOnDispose: () => true,
      persist,
      onPendingChange,
      onConfirmed: vi.fn(),
    });

    coordinator.change("write me now");
    coordinator.flush();
    await Promise.resolve();

    expect(persist).toHaveBeenCalledOnce();
    expect(persist).toHaveBeenCalledWith("write me now");
    await vi.runAllTimersAsync();
    expect(persist).toHaveBeenCalledOnce();
    expect(onPendingChange.mock.calls.at(-1)).toEqual([false]);
  });

  it("flush on a clean buffer is a no-op", async () => {
    vi.useFakeTimers();
    const persist = vi
      .fn<(contents: string) => Promise<AtomCommandResult<void, never>>>()
      .mockResolvedValue(AsyncResult.success(undefined));
    const coordinator = new FileSaveCoordinator({
      getScheduling: () => ({ kind: "debounce", delayMs: 500 }) as const,
      getFlushOnDispose: () => true,
      persist,
      onPendingChange: vi.fn(),
      onConfirmed: vi.fn(),
    });

    coordinator.flush();
    await vi.runAllTimersAsync();
    expect(persist).not.toHaveBeenCalled();
  });

  it("reset discards pending edits and clears the pending flag", async () => {
    vi.useFakeTimers();
    const persist = vi
      .fn<(contents: string) => Promise<AtomCommandResult<void, never>>>()
      .mockResolvedValue(AsyncResult.success(undefined));
    const onPendingChange = vi.fn();
    const coordinator = new FileSaveCoordinator({
      getScheduling: () => ({ kind: "debounce", delayMs: 500 }) as const,
      getFlushOnDispose: () => true,
      persist,
      onPendingChange,
      onConfirmed: vi.fn(),
    });

    coordinator.change("discard me");
    coordinator.reset();
    await vi.runAllTimersAsync();

    expect(persist).not.toHaveBeenCalled();
    expect(onPendingChange.mock.calls.at(-1)).toEqual([false]);
  });

  it("reset during an in-flight write settles without rescheduling", async () => {
    vi.useFakeTimers();
    const write = deferred();
    const persist = vi
      .fn<(contents: string) => Promise<AtomCommandResult<void, never>>>()
      .mockReturnValue(write.promise);
    const onPendingChange = vi.fn();
    const coordinator = new FileSaveCoordinator({
      getScheduling: () => ({ kind: "debounce", delayMs: 500 }) as const,
      getFlushOnDispose: () => true,
      persist,
      onPendingChange,
      onConfirmed: vi.fn(),
    });

    coordinator.change("in flight");
    await vi.advanceTimersByTimeAsync(500);
    expect(persist).toHaveBeenCalledTimes(1);

    coordinator.reset();
    write.resolve(AsyncResult.success(undefined));
    await vi.runAllTimersAsync();

    expect(persist).toHaveBeenCalledTimes(1);
    expect(onPendingChange.mock.calls.at(-1)).toEqual([false]);
  });

  it("honors a live scheduling change without recreating the coordinator", async () => {
    vi.useFakeTimers();
    const persist = vi
      .fn<(contents: string) => Promise<AtomCommandResult<void, never>>>()
      .mockResolvedValue(AsyncResult.success(undefined));
    let delayMs = 500;
    const coordinator = new FileSaveCoordinator({
      getScheduling: () => ({ kind: "debounce", delayMs }) as const,
      getFlushOnDispose: () => true,
      persist,
      onPendingChange: vi.fn(),
      onConfirmed: vi.fn(),
    });

    delayMs = 2000;
    coordinator.change("slow save");
    await vi.advanceTimersByTimeAsync(1999);
    expect(persist).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(persist).toHaveBeenCalledOnce();
  });

  it("manual scheduling never persists on its own", async () => {
    vi.useFakeTimers();
    const persist = vi
      .fn<(contents: string) => Promise<AtomCommandResult<void, never>>>()
      .mockResolvedValue(AsyncResult.success(undefined));
    const onPendingChange = vi.fn();
    const coordinator = new FileSaveCoordinator({
      getScheduling: () => ({ kind: "manual" }) as const,
      getFlushOnDispose: () => false,
      persist,
      onPendingChange,
      onConfirmed: vi.fn(),
    });

    coordinator.change("unsaved");
    await vi.runAllTimersAsync();
    expect(persist).not.toHaveBeenCalled();
    expect(onPendingChange.mock.calls.at(-1)).toEqual([true]);

    await coordinator.flush();
    expect(persist).toHaveBeenCalledOnce();
    expect(persist).toHaveBeenCalledWith("unsaved");
    expect(onPendingChange.mock.calls.at(-1)).toEqual([false]);
  });

  it("manual scheduling stays pending after trailing edits instead of rescheduling", async () => {
    vi.useFakeTimers();
    const firstWrite = deferred();
    const persist = vi
      .fn<(contents: string) => Promise<AtomCommandResult<void, never>>>()
      .mockReturnValueOnce(firstWrite.promise)
      .mockResolvedValue(AsyncResult.success(undefined));
    const onPendingChange = vi.fn();
    const coordinator = new FileSaveCoordinator({
      getScheduling: () => ({ kind: "manual" }) as const,
      getFlushOnDispose: () => false,
      persist,
      onPendingChange,
      onConfirmed: vi.fn(),
    });

    coordinator.change("first");
    void coordinator.flush();
    coordinator.change("trailing");
    firstWrite.resolve(AsyncResult.success(undefined));
    await vi.runAllTimersAsync();

    expect(persist).toHaveBeenCalledTimes(1);
    expect(onPendingChange.mock.calls.at(-1)).toEqual([true]);

    await coordinator.flush();
    expect(persist).toHaveBeenCalledTimes(2);
    expect(persist).toHaveBeenLastCalledWith("trailing");
    expect(onPendingChange.mock.calls.at(-1)).toEqual([false]);
  });

  it("a flush during an in-flight save persists trailing edits once it settles", async () => {
    vi.useFakeTimers();
    const firstWrite = deferred();
    const persist = vi
      .fn<(contents: string) => Promise<AtomCommandResult<void, never>>>()
      .mockReturnValueOnce(firstWrite.promise)
      .mockResolvedValue(AsyncResult.success(undefined));
    const coordinator = new FileSaveCoordinator({
      getScheduling: () => ({ kind: "manual" }) as const,
      getFlushOnDispose: () => false,
      persist,
      onPendingChange: vi.fn(),
      onConfirmed: vi.fn(),
    });

    coordinator.change("first");
    const firstFlush = coordinator.flush();
    coordinator.change("trailing");
    const secondFlush = coordinator.flush();
    firstWrite.resolve(AsyncResult.success(undefined));
    await Promise.all([firstFlush, secondFlush]);

    expect(persist).toHaveBeenCalledTimes(2);
    expect(persist).toHaveBeenLastCalledWith("trailing");
  });

  it("dispose leaves pending edits unsaved when flush-on-dispose is off", async () => {
    vi.useFakeTimers();
    const persist = vi
      .fn<(contents: string) => Promise<AtomCommandResult<void, never>>>()
      .mockResolvedValue(AsyncResult.success(undefined));
    const coordinator = new FileSaveCoordinator({
      getScheduling: () => ({ kind: "manual" }) as const,
      getFlushOnDispose: () => false,
      persist,
      onPendingChange: vi.fn(),
      onConfirmed: vi.fn(),
    });

    coordinator.change("keep me pending");
    coordinator.dispose();
    await vi.runAllTimersAsync();
    expect(persist).not.toHaveBeenCalled();
  });

  it("dispose persists pending edits when flush-on-dispose is on", async () => {
    vi.useFakeTimers();
    const persist = vi
      .fn<(contents: string) => Promise<AtomCommandResult<void, never>>>()
      .mockResolvedValue(AsyncResult.success(undefined));
    const coordinator = new FileSaveCoordinator({
      getScheduling: () => ({ kind: "manual" }) as const,
      getFlushOnDispose: () => true,
      persist,
      onPendingChange: vi.fn(),
      onConfirmed: vi.fn(),
    });

    coordinator.change("save on unmount");
    coordinator.dispose();
    await vi.runAllTimersAsync();
    expect(persist).toHaveBeenCalledOnce();
    expect(persist).toHaveBeenCalledWith("save on unmount");
  });

  it("leaves the file pending when the latest write fails", async () => {
    vi.useFakeTimers();
    const onPendingChange = vi.fn();
    const coordinator = new FileSaveCoordinator({
      getScheduling: () => ({ kind: "debounce", delayMs: 500 }) as const,
      getFlushOnDispose: () => true,
      persist: vi
        .fn()
        .mockResolvedValue(AsyncResult.failure(Cause.fail(new Error("write failed")))),
      onPendingChange,
      onConfirmed: vi.fn(),
    });

    coordinator.change("latest");
    await vi.advanceTimersByTimeAsync(500);
    await Promise.resolve();
    expect(onPendingChange).toHaveBeenCalledWith(true);
    expect(onPendingChange).not.toHaveBeenCalledWith(false);
  });
});
