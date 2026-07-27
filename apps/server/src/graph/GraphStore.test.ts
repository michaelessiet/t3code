/**
 * Guard-rail tests for the graph store.
 *
 * A GC whose happy path works and whose refusals do not is worse than no GC,
 * so most of what is here drives `evict` at inputs it must reject and then
 * asserts the directory is *still there*.
 */
import { describe, expect, it } from "@effect/vitest";
import * as NodeServices from "@effect/platform-node/NodeServices";
import { type GraphStoreEntry, type ProjectId } from "@t3tools/contracts";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Path from "effect/Path";
import * as Schema from "effect/Schema";
import * as TestClock from "effect/testing/TestClock";

import * as ServerConfig from "../config.ts";
import * as GraphStoreModule from "./GraphStore.ts";
import { graphStoreDirectoryName } from "./graphStoreKey.ts";

const PROJECT_ID = "6f1f9a4c-4f77-4b9e-9f3a-1d2e3f4a5b6c" as ProjectId;
const OTHER_PROJECT_ID = "1a2b3c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d" as ProjectId;

const isPathError = Schema.is(
  Schema.TaggedStruct("GraphStorePathError", { reason: Schema.String }),
);

const testLayer = GraphStoreModule.layer.pipe(
  Layer.provide(ServerConfig.layerTest(process.cwd(), { prefix: "t3-graph-store-test-" })),
  Layer.provideMerge(NodeServices.layer),
);

const entryFor = (branch: string | null): GraphStoreEntry => ({
  key: { projectId: PROJECT_ID, branch },
  workspaceRoot: "/repos/t3code",
  headSha: "0a1b2c3d",
  mode: "structural",
  graphifyVersion: "0.9.27",
  builtAt: 1_700_000_000_000,
  lastOpenedAt: 1_700_000_000_000,
  nodeCount: 12,
  edgeCount: 34,
  sizeBytes: 0,
});

/** Runs `effect` against a fresh store, failing the test on a defect. */
const withStore = <A, E>(
  body: (
    store: GraphStoreModule.GraphStore["Service"],
    fs: FileSystem.FileSystem,
    path: Path.Path,
  ) => Effect.Effect<A, E, FileSystem.FileSystem | Path.Path>,
) =>
  Effect.gen(function* () {
    const store = yield* GraphStoreModule.GraphStore;
    const fs = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    return yield* body(store, fs, path);
  }).pipe(Effect.provide(testLayer));

