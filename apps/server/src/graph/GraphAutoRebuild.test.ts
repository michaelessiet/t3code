/**
 * Auto-rebuild tests.
 *
 * The interesting behaviour is all refusal: which changes are *not* turned into
 * builds. So every case drives a real store and a fake watcher, pushes a change
 * event, lets the quiet period elapse, and asserts on what reached the build
 * worker — an empty list being the expected answer more often than not.
 *
 * `GraphStore` is real (against a temp directory) because the decision to skip
 * hinges on whether an entry exists on disk for the branch checked out *now*,
 * and a mock of that is a mock of the thing under test.
 */
import { describe, expect, it } from "@effect/vitest";
import * as NodeServices from "@effect/platform-node/NodeServices";
import {
  DEFAULT_SERVER_SETTINGS,
  GraphRuntimeUnavailableError,
  type GraphBuildStatus,
  type GraphStoreEntry,
  type KnowledgeGraphSettings,
  type ProjectId,
  type ProjectWatchStreamEvent,
  type ServerSettings,
  GraphWorkspaceUnknownError,
} from "@t3tools/contracts";
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Queue from "effect/Queue";
import * as Ref from "effect/Ref";
import * as Stream from "effect/Stream";
import * as TestClock from "effect/testing/TestClock";

import * as ServerConfig from "../config.ts";
import { ServerSettingsService } from "../serverSettings.ts";
import { WorkspaceWatcher } from "../workspace/WorkspaceWatcher.ts";
import { GraphAutoRebuild, makeLayer } from "./GraphAutoRebuild.ts";
import { GraphBuildWorker, type GraphBuildRequest } from "./GraphBuildWorker.ts";
import { GRAPHIFY_PINNED_VERSION } from "./graphifyDetection.ts";
import { GraphifyRuntime } from "./GraphifyRuntime.ts";
import { GraphStore } from "./GraphStore.ts";
import * as GraphStoreModule from "./GraphStore.ts";
import { GraphWorkspaceResolver } from "./GraphWorkspaceResolver.ts";

const PROJECT_ID = "6f1f9a4c-4f77-4b9e-9f3a-1d2e3f4a5b6c" as ProjectId;
const HEAD_SHA = "0a1b2c3d";
const WORKSPACE_ROOT = "/repos/t3code";
const QUIET_PERIOD_MS = 1_000;

const IDLE_STATUS: GraphBuildStatus = {
  state: "idle",
  mode: null,
  message: null,
  startedAt: null,
  finishedAt: null,
  detail: null,
};

const entryFor = (branch: string | null): GraphStoreEntry => ({
  key: { projectId: PROJECT_ID, branch },
  workspaceRoot: WORKSPACE_ROOT,
  headSha: HEAD_SHA,
  mode: "structural",
  graphifyVersion: GRAPHIFY_PINNED_VERSION,
  builtAt: 1,
  lastOpenedAt: 1,
  nodeCount: 12,
  edgeCount: 34,
  sizeBytes: 1024,
});

/** Mutable so a test can change the world between reconciliations. */
interface World {
  knowledgeGraph: Partial<KnowledgeGraphSettings>;
  /** Branch the resolver reports for `WORKSPACE_ROOT`; `null` = detached. */
  branch: string | null;
  /** False = the resolver cannot attribute the directory to a project. */
  resolvable: boolean;
  runtimeReady: boolean;
  buildStatus: GraphBuildStatus;
}

