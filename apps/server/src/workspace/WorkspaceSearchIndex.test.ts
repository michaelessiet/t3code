// @effect-diagnostics nodeBuiltinImport:off
import * as NodeChildProcess from "node:child_process";
import * as NodeFSP from "node:fs/promises";
import * as NodeOS from "node:os";
import * as NodePath from "node:path";
import * as NodeUtil from "node:util";

import { FileFinder } from "@ff-labs/fff-node";
import { afterAll, afterEach, beforeAll, expect, it } from "@effect/vitest";
import * as Cause from "effect/Cause";
import * as Effect from "effect/Effect";
import * as Exit from "effect/Exit";
import { vi } from "vite-plus/test";

import * as WorkspaceSearchIndex from "./WorkspaceSearchIndex.ts";

afterEach(() => {
  vi.restoreAllMocks();
});

it.effect("preserves unexpected FileFinder creation failures", () =>
  Effect.gen(function* () {
    const cause = new Error("native initialization failed");
    vi.spyOn(FileFinder, "create").mockImplementationOnce(() => {
      throw cause;
    });

    const error = yield* Effect.flip(
      Effect.scoped(WorkspaceSearchIndex.make("/workspace/project")),
    );

    expect(error).toMatchObject({
      _tag: "WorkspaceSearchIndexCreateFailed",
      cwd: "/workspace/project",
      reason: "FileFinder.create threw unexpectedly.",
      cause,
    });
  }),
);

it.effect("keeps returned FileFinder creation diagnostics out of the cause chain", () =>
  Effect.gen(function* () {
    vi.spyOn(FileFinder, "create").mockReturnValueOnce({
      ok: false,
      error: "native index rejected the directory",
    });

    const error = yield* Effect.flip(
      Effect.scoped(WorkspaceSearchIndex.make("/workspace/project")),
    );

    expect(error).toMatchObject({
      _tag: "WorkspaceSearchIndexCreateFailed",
      cwd: "/workspace/project",
      reason: "native index rejected the directory",
    });
    expect(error.cause).toBeUndefined();
  }),
);

it.effect("preserves FileFinder destroy failures as structured defects", () =>
  Effect.gen(function* () {
    const cause = new Error("native destroy failed");
    const finder = {
      destroy: vi.fn(() => {
        throw cause;
      }),
      isScanning: vi.fn(() => false),
    } as unknown as FileFinder;
    vi.spyOn(FileFinder, "create").mockReturnValueOnce({ ok: true, value: finder });

    const exit = yield* Effect.scoped(WorkspaceSearchIndex.make("/workspace/project")).pipe(
      Effect.exit,
    );

    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      expect(Cause.hasDies(exit.cause)).toBe(true);
      const error = Cause.squash(exit.cause);
      expect(error).toBeInstanceOf(WorkspaceSearchIndex.WorkspaceSearchIndexDestroyFailed);
      expect(error).toMatchObject({
        _tag: "WorkspaceSearchIndexDestroyFailed",
        cwd: "/workspace/project",
        cause,
      });
    }
  }),
);

it.effect("preserves search and refresh failures with operation context", () =>
  Effect.scoped(
    Effect.gen(function* () {
      const searchCause = new Error("native search failed");
      const refreshCause = new Error("native scan failed");
      const finder = {
        destroy: vi.fn(),
        isScanning: vi.fn(() => false),
        mixedSearch: vi.fn(() => {
          throw searchCause;
        }),
        scanFiles: vi.fn(() => {
          throw refreshCause;
        }),
      } as unknown as FileFinder;
      vi.spyOn(FileFinder, "create").mockReturnValueOnce({ ok: true, value: finder });

      const searchIndex = yield* WorkspaceSearchIndex.make("/workspace/project");
      const query = "authorization: Bearer secret-token";
      const searchError = yield* Effect.flip(searchIndex.search(query, 3));
      const refreshError = yield* Effect.flip(searchIndex.refresh());

      expect(searchError).toMatchObject({
        _tag: "WorkspaceSearchIndexSearchFailed",
        cwd: "/workspace/project",
        queryLength: query.length,
        pageSize: 4,
        reason: "FileFinder.mixedSearch threw unexpectedly.",
        cause: searchCause,
      });
      expect(searchError).not.toHaveProperty("query");
      expect(searchError.message).not.toMatch(/Bearer|secret-token/);
      expect(refreshError).toMatchObject({
        _tag: "WorkspaceSearchIndexRefreshFailed",
        cwd: "/workspace/project",
        reason: "FileFinder.scanFiles threw unexpectedly.",
        cause: refreshCause,
      });
    }),
  ),
);