describe("GraphStore happy path", () => {
  it.effect("creates, reads back and evicts one entry without touching its sibling", () =>
    withStore((store, fs) =>
      Effect.gen(function* () {
        const main = yield* store.locate({ projectId: PROJECT_ID, branch: "main", headSha: null });
        const feature = yield* store.locate({
          projectId: PROJECT_ID,
          branch: "feat/graph",
          headSha: null,
        });

        yield* store.ensure(main);
        yield* store.ensure(feature);
        yield* store.writeEntry(main, entryFor("main"));
        yield* store.writeEntry(feature, entryFor("feat/graph"));

        // GRAPHIFY_OUT points at a directory that really exists.
        expect(yield* fs.exists(main.outDir)).toBe(true);
        expect(main.outDir.endsWith("graphify-out")).toBe(true);

        expect((yield* store.readEntry(main))?.key.branch).toBe("main");

        expect(yield* store.evict(main, "test")).toBe(true);
        expect(yield* fs.exists(main.entryDir)).toBe(false);
        expect(yield* fs.exists(feature.entryDir)).toBe(true);
        expect((yield* store.readEntry(feature))?.key.branch).toBe("feat/graph");
      }),
    ),
  );

  it.effect("stamps lastOpenedAt on touch and leaves the rest alone", () =>
    withStore((store) =>
      Effect.gen(function* () {
        const location = yield* store.locate({
          projectId: PROJECT_ID,
          branch: "main",
          headSha: null,
        });
        yield* store.ensure(location);
        yield* store.writeEntry(location, entryFor("main"));

        // `it.effect` starts the clock at 0, so give it a value that could not
        // have come from the entry we just wrote.
        yield* TestClock.setTime(1_800_000_000_000);
        yield* store.touch(location);

        const entry = yield* store.readEntry(location);
        expect(entry?.lastOpenedAt).toBe(1_800_000_000_000);
        expect(entry?.builtAt).toBe(1_700_000_000_000);
        expect(entry?.nodeCount).toBe(12);
      }),
    ),
  );

  it.effect("touching an entry with no metadata is a no-op, not a crash", () =>
    withStore((store) =>
      Effect.gen(function* () {
        const location = yield* store.locate({
          projectId: PROJECT_ID,
          branch: "main",
          headSha: null,
        });
        yield* store.ensure(location);
        yield* store.touch(location);
        expect(yield* store.readEntry(location)).toBeNull();
      }),
    ),
  );

  it.effect("lists real entries and skips foreign directories", () =>
    withStore((store, fs, path) =>
      Effect.gen(function* () {
        const location = yield* store.locate({
          projectId: PROJECT_ID,
          branch: "main",
          headSha: null,
        });
        yield* store.ensure(location);
        yield* store.writeEntry(location, entryFor("main"));

        // Things a user, another tool, or an older version might leave behind.
        yield* fs.makeDirectory(path.join(store.root, "not-a-project-id"), { recursive: true });
        yield* fs.makeDirectory(path.join(store.root, PROJECT_ID, "stray"), { recursive: true });

        const listings = yield* store.list;
        expect(listings).toHaveLength(1);
        expect(listings[0]?.entry?.key.branch).toBe("main");
        // `list` recovers the branch from meta.json, not from the slug.
        expect(listings[0]?.location.branch).toBe("main");
      }),
    ),
  );

  it.effect("measures the bytes an entry occupies", () =>
    withStore((store, fs, path) =>
      Effect.gen(function* () {
        const location = yield* store.locate({
          projectId: PROJECT_ID,
          branch: "main",
          headSha: null,
        });
        yield* store.ensure(location);
        yield* fs.writeFileString(path.join(location.outDir, "graph.json"), "x".repeat(1024));
        yield* fs.makeDirectory(path.join(location.outDir, "cache"), { recursive: true });
        yield* fs.writeFileString(path.join(location.outDir, "cache", "a.bin"), "y".repeat(512));

        expect(yield* store.measure(location)).toBeGreaterThanOrEqual(1536);
      }),
    ),
  );

  it.effect("drops a whole project directory and nothing else", () =>
    withStore((store, fs) =>
      Effect.gen(function* () {
        const mine = yield* store.locate({ projectId: PROJECT_ID, branch: "main", headSha: null });
        const theirs = yield* store.locate({
          projectId: OTHER_PROJECT_ID,
          branch: "main",
          headSha: null,
        });
        yield* store.ensure(mine);
        yield* store.ensure(theirs);

        expect(yield* store.evictProject(PROJECT_ID, "project deleted")).toBe(true);
        expect(yield* fs.exists(mine.entryDir)).toBe(false);
        expect(yield* fs.exists(theirs.entryDir)).toBe(true);
      }),
    ),
  );
});

