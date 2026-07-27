/**
 * Sweep tests.
 *
 * The store's own tests cover `evict` refusing bad paths; these cover the
 * layer above it — *deciding* what to remove. Every case is driven against a
 * real temp store with stubbed projections, git and settings, and asserts on
 * what is still on disk afterwards rather than on the report, because the
 * report is what the code believes and the directory is what actually
 * happened.
 */
import { describe, expect, it } from "@effect/vitest";
import * as NodeServices from "@effect/platform-node/NodeServices";
import {
  DEFAULT_SERVER_SETTINGS,
  GitCommandError,
  type GraphStoreEntry,
  type KnowledgeGraphSettings,
  type OrchestrationProjectShell,
  type ProjectId,
  type ServerSettings,
} from "@t3tools/contracts";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Option from "effect/Option";
import * as TestClock from "effect/testing/TestClock";

import * as ServerConfig from "../config.ts";
import { ProjectionSnapshotQuery } from "../orchestration/Services/ProjectionSnapshotQuery.ts";
import { ServerSettingsService } from "../serverSettings.ts";
import * as GitVcsDriver from "../vcs/GitVcsDriver.ts";
import { GRAPHIFY_PINNED_VERSION } from "./graphifyDetection.ts";
import { GraphStore } from "./GraphStore.ts";
import * as GraphStoreModule from "./GraphStore.ts";
import {
  GraphStoreSweep,
  isExpired,
  layer as sweepLayer,
  selectOverBudget,
} from "./GraphStoreSweep.ts";
import * as WorkspaceGraph from "./WorkspaceGraph.ts";

const PROJECT_ID = "6f1f9a4c-4f77-4b9e-9f3a-1d2e3f4a5b6c" as ProjectId;
const OTHER_PROJECT_ID = "1a2b3c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d" as ProjectId;

const DAY_MS = 24 * 60 * 60 * 1000;
/** A plausible wall-clock instant; `it.effect` otherwise starts the clock at 0. */
const NOW = 1_800_000_000_000;

/**
 * Mutable so a test can change the world *between* sweeps — which is the only
 * way to exercise the two-pass branch rule and idempotence.
 */
interface World {
  /** Absent id = soft-deleted. */
  projects: Map<ProjectId, string>;
  /** Absent id = git could not answer. */
  branches: Map<ProjectId, ReadonlyArray<string>>;
  knowledgeGraph: Partial<KnowledgeGraphSettings>;
}

const entryFor = (input: {
  readonly projectId: ProjectId;
  readonly branch: string | null;
  readonly lastOpenedAt?: number;
  readonly graphifyVersion?: string;
  readonly sizeBytes?: number;
}): GraphStoreEntry => ({
  key: { projectId: input.projectId, branch: input.branch },
  workspaceRoot: "/repos/t3code",
  headSha: "0a1b2c3d",
  mode: "structural",
  graphifyVersion: input.graphifyVersion ?? GRAPHIFY_PINNED_VERSION,
  builtAt: NOW - DAY_MS,
  lastOpenedAt: input.lastOpenedAt ?? NOW - DAY_MS,
  nodeCount: 12,
  edgeCount: 34,
  sizeBytes: input.sizeBytes ?? 1024,
});

const shellFor = (projectId: ProjectId, workspaceRoot: string): OrchestrationProjectShell => ({
  id: projectId,
  title: "t3code",
  workspaceRoot,
  repositoryIdentity: null,
  defaultModelSelection: null,
  scripts: [],
  createdAt: "2026-01-01T00:00:00.000Z",
  updatedAt: "2026-01-01T00:00:00.000Z",
});

const layersFor = (world: World) => {
  const projections = Layer.mock(ProjectionSnapshotQuery)({
    getProjectShellById: (projectId) => {
      const workspaceRoot = world.projects.get(projectId);
      return Effect.succeed(
        workspaceRoot === undefined
          ? Option.none()
          : Option.some(shellFor(projectId, workspaceRoot)),
      );
    },
  });

  const git = Layer.mock(GitVcsDriver.GitVcsDriver)({
    listLocalBranchNames: (cwd) => {
      const entry = [...world.projects.entries()].find(([, root]) => root === cwd);
      const names = entry === undefined ? undefined : world.branches.get(entry[0]);
      return names === undefined
        ? Effect.fail(
            new GitCommandError({
              operation: "listLocalBranchNames",
              command: "git branch",
              cwd,
              exitCode: 128,
              detail: "not a git repository",
            }),
          )
        : Effect.succeed([...names]);
    },
  });

  const settings = Layer.mock(ServerSettingsService)({
    getSettings: Effect.sync(
      (): ServerSettings => ({
        ...DEFAULT_SERVER_SETTINGS,
        knowledgeGraph: { ...DEFAULT_SERVER_SETTINGS.knowledgeGraph, ...world.knowledgeGraph },
      }),
    ),
  });

  return sweepLayer.pipe(
    Layer.provideMerge(
      Layer.mergeAll(GraphStoreModule.layer, WorkspaceGraph.layer, projections, git, settings),
    ),
    Layer.provide(ServerConfig.layerTest(process.cwd(), { prefix: "t3-graph-sweep-test-" })),
    Layer.provideMerge(NodeServices.layer),
  );
};