it.effect("keeps returned search diagnostics out of the cause chain", () =>
  Effect.scoped(
    Effect.gen(function* () {
      const finder = {
        destroy: vi.fn(),
        isScanning: vi.fn(() => false),
        mixedSearch: vi.fn(() => ({ ok: false, error: "native query rejected" })),
        scanFiles: vi.fn(() => ({ ok: false, error: "native refresh rejected" })),
      } as unknown as FileFinder;
      vi.spyOn(FileFinder, "create").mockReturnValueOnce({ ok: true, value: finder });

      const searchIndex = yield* WorkspaceSearchIndex.make("/workspace/project");
      const query = "authorization: Bearer secret-token";
      const searchError = yield* Effect.flip(searchIndex.search(query, 3));
      const refreshError = yield* Effect.flip(searchIndex.refresh());

      expect(searchError).toMatchObject({
        _tag: "WorkspaceSearchIndexSearchFailed",
        cwd: "/workspace/project",
        queryLength: query.length,
        pageSize: 4,
        reason: "native query rejected",
      });
      expect(searchError).not.toHaveProperty("query");
      expect(searchError.message).not.toMatch(/Bearer|secret-token/);
      expect(searchError.cause).toBeUndefined();
      expect(refreshError).toMatchObject({
        _tag: "WorkspaceSearchIndexRefreshFailed",
        cwd: "/workspace/project",
        reason: "native refresh rejected",
      });
      expect(refreshError.cause).toBeUndefined();
    }),
  ),
);

it("merges supplement entries after the indexed entries with the indexed entry winning", () => {
  const merged = WorkspaceSearchIndex.mergeSupplementEntries(
    [
      { path: "src/index.ts", kind: "file" },
      { path: "dist", kind: "directory" },
    ],
    [
      { path: "dist", kind: "directory", ignored: true },
      { path: ".env", kind: "file", ignored: true },
    ],
  );

  expect(merged).toEqual([
    { path: "src/index.ts", kind: "file" },
    { path: "dist", kind: "directory" },
    { path: ".env", kind: "file", ignored: true },
  ]);
});

it("ranks contiguous-substring matches above scattered fuzzy matches from either source", () => {
  const indexed = [
    { path: "docs/deep-store-notes.md", kind: "file" },
    { path: "src/DataStore.ts", kind: "file" },
  ] as const;
  const supplementMatches = [{ path: ".DS_Store", kind: "file", ignored: true }] as const;

  const merged = WorkspaceSearchIndex.rankMergedSearchEntries(
    [...indexed],
    [...supplementMatches],
    "ds_store",
  );

  // Only .DS_Store contains "ds_store" contiguously; it outranks the fuzzy tail.
  expect(merged.map((entry) => entry.path)).toEqual([
    ".DS_Store",
    "docs/deep-store-notes.md",
    "src/DataStore.ts",
  ]);
});

it("preserves native relevance order and dedupes when ranking merged results", () => {
  const merged = WorkspaceSearchIndex.rankMergedSearchEntries(
    [
      { path: "src/env/reader.ts", kind: "file" },
      { path: "docs/envelope.md", kind: "file" },
    ],
    [
      { path: ".env", kind: "file", ignored: true },
      { path: "src/env/reader.ts", kind: "file", ignored: true },
    ],
    "env",
  );

  expect(merged).toEqual([
    { path: "src/env/reader.ts", kind: "file" },
    { path: "docs/envelope.md", kind: "file" },
    { path: ".env", kind: "file", ignored: true },
  ]);
});

