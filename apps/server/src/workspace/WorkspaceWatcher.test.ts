import * as NodeServices from "@effect/platform-node/NodeServices";
import { it, describe, expect } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as Fiber from "effect/Fiber";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Path from "effect/Path";
import * as Stream from "effect/Stream";

import * as WorkspacePaths from "./WorkspacePaths.ts";
import * as WorkspaceWatcher from "./WorkspaceWatcher.ts";

const TestLayer = WorkspaceWatcher.layer.pipe(
  Layer.provide(WorkspacePaths.layer),
  Layer.provideMerge(NodeServices.layer),
);

const makeTempDir = Effect.gen(function* () {
  const fileSystem = yield* FileSystem.FileSystem;
  return yield* fileSystem.makeTempDirectoryScoped({
    prefix: "t3code-workspace-watcher-",
  });
});

describe("normalizeWatchPath", () => {
  const fakePath = {
    isAbsolute: (input: string) => input.startsWith("/"),
    relative: (from: string, to: string) =>
      to.startsWith(`${from}/`) ? to.slice(from.length + 1) : "..",
    normalize: (input: string) => input,
  } as Path.Path;

  it("keeps workspace-relative paths and normalizes separators", () => {
    expect(WorkspaceWatcher.normalizeWatchPath(fakePath, "/repo", "src\\index.ts")).toBe(
      "src/index.ts",
    );
  });

  it("resolves absolute event paths against the workspace root", () => {
    expect(WorkspaceWatcher.normalizeWatchPath(fakePath, "/repo", "/repo/src/index.ts")).toBe(
      "src/index.ts",
    );
  });

  it("drops paths outside the workspace root", () => {
    expect(WorkspaceWatcher.normalizeWatchPath(fakePath, "/repo", "/elsewhere/file.ts")).toBe(null);
  });

  it("drops ignored high-churn directories", () => {
    expect(
      WorkspaceWatcher.normalizeWatchPath(fakePath, "/repo", "node_modules/pkg/index.js"),
    ).toBe(null);
    expect(WorkspaceWatcher.normalizeWatchPath(fakePath, "/repo", ".git/index.lock")).toBe(null);
  });
});

describe("coalesceWatchPaths", () => {
  it("dedupes paths within a batch", () => {
    expect(WorkspaceWatcher.coalesceWatchPaths(["a.ts", "b.ts", "a.ts"])).toEqual({
      _tag: "changes",
      paths: ["a.ts", "b.ts"],
    });
  });

  it("returns null for empty batches", () => {
    expect(WorkspaceWatcher.coalesceWatchPaths([])).toBe(null);
  });

  it("degrades oversized batches to overflow", () => {
    const paths = Array.from({ length: 513 }, (_, index) => `file-${index}.ts`);
    expect(WorkspaceWatcher.coalesceWatchPaths(paths)).toEqual({ _tag: "overflow" });
  });
});

it.layer(TestLayer, { excludeTestServices: true })("WorkspaceWatcherLive", (it) => {
  describe("subscribe", () => {
    it.effect("streams coalesced change events for file writes", () =>
      Effect.gen(function* () {
        const workspaceWatcher = yield* WorkspaceWatcher.WorkspaceWatcher;
        const fileSystem = yield* FileSystem.FileSystem;
        const path = yield* Path.Path;
        const cwd = yield* makeTempDir;

        // macOS also reports events on the watched directory itself; wait
        // specifically for the batch that names the written file.
        const matchingEvent = yield* workspaceWatcher.subscribe({ cwd }).pipe(
          Stream.filter((event) => event._tag === "changes" && event.paths.includes("src-file.ts")),
          Stream.take(1),
          Stream.runCollect,
          Effect.forkChild,
        );
        // Give the watcher a beat to register before producing events.
        yield* Effect.sleep("250 millis");
        yield* fileSystem
          .writeFileString(path.join(cwd, "src-file.ts"), "export {};\n")
          .pipe(Effect.orDie);

        const events = yield* Fiber.join(matchingEvent).pipe(Effect.timeout("10 seconds"));

        expect(events).toHaveLength(1);
      }),
    );

    it.effect("rejects workspace roots that do not exist", () =>
      Effect.gen(function* () {
        const workspaceWatcher = yield* WorkspaceWatcher.WorkspaceWatcher;
        const path = yield* Path.Path;
        const cwd = yield* makeTempDir;

        const error = yield* workspaceWatcher
          .subscribe({ cwd: path.join(cwd, "does-not-exist") })
          .pipe(Stream.runDrain, Effect.flip);

        expect(error._tag).toBe("WorkspaceRootNotExistsError");
      }),
    );
  });
});