const layersFor = (
  world: World,
  requests: Ref.Ref<ReadonlyArray<GraphBuildRequest>>,
  events: Queue.Queue<ProjectWatchStreamEvent>,
) => {
  const settings = Layer.mock(ServerSettingsService)({
    getSettings: Effect.sync(
      (): ServerSettings => ({
        ...DEFAULT_SERVER_SETTINGS,
        knowledgeGraph: { ...DEFAULT_SERVER_SETTINGS.knowledgeGraph, ...world.knowledgeGraph },
      }),
    ),
  });

  // Shaped like the real resolver rather than returning a canned location:
  // the branch is read at resolve time, which is exactly the behaviour the
  // branch-switch test depends on.
  const resolver = Layer.effect(
    GraphWorkspaceResolver,
    Effect.gen(function* () {
      const store = yield* GraphStore;
      return GraphWorkspaceResolver.of({
        resolve: (cwd) =>
          Effect.gen(function* () {
            if (!world.resolvable) return yield* new GraphWorkspaceUnknownError({ cwd });
            const location = yield* store.locate({
              projectId: PROJECT_ID,
              branch: world.branch,
              headSha: HEAD_SHA,
            });
            return {
              projectId: PROJECT_ID,
              workspaceRoot: cwd,
              branch: world.branch,
              headSha: HEAD_SHA,
              location,
            };
          }),
      });
    }),
  );

  const runtime = Layer.mock(GraphifyRuntime)({
    resolve: Effect.suspend(() =>
      world.runtimeReady
        ? Effect.succeed({
            command: "graphify",
            args: [],
            source: "system" as const,
            version: GRAPHIFY_PINNED_VERSION,
          })
        : Effect.fail(new GraphRuntimeUnavailableError({ detail: "not installed" })),
    ),
  });

  const worker = Layer.mock(GraphBuildWorker)({
    request: (input) =>
      Ref.update(requests, (previous) => [...previous, input]).pipe(Effect.as(IDLE_STATUS)),
    statusFor: () => Effect.sync(() => world.buildStatus),
  });

  const watcher = Layer.mock(WorkspaceWatcher)({
    subscribe: () => Stream.fromQueue(events),
  });

  // The store goes *underneath* the rest rather than beside it: the resolver
  // stub reads it, and a sibling in the same `mergeAll` would stand up a
  // second store pointed at a different temp directory.
  const dependencies = Layer.mergeAll(resolver, runtime, worker, watcher, settings).pipe(
    Layer.provideMerge(GraphStoreModule.layer),
  );

  return makeLayer({ reconcileIntervalMs: 60_000, quietPeriodMs: QUIET_PERIOD_MS }).pipe(
    Layer.provideMerge(dependencies),
    Layer.provide(ServerConfig.layerTest(process.cwd(), { prefix: "t3-graph-auto-test-" })),
    Layer.provideMerge(NodeServices.layer),
  );
};

/**
 * One turn of the real event loop.
 *
 * `TestClock` virtualises Effect's clock, not Node's, so this still yields to
 * the platform — which is what the store's `readFile` needs to make progress.
 */
// Effect.sleep is virtualised by TestClock, which is the one thing this must not
// be: the point is to yield to the platform so the store's real file reads can
// make progress.
// @effect-diagnostics-next-line globalTimers:off - deliberately a platform timer, see above
const macrotask = Effect.promise(() => new Promise<void>((resolve) => setTimeout(resolve, 0)));

/**
 * Waits until a build request lands, or until it is clear none will.
 *
 * `rebuild` reads `meta.json` off disk before it decides, so unlike a
 * synchronous callback it does not finish within a fiber yield: sampling the
 * ref straight after `TestClock.adjust` reads it before the rebuild has run,
 * and the test sees zero requests whether or not the reactor works. The bound
 * is what keeps the refusal cases — where nothing is ever coming — from
 * hanging; they simply spend all of it, which is a few milliseconds.
 */
const settle = (requests: Ref.Ref<ReadonlyArray<GraphBuildRequest>>) =>
  Effect.gen(function* () {
    for (let attempt = 0; attempt < 50; attempt++) {
      if ((yield* Ref.get(requests)).length > 0) return;
      yield* macrotask;
    }
  });

/**
 * Pushes one change event and lets the debounce fire.
 *
 * The yields matter: the watch fiber has to be scheduled and pull the element
 * before its debounce timer exists, and a `TestClock.adjust` that lands first
 * would advance past a sleep that has not been registered yet — after which the
 * clock is frozen and the timer never completes.
 */
const pushChange = (
  events: Queue.Queue<ProjectWatchStreamEvent>,
  requests: Ref.Ref<ReadonlyArray<GraphBuildRequest>>,
) =>
  Effect.gen(function* () {
    yield* Queue.offer(events, { _tag: "changes", paths: ["src/ws.ts"] });
    yield* Effect.yieldNow;
    yield* Effect.yieldNow;
    yield* TestClock.adjust(Duration.millis(QUIET_PERIOD_MS * 2));
    yield* settle(requests);
  });

/**
 * Seeds the store, reconciles once, pushes a change, and elapses the quiet
 * period — the whole cycle every test needs — then hands back what was built.
 */
const runCycle = (
  world: World,
  seed: ReadonlyArray<{ readonly branch: string | null; readonly entry: GraphStoreEntry | null }>,
  after?: (context: { readonly world: World }) => Effect.Effect<void>,
) =>
  Effect.gen(function* () {
    const requests = yield* Ref.make<ReadonlyArray<GraphBuildRequest>>([]);
    const events = yield* Queue.unbounded<ProjectWatchStreamEvent>();

    return yield* Effect.gen(function* () {
      const store = yield* GraphStore;
      for (const item of seed) {
        const location = yield* store.locate({
          projectId: PROJECT_ID,
          branch: item.branch,
          headSha: HEAD_SHA,
        });
        yield* store.ensure(location);
        if (item.entry !== null) yield* store.writeEntry(location, item.entry);
      }

      const reactor = yield* GraphAutoRebuild;
      const watched = yield* reactor.reconcile;

      if (after !== undefined) yield* after({ world });

      yield* pushChange(events, requests);

      return { watched, requests: yield* Ref.get(requests) };
    }).pipe(Effect.scoped, Effect.provide(layersFor(world, requests, events)));
  });