it("matches supplement paths by case-insensitive subsequence", () => {
  expect(WorkspaceSearchIndex.matchesSupplementQuery("dist/App.js", "dapp")).toBe(true);
  expect(WorkspaceSearchIndex.matchesSupplementQuery(".env", "env")).toBe(true);
  expect(WorkspaceSearchIndex.matchesSupplementQuery(".env", "")).toBe(true);
  expect(WorkspaceSearchIndex.matchesSupplementQuery(".env", "envx")).toBe(false);
  expect(WorkspaceSearchIndex.matchesSupplementQuery("dist/a.js", "sub")).toBe(false);
});

const execFile = NodeUtil.promisify(NodeChildProcess.execFile);

let ignoredFixture: string;

beforeAll(async () => {
  ignoredFixture = await NodeFSP.mkdtemp(NodePath.join(NodeOS.tmpdir(), "t3-search-index-git-"));
  await execFile("git", ["-C", ignoredFixture, "init", "--quiet"]);
  await NodeFSP.writeFile(NodePath.join(ignoredFixture, ".gitignore"), ".env\ndist/\n");
  await NodeFSP.writeFile(NodePath.join(ignoredFixture, ".env"), "SECRET=1\n");
  await NodeFSP.mkdir(NodePath.join(ignoredFixture, "dist"), { recursive: true });
  await NodeFSP.writeFile(NodePath.join(ignoredFixture, "dist", "app.js"), "1\n");
});

afterAll(async () => {
  await NodeFSP.rm(ignoredFixture, { recursive: true, force: true });
});

// Mimics the native index for a workspace whose only non-ignored file is
// src/index.ts: the listing query ("") returns it, everything else misses.
const indexedOnlyFinder = () =>
  ({
    destroy: vi.fn(),
    isScanning: vi.fn(() => false),
    mixedSearch: vi.fn((query: string) => ({
      ok: true,
      value:
        query === ""
          ? {
              items: [{ type: "file", item: { relativePath: "src/index.ts" } }],
              totalMatched: 1,
            }
          : { items: [], totalMatched: 0 },
    })),
    scanFiles: vi.fn(() => ({ ok: true, value: undefined })),
  }) as unknown as FileFinder;

it.effect("list includes gitignored entries the native index cannot see", () =>
  Effect.scoped(
    Effect.gen(function* () {
      vi.spyOn(FileFinder, "create").mockReturnValueOnce({
        ok: true,
        value: indexedOnlyFinder(),
      });

      const searchIndex = yield* WorkspaceSearchIndex.make(ignoredFixture);
      const { entries, truncated } = yield* searchIndex.list();

      const byPath = new Map(entries.map((entry) => [entry.path, entry]));
      expect(byPath.get("src/index.ts")).toEqual({ path: "src/index.ts", kind: "file" });
      expect(byPath.get(".env")).toEqual({ path: ".env", kind: "file", ignored: true });
      expect(byPath.get("dist")).toEqual({ path: "dist", kind: "directory", ignored: true });
      expect(byPath.get("dist/app.js")).toEqual({
        path: "dist/app.js",
        kind: "file",
        ignored: true,
      });
      expect(truncated).toBe(false);
    }),
  ),
);

it.effect("search surfaces gitignored entries after the native index results", () =>
  Effect.scoped(
    Effect.gen(function* () {
      vi.spyOn(FileFinder, "create").mockReturnValueOnce({
        ok: true,
        value: indexedOnlyFinder(),
      });

      const searchIndex = yield* WorkspaceSearchIndex.make(ignoredFixture);
      const envResult = yield* searchIndex.search("env", 10);
      expect(envResult.entries).toEqual([{ path: ".env", kind: "file", ignored: true }]);

      const distResult = yield* searchIndex.search("distapp", 10);
      expect(distResult.entries).toEqual([{ path: "dist/app.js", kind: "file", ignored: true }]);

      const missResult = yield* searchIndex.search("zzzz", 10);
      expect(missResult.entries).toEqual([]);
    }),
  ),
);
