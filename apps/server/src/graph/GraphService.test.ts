/**
 * GraphService tests — the consume side of auto-rebuild's consumer gating.
 *
 * `GraphAutoRebuild` defers rebuilds for graphs nobody has read lately and
 * leaves a dirty mark in the store; the contract tested here is that the next
 * read serves the graph on disk immediately and queues exactly one background
 * refresh — and that every guard (setting off, build in flight, runtime
 * missing) leaves the mark in place rather than losing the debt.
 *
 * The store is real (a temp directory), same reasoning as
 * `GraphAutoRebuild.test.ts`: the dirty mark *is* a file, and whether it
 * survives or clears is the thing under test. The graph itself is a canned
 * one-node index behind a `WorkspaceGraph` mock — parsing is `GraphJson`'s
 * business, not this seam's.
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
  type ServerSettings,
} from "@t3tools/contracts";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Ref from "effect/Ref";

import * as ServerConfig from "../config.ts";
import { ServerSettingsService } from "../serverSettings.ts";
import { GraphBuildWorker, type GraphBuildRequest } from "./GraphBuildWorker.ts";
import { GRAPHIFY_PINNED_VERSION } from "./graphifyDetection.ts";
import { GraphifyRuntime } from "./GraphifyRuntime.ts";
import { buildGraphIndex, decodeGraphJson } from "./GraphJson.ts";
import { GraphService } from "./GraphService.ts";
import * as GraphServiceModule from "./GraphService.ts";
import { GraphStore } from "./GraphStore.ts";
import * as GraphStoreModule from "./GraphStore.ts";
import { GraphWorkspaceResolver } from "./GraphWorkspaceResolver.ts";
import { WorkspaceGraph } from "./WorkspaceGraph.ts";

const PROJECT_ID = "6f1f9a4c-4f77-4b9e-9f3a-1d2e3f4a5b6c" as ProjectId;
const HEAD_SHA = "0a1b2c3d";
const WORKSPACE_ROOT = "/repos/t3code";

const IDLE_STATUS: GraphBuildStatus = {
  state: "idle",
  mode: null,
  message: null,
  startedAt: null,
  finishedAt: null,
  detail: null,
};

const ENTRY: GraphStoreEntry = {
  key: { projectId: PROJECT_ID, branch: "main" },
  workspaceRoot: WORKSPACE_ROOT,
  headSha: HEAD_SHA,
  mode: "structural",
  graphifyVersion: GRAPHIFY_PINNED_VERSION,
  builtAt: 1,
  lastOpenedAt: 1,
  nodeCount: 1,
  edgeCount: 0,
  sizeBytes: 1024,
};

/** The smallest graph `graphSnapshot` can be handed: one node, no edges. */
const INDEX = (() => {
  const decoded = decodeGraphJson(
    JSON.stringify({
      directed: false,
      multigraph: false,
      graph: {},
      nodes: [
        {
          id: "src_a_ts",
          label: "a.ts",
          file_type: "code",
          source_file: "src/a.ts",
          source_location: "L1",
          community: 0,
        },
      ],
      links: [],
      hyperedges: [],
      built_at_commit: HEAD_SHA,
    }),
  );
  if (decoded._tag === "Failure") throw new Error(`fixture did not decode: ${decoded.failure}`);
  return buildGraphIndex(decoded.success);
})();

interface World {
  knowledgeGraph: Partial<KnowledgeGraphSettings>;
  runtimeReady: boolean;
  buildStatus: GraphBuildStatus;
}

const world = (overrides?: Partial<World>): World => ({
  knowledgeGraph: { enabled: true, autoRebuild: true },
  runtimeReady: true,
  buildStatus: IDLE_STATUS,
  ...overrides,
});

