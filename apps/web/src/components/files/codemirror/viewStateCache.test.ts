import { describe, expect, it } from "vite-plus/test";

import {
  loadEditorViewState,
  saveEditorViewState,
  shouldRestoreEditorViewState,
  type CachedEditorViewState,
} from "./viewStateCache";

const state = (overrides?: Partial<CachedEditorViewState>): CachedEditorViewState => ({
  anchor: 10,
  head: 10,
  scrollTop: 120,
  revealRequestId: 3,
  ...overrides,
});

describe("saveEditorViewState / loadEditorViewState", () => {
  it("round-trips a saved position", () => {
    saveEditorViewState("env:/root:a.ts", state());
    expect(loadEditorViewState("env:/root:a.ts")).toEqual(state());
  });

  it("returns null for unknown keys", () => {
    expect(loadEditorViewState("env:/root:never-opened.ts")).toBeNull();
  });

  it("overwrites on re-save", () => {
    saveEditorViewState("env:/root:b.ts", state({ scrollTop: 1 }));
    saveEditorViewState("env:/root:b.ts", state({ scrollTop: 2 }));
    expect(loadEditorViewState("env:/root:b.ts")?.scrollTop).toBe(2);
  });

  it("evicts the least recently saved entry past the cap", () => {
    for (let index = 0; index < 200; index += 1) {
      saveEditorViewState(`lru:${index}`, state());
    }
    // Bump lru:0 to newest, then push one more entry over the cap.
    saveEditorViewState("lru:0", state());
    saveEditorViewState("lru:overflow", state());
    expect(loadEditorViewState("lru:0")).not.toBeNull();
    expect(loadEditorViewState("lru:overflow")).not.toBeNull();
    expect(loadEditorViewState("lru:1")).toBeNull();
  });
});

describe("shouldRestoreEditorViewState", () => {
  it("does not restore without a saved position", () => {
    expect(shouldRestoreEditorViewState(null, null, 1)).toBe(false);
    expect(shouldRestoreEditorViewState(null, 12, 2)).toBe(false);
  });

  it("restores on a tab switch (same requestId, stale revealLine)", () => {
    expect(shouldRestoreEditorViewState(state({ revealRequestId: 3 }), 12, 3)).toBe(true);
  });

  it("restores on a file-tree re-open (bumped requestId, null revealLine)", () => {
    expect(shouldRestoreEditorViewState(state({ revealRequestId: 3 }), null, 4)).toBe(true);
  });

  it("yields to a fresh explicit reveal (bumped requestId with a line)", () => {
    expect(shouldRestoreEditorViewState(state({ revealRequestId: 3 }), 12, 4)).toBe(false);
  });

  it("yields when captured before any reveal and one arrives later", () => {
    expect(shouldRestoreEditorViewState(state({ revealRequestId: undefined }), 12, 1)).toBe(false);
  });

  it("restores when the editor never receives reveal props", () => {
    expect(
      shouldRestoreEditorViewState(state({ revealRequestId: undefined }), null, undefined),
    ).toBe(true);
  });
});