/**
 * Seeds a store, then hands the test the sweep and the store.
 *
 * Seeding goes through `GraphStore` rather than writing files directly, so the
 * fixtures are exactly the shape the sweep will meet in production.
 */
const withSweep = <A, E>(
  world: World,
  seed: ReadonlyArray<{
    readonly projectId: ProjectId;
    readonly branch: string | null;
    readonly entry: GraphStoreEntry | null;
  }>,
  body: (context: {
    readonly sweep: GraphStoreSweep["Service"];
    readonly store: GraphStore["Service"];
    readonly fs: FileSystem.FileSystem;
    readonly dirs: ReadonlyMap<string, string>;
  }) => Effect.Effect<A, E, FileSystem.FileSystem>,
) =>
  Effect.gen(function* () {
    yield* TestClock.setTime(NOW);
    const sweep = yield* GraphStoreSweep;
    const store = yield* GraphStore;
    const fs = yield* FileSystem.FileSystem;

    const dirs = new Map<string, string>();
    for (const item of seed) {
      const location = yield* store.locate({
        projectId: item.projectId,
        branch: item.branch,
        headSha: "0a1b2c3d",
      });
      yield* store.ensure(location);
      if (item.entry !== null) yield* store.writeEntry(location, item.entry);
      dirs.set(`${item.projectId}/${item.branch ?? "detached"}`, location.entryDir);
    }

    return yield* body({ sweep, store, fs, dirs });
  }).pipe(Effect.provide(layersFor(world)));

describe("isExpired", () => {
  it("keeps an entry opened yesterday and drops one opened 90 days ago", () => {
    const at = (lastOpenedAt: number) =>
      isExpired({
        entry: entryFor({ projectId: PROJECT_ID, branch: "main", lastOpenedAt }),
        retentionDays: 60,
        now: NOW,
      });

    expect(at(NOW - DAY_MS)).toBe(false);
    expect(at(NOW - 90 * DAY_MS)).toBe(true);
    // Exactly on the boundary survives: retention is "older than N days".
    expect(at(NOW - 60 * DAY_MS)).toBe(false);
    expect(at(NOW - 60 * DAY_MS - 1)).toBe(true);
  });

  it("never expires when retention is disabled or metadata is missing", () => {
    expect(
      isExpired({
        entry: entryFor({ projectId: PROJECT_ID, branch: "main", lastOpenedAt: 0 }),
        retentionDays: 0,
        now: NOW,
      }),
    ).toBe(false);
    // A half-built entry has no age, and guessing "old" would delete a build
    // that is running right now.
    expect(isExpired({ entry: null, retentionDays: 60, now: NOW })).toBe(false);
  });
});

describe("selectOverBudget", () => {
  const entry = (directoryName: string, lastOpenedAt: number | null, sizeBytes: number) => ({
    directoryName,
    lastOpenedAt,
    sizeBytes,
  });

  it("removes least-recently-opened first, and only enough to fit", () => {
    const three = [entry("new", 30, 60), entry("old", 10, 60), entry("mid", 20, 60)];

    // 180 bytes against 130: dropping the oldest is enough.
    expect(selectOverBudget({ budgetBytes: 130, entries: three })).toEqual(["old"]);
    // 180 against 100: it keeps going, still oldest-first, and still stops as
    // soon as the total fits rather than emptying the store.
    expect(selectOverBudget({ budgetBytes: 100, entries: three })).toEqual(["old", "mid"]);
  });

  it("does nothing under budget, and sorts metadata-less entries oldest", () => {
    expect(selectOverBudget({ budgetBytes: 100, entries: [entry("a", 10, 40)] })).toEqual([]);
    expect(
      selectOverBudget({
        budgetBytes: 50,
        entries: [entry("known", 10, 40), entry("unknown", null, 40)],
      }),
    ).toEqual(["unknown"]);
    // A zero budget is "unbounded", not "delete everything".
    expect(selectOverBudget({ budgetBytes: 0, entries: [entry("a", 10, 40)] })).toEqual([]);
  });
});