const layersFor = (state: World, requests: Ref.Ref<ReadonlyArray<GraphBuildRequest>>) => {
  const settings = Layer.mock(ServerSettingsService)({
    getSettings: Effect.sync(
      (): ServerSettings => ({
        ...DEFAULT_SERVER_SETTINGS,
        knowledgeGraph: { ...DEFAULT_SERVER_SETTINGS.knowledgeGraph, ...state.knowledgeGraph },
      }),
    ),
  });

  const resolver = Layer.effect(
    GraphWorkspaceResolver,
    Effect.gen(function* () {
      const store = yield* GraphStore;
      return GraphWorkspaceResolver.of({
        resolve: (cwd) =>
          Effect.gen(function* () {
            const location = yield* store.locate({
              projectId: PROJECT_ID,
              branch: "main",
              headSha: HEAD_SHA,
            });
            return {
              projectId: PROJECT_ID,
              workspaceRoot: cwd,
              branch: "main",
              headSha: HEAD_SHA,
              location,
            };
          }),
      });
    }),
  );

  const runtime = Layer.mock(GraphifyRuntime)({
    resolve: Effect.suspend(() =>
      state.runtimeReady
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
    statusFor: () => Effect.sync(() => state.buildStatus),
  });

  const graphs = Layer.mock(WorkspaceGraph)({
    load: () => Effect.succeed({ index: INDEX, modifiedAt: 1 }),
    forget: () => Effect.void,
  });

  const dependencies = Layer.mergeAll(resolver, runtime, worker, graphs, settings).pipe(
    Layer.provideMerge(GraphStoreModule.layer),
  );

  return GraphServiceModule.layer.pipe(
    Layer.provideMerge(dependencies),
    Layer.provide(ServerConfig.layerTest(process.cwd(), { prefix: "t3-graph-service-test-" })),
    Layer.provideMerge(NodeServices.layer),
  );
};

/**
 * Seeds one built entry (optionally dirty), runs `use` against the service,
 * and reports what reached the build worker and whether the mark survived.
 */
const runRead = <A, E>(
  state: World,
  options: { readonly dirty: boolean },
  use: (service: GraphService["Service"]) => Effect.Effect<A, E>,
) =>
  Effect.gen(function* () {
    const requests = yield* Ref.make<ReadonlyArray<GraphBuildRequest>>([]);

    return yield* Effect.gen(function* () {
      const store = yield* GraphStore;
      const location = yield* store.locate({
        projectId: PROJECT_ID,
        branch: "main",
        headSha: HEAD_SHA,
      });
      yield* store.ensure(location);
      yield* store.writeEntry(location, ENTRY);
      if (options.dirty) yield* store.markDirty(location);

      const service = yield* GraphService;
      const output = yield* use(service);

      return {
        output,
        requests: yield* Ref.get(requests),
        dirty: yield* store.isDirty(location),
      };
    }).pipe(Effect.scoped, Effect.provide(layersFor(state, requests)));
  });

describe("GraphService deferred rebuilds", () => {
  it.effect("a read of a dirty graph serves it and queues one background refresh", () =>
    Effect.gen(function* () {
      const result = yield* runRead(world(), { dirty: true }, (service) =>
        service.snapshot(WORKSPACE_ROOT),
      );

      // The read is served from disk — the freshness contract is a `stale`
      // flag plus a background build, never a blocking wait.
      expect(result.output).toMatchObject({ nodeCount: 1, stale: false });
      expect(result.requests).toHaveLength(1);
      expect(result.requests[0]?.mode).toBe("structural");
      expect(result.requests[0]?.force).toBe(false);
      expect(result.requests[0]?.workspaceRoot).toBe(WORKSPACE_ROOT);
      expect(result.dirty).toBe(false);
    }),
  );

  it.effect("a read of a clean graph queues nothing", () =>
    Effect.gen(function* () {
      const result = yield* runRead(world(), { dirty: false }, (service) =>
        service.snapshot(WORKSPACE_ROOT),
      );

      expect(result.requests).toEqual([]);
      expect(result.dirty).toBe(false);
    }),
  );

  it.effect("honours auto-rebuild being switched off, keeping the mark", () =>
    Effect.gen(function* () {
      const result = yield* runRead(
        world({ knowledgeGraph: { enabled: true, autoRebuild: false } }),
        { dirty: true },
        (service) => service.snapshot(WORKSPACE_ROOT),
      );

      expect(result.requests).toEqual([]);
      expect(result.dirty).toBe(true);
    }),
  );

  it.effect("does not stack onto a build already in flight, keeping the mark", () =>
    Effect.gen(function* () {
      const result = yield* runRead(
        world({ buildStatus: { ...IDLE_STATUS, state: "running", mode: "structural" } }),
        { dirty: true },
        (service) => service.snapshot(WORKSPACE_ROOT),
      );

      expect(result.requests).toEqual([]);
      // The in-flight build predates this read; the debt stays until a build
      // is queued *for* it, so a later read can still settle it.
      expect(result.dirty).toBe(true);
    }),
  );

  it.effect("stays deferred when graphify is not installed, keeping the mark", () =>
    Effect.gen(function* () {
      const result = yield* runRead(world({ runtimeReady: false }), { dirty: true }, (service) =>
        service.snapshot(WORKSPACE_ROOT),
      );

      expect(result.requests).toEqual([]);
      expect(result.dirty).toBe(true);
    }),
  );

  it.effect("a manual build always runs and settles the mark, even with auto-rebuild off", () =>
    Effect.gen(function* () {
      const result = yield* runRead(
        world({ knowledgeGraph: { enabled: true, autoRebuild: false } }),
        { dirty: true },
        (service) => service.build({ cwd: WORKSPACE_ROOT, mode: "structural", force: true }),
      );

      expect(result.requests).toHaveLength(1);
      expect(result.requests[0]?.force).toBe(true);
      expect(result.dirty).toBe(false);
    }),
  );
});