describe("GraphStore refuses to delete", () => {
  it.effect("a location whose project segment is a traversal", () =>
    withStore((store, fs, path) =>
      Effect.gen(function* () {
        const victim = path.join(store.root, "..", "victim");
        yield* fs.makeDirectory(victim, { recursive: true });

        const location = yield* store.locate({
          projectId: PROJECT_ID,
          branch: "main",
          headSha: null,
        });
        const result = yield* Effect.result(
          store.evict(
            { ...location, projectId: "../.." as ProjectId, entryDir: victim },
            "hostile",
          ),
        );

        expect(result._tag).toBe("Failure");
        expect(yield* fs.exists(victim)).toBe(true);
      }),
    ),
  );

  it.effect("a location whose entry segment is a traversal", () =>
    withStore((store, fs, path) =>
      Effect.gen(function* () {
        const projectDir = path.join(store.root, PROJECT_ID);
        yield* fs.makeDirectory(projectDir, { recursive: true });

        const location = yield* store.locate({
          projectId: PROJECT_ID,
          branch: "main",
          headSha: null,
        });
        const result = yield* Effect.result(
          store.evict({ ...location, directoryName: ".." }, "hostile"),
        );

        expect(result._tag).toBe("Failure");
        expect(yield* fs.exists(projectDir)).toBe(true);
      }),
    ),
  );

  it.effect("an entry name it did not mint", () =>
    withStore((store, fs, path) =>
      Effect.gen(function* () {
        const handWritten = path.join(store.root, PROJECT_ID, "important-data");
        yield* fs.makeDirectory(handWritten, { recursive: true });

        const location = yield* store.locate({
          projectId: PROJECT_ID,
          branch: "main",
          headSha: null,
        });
        const result = yield* Effect.result(
          store.evict(
            { ...location, directoryName: "important-data", entryDir: handWritten },
            "hostile",
          ),
        );

        expect(result._tag).toBe("Failure");
        expect(yield* fs.exists(handWritten)).toBe(true);
      }),
    ),
  );

  it.effect("through a symlinked entry, leaving the link target intact", () =>
    withStore((store, fs, path) =>
      Effect.gen(function* () {
        // Somewhere outside the store that the user would very much like to keep.
        const outside = yield* fs.makeTempDirectoryScoped({ prefix: "t3-graph-store-outside-" });
        yield* fs.writeFileString(path.join(outside, "precious.txt"), "keep me");

        // An entry directory that is really a link to it.
        const directoryName = graphStoreDirectoryName({ branch: "main", headSha: null });
        yield* fs.makeDirectory(path.join(store.root, PROJECT_ID), { recursive: true });
        yield* fs.symlink(outside, path.join(store.root, PROJECT_ID, directoryName));

        const location = yield* store.locate({
          projectId: PROJECT_ID,
          branch: "main",
          headSha: null,
        });
        const result = yield* Effect.result(store.evict(location, "sweep"));

        expect(result._tag).toBe("Failure");
        if (result._tag === "Failure") {
          expect(isPathError(result.failure)).toBe(true);
        }
        expect(yield* fs.exists(path.join(outside, "precious.txt"))).toBe(true);
        // Scoped so the link target is cleaned up with the test, not left in
        // the OS temp directory.
      }).pipe(Effect.scoped),
    ),
  );

  it.effect("the store root itself", () =>
    withStore((store, fs) =>
      Effect.gen(function* () {
        yield* fs.makeDirectory(store.root, { recursive: true });
        const result = yield* Effect.result(store.evictProject("", "hostile"));

        expect(result._tag).toBe("Failure");
        expect(yield* fs.exists(store.root)).toBe(true);
      }),
    ),
  );

  it.effect("a project directory that is not a project id", () =>
    withStore((store, fs, path) =>
      Effect.gen(function* () {
        const foreign = path.join(store.root, "not-a-project-id");
        yield* fs.makeDirectory(foreign, { recursive: true });

        const result = yield* Effect.result(store.evictProject("not-a-project-id", "sweep"));

        expect(result._tag).toBe("Failure");
        expect(yield* fs.exists(foreign)).toBe(true);
      }),
    ),
  );

  it.effect("and refuses to locate a non-UUID project id at all", () =>
    withStore((store) =>
      Effect.gen(function* () {
        const result = yield* Effect.result(
          store.locate({ projectId: "../../etc" as ProjectId, branch: "main", headSha: null }),
        );
        expect(result._tag).toBe("Failure");
      }),
    ),
  );
});