describe("GraphStoreSweep", () => {
  it.effect("drops a soft-deleted project and leaves a live one alone", () => {
    const world: World = {
      projects: new Map([[OTHER_PROJECT_ID, process.cwd()]]),
      branches: new Map([[OTHER_PROJECT_ID, ["main"]]]),
      knowledgeGraph: {},
    };

    return withSweep(
      world,
      [
        {
          projectId: PROJECT_ID,
          branch: "main",
          entry: entryFor({ projectId: PROJECT_ID, branch: "main" }),
        },
        {
          projectId: OTHER_PROJECT_ID,
          branch: "main",
          entry: entryFor({ projectId: OTHER_PROJECT_ID, branch: "main" }),
        },
      ],
      ({ sweep, fs, dirs }) =>
        Effect.gen(function* () {
          const report = yield* sweep.sweep;

          expect(report.evicted).toBe(1);
          expect(yield* fs.exists(dirs.get(`${PROJECT_ID}/main`) ?? "")).toBe(false);
          expect(yield* fs.exists(dirs.get(`${OTHER_PROJECT_ID}/main`) ?? "")).toBe(true);
        }),
    );
  });

  it.effect("drops a project whose workspace root is gone from disk", () => {
    const world: World = {
      projects: new Map([[PROJECT_ID, "/definitely/not/a/real/checkout"]]),
      branches: new Map(),
      knowledgeGraph: {},
    };

    return withSweep(
      world,
      [
        {
          projectId: PROJECT_ID,
          branch: "main",
          entry: entryFor({ projectId: PROJECT_ID, branch: "main" }),
        },
      ],
      ({ sweep, fs, dirs }) =>
        Effect.gen(function* () {
          yield* sweep.sweep;
          expect(yield* fs.exists(dirs.get(`${PROJECT_ID}/main`) ?? "")).toBe(false);
        }),
    );
  });

  it.effect("needs two consecutive sweeps before dropping a missing branch", () => {
    const world: World = {
      projects: new Map([[PROJECT_ID, process.cwd()]]),
      branches: new Map([[PROJECT_ID, ["main", "feat/graph"]]]),
      knowledgeGraph: {},
    };

    return withSweep(
      world,
      [
        {
          projectId: PROJECT_ID,
          branch: "feat/graph",
          entry: entryFor({ projectId: PROJECT_ID, branch: "feat/graph" }),
        },
      ],
      ({ sweep, fs, dirs }) =>
        Effect.gen(function* () {
          const entryDir = dirs.get(`${PROJECT_ID}/feat/graph`) ?? "";

          yield* sweep.sweep;
          expect(yield* fs.exists(entryDir)).toBe(true);

          // The branch disappears — a rebase would look exactly like this.
          world.branches.set(PROJECT_ID, ["main"]);
          expect((yield* sweep.sweep).evicted).toBe(0);
          expect(yield* fs.exists(entryDir)).toBe(true);

          expect((yield* sweep.sweep).evicted).toBe(1);
          expect(yield* fs.exists(entryDir)).toBe(false);
        }),
    );
  });

  it.effect("forgives a branch that comes back before the second sweep", () => {
    const world: World = {
      projects: new Map([[PROJECT_ID, process.cwd()]]),
      branches: new Map([[PROJECT_ID, ["main"]]]),
      knowledgeGraph: {},
    };

    return withSweep(
      world,
      [
        {
          projectId: PROJECT_ID,
          branch: "feat/graph",
          entry: entryFor({ projectId: PROJECT_ID, branch: "feat/graph" }),
        },
      ],
      ({ sweep, fs, dirs }) =>
        Effect.gen(function* () {
          const entryDir = dirs.get(`${PROJECT_ID}/feat/graph`) ?? "";

          yield* sweep.sweep;
          world.branches.set(PROJECT_ID, ["main", "feat/graph"]);
          yield* sweep.sweep;
          // …and gone again: the strike must have been cleared, so this is
          // strike one rather than strike two.
          world.branches.set(PROJECT_ID, ["main"]);
          expect((yield* sweep.sweep).evicted).toBe(0);
          expect(yield* fs.exists(entryDir)).toBe(true);
        }),
    );
  });

  it.effect("keeps everything when git cannot answer", () => {
    const world: World = {
      projects: new Map([[PROJECT_ID, process.cwd()]]),
      // No branch list at all — `listLocalBranchNames` fails.
      branches: new Map(),
      knowledgeGraph: {},
    };

    return withSweep(
      world,
      [
        {
          projectId: PROJECT_ID,
          branch: "main",
          entry: entryFor({ projectId: PROJECT_ID, branch: "main" }),
        },
      ],
      ({ sweep, fs, dirs }) =>
        Effect.gen(function* () {
          yield* sweep.sweep;
          yield* sweep.sweep;
          expect(yield* fs.exists(dirs.get(`${PROJECT_ID}/main`) ?? "")).toBe(true);
        }),
    );
  });

  it.effect("drops an entry past retention and keeps a recent one", () => {
    const world: World = {
      projects: new Map([[PROJECT_ID, process.cwd()]]),
      branches: new Map([[PROJECT_ID, ["main", "stale"]]]),
      knowledgeGraph: { retentionDays: 60 },
    };

    return withSweep(
      world,
      [
        {
          projectId: PROJECT_ID,
          branch: "main",
          entry: entryFor({ projectId: PROJECT_ID, branch: "main", lastOpenedAt: NOW - DAY_MS }),
        },
        {
          projectId: PROJECT_ID,
          branch: "stale",
          entry: entryFor({
            projectId: PROJECT_ID,
            branch: "stale",
            lastOpenedAt: NOW - 90 * DAY_MS,
          }),
        },
      ],
      ({ sweep, fs, dirs }) =>
        Effect.gen(function* () {
          const report = yield* sweep.sweep;

          expect(report.evicted).toBe(1);
          expect(yield* fs.exists(dirs.get(`${PROJECT_ID}/main`) ?? "")).toBe(true);
          expect(yield* fs.exists(dirs.get(`${PROJECT_ID}/stale`) ?? "")).toBe(false);
        }),
    );
  });

  it.effect("drops an entry built by a different graphify", () => {
    const world: World = {
      projects: new Map([[PROJECT_ID, process.cwd()]]),
      branches: new Map([[PROJECT_ID, ["main"]]]),
      knowledgeGraph: {},
    };

    return withSweep(
      world,
      [
        {
          projectId: PROJECT_ID,
          branch: "main",
          entry: entryFor({ projectId: PROJECT_ID, branch: "main", graphifyVersion: "0.0.1" }),
        },
      ],
      ({ sweep, fs, dirs }) =>
        Effect.gen(function* () {
          expect((yield* sweep.sweep).evicted).toBe(1);
          expect(yield* fs.exists(dirs.get(`${PROJECT_ID}/main`) ?? "")).toBe(false);
        }),
    );
  });

  it.effect("evicts least-recently-opened entries until the store fits its budget", () => {
    const world: World = {
      projects: new Map([[PROJECT_ID, process.cwd()]]),
      branches: new Map([[PROJECT_ID, ["main", "old"]]]),
      // One megabyte of budget against two entries claiming 0.75 MB each.
      knowledgeGraph: { maxStoreMegabytes: 1 },
    };
    const threeQuarters = Math.floor(0.75 * 1024 * 1024);

    return withSweep(
      world,
      [
        {
          projectId: PROJECT_ID,
          branch: "main",
          entry: entryFor({
            projectId: PROJECT_ID,
            branch: "main",
            lastOpenedAt: NOW - 1000,
            sizeBytes: threeQuarters,
          }),
        },
        {
          projectId: PROJECT_ID,
          branch: "old",
          entry: entryFor({
            projectId: PROJECT_ID,
            branch: "old",
            lastOpenedAt: NOW - 10 * DAY_MS,
            sizeBytes: threeQuarters,
          }),
        },
      ],
      ({ sweep, fs, dirs }) =>
        Effect.gen(function* () {
          expect((yield* sweep.sweep).evicted).toBe(1);
          expect(yield* fs.exists(dirs.get(`${PROJECT_ID}/main`) ?? "")).toBe(true);
          expect(yield* fs.exists(dirs.get(`${PROJECT_ID}/old`) ?? "")).toBe(false);
        }),
    );
  });

  it.effect("is idempotent — a second pass over a healthy store removes nothing", () => {
    const world: World = {
      projects: new Map([[PROJECT_ID, process.cwd()]]),
      branches: new Map([[PROJECT_ID, ["main"]]]),
      knowledgeGraph: {},
    };

    return withSweep(
      world,
      [
        {
          projectId: PROJECT_ID,
          branch: "main",
          entry: entryFor({ projectId: PROJECT_ID, branch: "main" }),
        },
      ],
      ({ sweep, fs, dirs }) =>
        Effect.gen(function* () {
          expect((yield* sweep.sweep).evicted).toBe(0);
          expect((yield* sweep.sweep).evicted).toBe(0);
          expect(yield* fs.exists(dirs.get(`${PROJECT_ID}/main`) ?? "")).toBe(true);
        }),
    );
  });

  it.effect("leaves a half-built entry with no metadata alone", () => {
    const world: World = {
      projects: new Map([[PROJECT_ID, process.cwd()]]),
      branches: new Map([[PROJECT_ID, ["main"]]]),
      knowledgeGraph: { retentionDays: 1 },
    };

    return withSweep(
      world,
      [{ projectId: PROJECT_ID, branch: "main", entry: null }],
      ({ sweep, fs, dirs }) =>
        Effect.gen(function* () {
          expect((yield* sweep.sweep).evicted).toBe(0);
          expect(yield* fs.exists(dirs.get(`${PROJECT_ID}/main`) ?? "")).toBe(true);
        }),
    );
  });
});