const world = (overrides?: Partial<World>): World => ({
  knowledgeGraph: { enabled: true, autoRebuild: true },
  branch: "main",
  resolvable: true,
  runtimeReady: true,
  buildStatus: IDLE_STATUS,
  ...overrides,
});

describe("GraphAutoRebuild", () => {
  it.effect("rebuilds structurally after the watcher goes quiet", () =>
    Effect.gen(function* () {
      const result = yield* runCycle(world(), [{ branch: "main", entry: entryFor("main") }]);

      expect(result.watched).toEqual([WORKSPACE_ROOT]);
      expect(result.requests).toHaveLength(1);
      expect(result.requests[0]?.mode).toBe("structural");
      // A background refresh must never force a full re-extract: `graphify
      // update` is the whole point, and --force would turn every quiet moment
      // into a minutes-long rebuild.
      expect(result.requests[0]?.force).toBe(false);
      expect(result.requests[0]?.workspaceRoot).toBe(WORKSPACE_ROOT);
    }),
  );

  it.effect("watches nothing while auto-rebuild is off", () =>
    Effect.gen(function* () {
      const result = yield* runCycle(world({ knowledgeGraph: { enabled: true } }), [
        { branch: "main", entry: entryFor("main") },
      ]);

      expect(result.watched).toEqual([]);
      expect(result.requests).toEqual([]);
    }),
  );

  it.effect("watches nothing while the whole feature is off", () =>
    Effect.gen(function* () {
      const result = yield* runCycle(
        world({ knowledgeGraph: { enabled: false, autoRebuild: true } }),
        [{ branch: "main", entry: entryFor("main") }],
      );

      expect(result.watched).toEqual([]);
      expect(result.requests).toEqual([]);
    }),
  );

  it.effect("ignores a store with no built entry, rather than building one", () =>
    Effect.gen(function* () {
      // A half-built entry — a directory with no `meta.json` — is not a graph
      // the user has, so it is not one to keep fresh.
      const result = yield* runCycle(world(), [{ branch: "main", entry: null }]);

      expect(result.watched).toEqual([]);
      expect(result.requests).toEqual([]);
    }),
  );

  it.effect("does not build a branch that has no graph of its own", () =>
    Effect.gen(function* () {
      const state = world();
      const result = yield* runCycle(
        state,
        [{ branch: "main", entry: entryFor("main") }],
        // Checked out `feat/x` after the watch started. Its files changed, but
        // nobody has ever built a graph for it.
        ({ world: live }) => Effect.sync(() => (live.branch = "feat/x")),
      );

      expect(result.watched).toEqual([WORKSPACE_ROOT]);
      expect(result.requests).toEqual([]);
    }),
  );

  it.effect("stops watching once the setting is switched off", () =>
    Effect.gen(function* () {
      const requests = yield* Ref.make<ReadonlyArray<GraphBuildRequest>>([]);
      const events = yield* Queue.unbounded<ProjectWatchStreamEvent>();
      const state = world();

      const watched = yield* Effect.gen(function* () {
        const store = yield* GraphStore;
        const location = yield* store.locate({
          projectId: PROJECT_ID,
          branch: "main",
          headSha: HEAD_SHA,
        });
        yield* store.ensure(location);
        yield* store.writeEntry(location, entryFor("main"));

        const reactor = yield* GraphAutoRebuild;
        const first = yield* reactor.reconcile;
        state.knowledgeGraph = { enabled: true, autoRebuild: false };
        const second = yield* reactor.reconcile;

        yield* pushChange(events, requests);

        return { first, second, requests: yield* Ref.get(requests) };
      }).pipe(Effect.scoped, Effect.provide(layersFor(state, requests, events)));

      expect(watched.first).toEqual([WORKSPACE_ROOT]);
      expect(watched.second).toEqual([]);
      expect(watched.requests).toEqual([]);
    }),
  );

  it.effect("stays quiet when graphify is not installed", () =>
    Effect.gen(function* () {
      // Queueing a build that can only fail would put a failure banner in the
      // panel that no user action caused.
      const result = yield* runCycle(world({ runtimeReady: false }), [
        { branch: "main", entry: entryFor("main") },
      ]);

      expect(result.watched).toEqual([WORKSPACE_ROOT]);
      expect(result.requests).toEqual([]);
    }),
  );

  it.effect("does not stack a request onto a build already running", () =>
    Effect.gen(function* () {
      const result = yield* runCycle(
        world({ buildStatus: { ...IDLE_STATUS, state: "running", mode: "structural" } }),
        [{ branch: "main", entry: entryFor("main") }],
      );

      expect(result.requests).toEqual([]);
    }),
  );

  it.effect("skips a directory it cannot attribute to a project", () =>
    Effect.gen(function* () {
      const result = yield* runCycle(world({ resolvable: false }), [
        { branch: "main", entry: entryFor("main") },
      ]);

      expect(result.requests).toEqual([]);
    }),
  );

  // The consumer-recency gate: `lastOpenedAt` in the seeded entry is 1, and the
  // TestClock starts at 0, so how far the clock is advanced before the change
  // event decides whether the graph counts as recently consumed. The default
  // window is thirty minutes.

  it.effect("keeps full cadence while the graph is being read", () =>
    Effect.gen(function* () {
      const requests = yield* Ref.make<ReadonlyArray<GraphBuildRequest>>([]);
      const events = yield* Queue.unbounded<ProjectWatchStreamEvent>();
      const state = world();

      const result = yield* Effect.gen(function* () {
        const store = yield* GraphStore;
        const location = yield* store.locate({
          projectId: PROJECT_ID,
          branch: "main",
          headSha: HEAD_SHA,
        });
        yield* store.ensure(location);
        yield* store.writeEntry(location, entryFor("main"));

        const reactor = yield* GraphAutoRebuild;
        yield* reactor.reconcile;

        // Twenty-nine minutes since the last read: inside the window.
        yield* TestClock.adjust(Duration.minutes(29));
        yield* pushChange(events, requests);

        return { requests: yield* Ref.get(requests), dirty: yield* store.isDirty(location) };
      }).pipe(Effect.scoped, Effect.provide(layersFor(state, requests, events)));

      expect(result.requests).toHaveLength(1);
      expect(result.dirty).toBe(false);
    }),
  );

  it.effect("marks the graph dirty instead of rebuilding when nobody has read it lately", () =>
    Effect.gen(function* () {
      const requests = yield* Ref.make<ReadonlyArray<GraphBuildRequest>>([]);
      const events = yield* Queue.unbounded<ProjectWatchStreamEvent>();
      const state = world();

      const result = yield* Effect.gen(function* () {
        const store = yield* GraphStore;
        const location = yield* store.locate({
          projectId: PROJECT_ID,
          branch: "main",
          headSha: HEAD_SHA,
        });
        yield* store.ensure(location);
        yield* store.writeEntry(location, entryFor("main"));

        const reactor = yield* GraphAutoRebuild;
        yield* reactor.reconcile;

        // Thirty-one minutes since the last read: outside the window.
        yield* TestClock.adjust(Duration.minutes(31));
        yield* pushChange(events, requests);

        return { requests: yield* Ref.get(requests), dirty: yield* store.isDirty(location) };
      }).pipe(Effect.scoped, Effect.provide(layersFor(state, requests, events)));

      expect(result.requests).toEqual([]);
      expect(result.dirty).toBe(true);
    }),
  );

  it.effect("a rebuild inside the window settles a leftover dirty mark", () =>
    Effect.gen(function* () {
      const requests = yield* Ref.make<ReadonlyArray<GraphBuildRequest>>([]);
      const events = yield* Queue.unbounded<ProjectWatchStreamEvent>();
      const state = world();

      const result = yield* Effect.gen(function* () {
        const store = yield* GraphStore;
        const location = yield* store.locate({
          projectId: PROJECT_ID,
          branch: "main",
          headSha: HEAD_SHA,
        });
        yield* store.ensure(location);
        yield* store.writeEntry(location, entryFor("main"));
        // Debt left over from an earlier deferral (or a previous run — the
        // marker is a file, so a restart carries it over).
        yield* store.markDirty(location);

        const reactor = yield* GraphAutoRebuild;
        yield* reactor.reconcile;

        yield* pushChange(events, requests);

        return { requests: yield* Ref.get(requests), dirty: yield* store.isDirty(location) };
      }).pipe(Effect.scoped, Effect.provide(layersFor(state, requests, events)));

      expect(result.requests).toHaveLength(1);
      expect(result.dirty).toBe(false);
    }),
  );
});
