/**
 * GraphBuildWorker - one graphify build per store entry, at a time.
 *
 * Builds are fire-and-forget: `request` returns as soon as the build is
 * queued, and the client polls `graph.status` or watches the build-event
 * stream. A structural extraction of a large monorepo is minutes, so making
 * the RPC wait for it would only produce timeouts.
 *
 * Coalescing is keyed on the **store directory name**, not the workspace root.
 * That is the whole reason the key exists: a watcher storm on one branch
 * collapses into a single build, while a branch switch mid-build produces a
 * different key and therefore a separate entry, so two branches' extractions
 * can never merge into one graph.
 *
 * ## Why the status lives here
 *
 * `KeyedCoalescingWorker` exposes only `enqueue` and `drainKey` — it has no
 * notion of what a job is doing. `graph.status` still has to answer "is it
 * building, and did the last one fail", so this service keeps its own
 * `Ref<Map<key, GraphBuildStatus>>` alongside the worker and updates it at each
 * transition. Terminal states are retained after the job ends, which is what
 * lets the panel show a failure with its stderr instead of silently reverting
 * to "idle".
 *
 * @module GraphBuildWorker
 */
import {
  type GraphBuildMode,
  type GraphBuildStatus,
  type GraphCommandFailedError,
  type GraphDisabledError,
  type GraphRuntimeUnavailableError,
  type GraphStoreEntry,
  type GraphStorePathError,
  type ServerSettingsError,
} from "@t3tools/contracts";
import { makeKeyedCoalescingWorker } from "@t3tools/shared/KeyedCoalescingWorker";
import * as Cause from "effect/Cause";
import * as Clock from "effect/Clock";
import * as Context from "effect/Context";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Path from "effect/Path";
import * as Ref from "effect/Ref";

import { GraphifyCli } from "./GraphifyCli.ts";
import { GraphifyRuntime } from "./GraphifyRuntime.ts";
import { GraphStore, type GraphStoreLocation } from "./GraphStore.ts";
import { GRAPH_JSON_FILE_NAME, WorkspaceGraph } from "./WorkspaceGraph.ts";

/** What an entry that has never been built in this process reports. */
const IDLE_STATUS: GraphBuildStatus = {
  state: "idle",
  mode: null,
  message: null,
  startedAt: null,
  finishedAt: null,
  detail: null,
};

export interface GraphBuildRequest {
  readonly location: GraphStoreLocation;
  /** Checkout graphify scans, as resolved by `GraphWorkspaceResolver`. */
  readonly workspaceRoot: string;
  readonly headSha: string | null;
  readonly mode: GraphBuildMode;
  readonly force: boolean;
}

export class GraphBuildWorker extends Context.Service<
  GraphBuildWorker,
  {
    /** Queues a build and returns the status the caller should render now. */
    readonly request: (input: GraphBuildRequest) => Effect.Effect<GraphBuildStatus>;
    readonly statusFor: (location: GraphStoreLocation) => Effect.Effect<GraphBuildStatus>;
    /** Resolves once this entry has no queued or running build. For tests. */
    readonly drain: (location: GraphStoreLocation) => Effect.Effect<void>;
  }
>()("t3/graph/GraphBuildWorker") {}

/**
 * Two requests for the same entry become one.
 *
 * `semantic` wins over `structural` because it is a superset — it runs the AST
 * pass too — so collapsing the pair into the cheaper one would quietly drop
 * work the user paid tokens to ask for. `force` is sticky for the same reason:
 * whoever asked for a clean rebuild had a reason the other caller did not know.
 */
export function mergeBuildRequests(
  current: GraphBuildRequest,
  next: GraphBuildRequest,
): GraphBuildRequest {
  return {
    ...next,
    mode: current.mode === "semantic" || next.mode === "semantic" ? "semantic" : next.mode,
    force: current.force || next.force,
  };
}

export const make = Effect.gen(function* () {
  const store = yield* GraphStore;
  const cli = yield* GraphifyCli;
  const runtime = yield* GraphifyRuntime;
  const graphs = yield* WorkspaceGraph;
  const fs = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;

  const statusRef = yield* Ref.make<ReadonlyMap<string, GraphBuildStatus>>(new Map());

  const keyOf = (location: GraphStoreLocation) => `${location.projectId}/${location.directoryName}`;

  const setStatus = (key: string, status: GraphBuildStatus) =>
    Ref.update(statusRef, (statuses) => new Map(statuses).set(key, status));

  const patchStatus = (key: string, patch: Partial<GraphBuildStatus>) =>
    Ref.update(statusRef, (statuses) => {
      const current = statuses.get(key) ?? IDLE_STATUS;
      return new Map(statuses).set(key, { ...current, ...patch });
    });

  /**
   * Whether `graphify update` can be used instead of a full `extract`.
   *
   * Requires an existing graph built by the same version from the same
   * checkout. graphify's caches key relative to the scan root but fall back to
   * absolute paths for custom `GRAPHIFY_OUT` layouts (`detect.py`), so reusing
   * a cache across two different checkout paths risks absolute-path residue —
   * hence the `workspaceRoot` comparison rather than trusting the key alone.
   */
  const canRefreshInPlace = (input: {
    readonly request: GraphBuildRequest;
    readonly entry: GraphStoreEntry | null;
    readonly graphExists: boolean;
    readonly version: string;
  }) =>
    !input.request.force &&
    // `update` runs graphify's code pass only, so it cannot produce or refresh
    // the semantic layer a semantic build was asked for.
    input.request.mode === "structural" &&
    input.graphExists &&
    input.entry !== null &&
    input.entry.mode === "structural" &&
    input.entry.graphifyVersion === input.version &&
    input.entry.workspaceRoot === input.request.workspaceRoot;

  const runBuild = Effect.fn("GraphBuildWorker.run")(function* (
    key: string,
    request: GraphBuildRequest,
  ) {
    const startedAt = yield* Clock.currentTimeMillis;
    yield* setStatus(key, {
      state: "running",
      mode: request.mode,
      message: "preparing",
      startedAt,
      finishedAt: null,
      detail: null,
    });

    yield* store.ensure(request.location);

    const resolved = yield* runtime.resolve;
    const previous = yield* store.readEntry(request.location);
    const graphPath = path.join(request.location.outDir, GRAPH_JSON_FILE_NAME);
    const graphExists = yield* fs.exists(graphPath).pipe(Effect.orElseSucceed(() => false));

    const incremental = canRefreshInPlace({
      request,
      entry: previous,
      graphExists,
      version: resolved.version,
    });

    yield* patchStatus(key, {
      message: incremental ? "refreshing changed files" : "extracting",
    });

    yield* cli.build({
      workspaceRoot: request.workspaceRoot,
      outDir: request.location.outDir,
      mode: request.mode,
      force: request.force,
      incremental,
    });

    // The build wrote a new `graph.json` under the same path the cache is keyed
    // on. `WorkspaceGraph` would notice via mtime anyway, but dropping it here
    // means the counts written to `meta.json` come from the graph that was just
    // produced rather than a race with a concurrent reader.
    yield* graphs.forget(request.location);
    const loaded = yield* graphs.load(request.location);
    const sizeBytes = yield* store.measure(request.location);
    const finishedAt = yield* Clock.currentTimeMillis;

    yield* store.writeEntry(request.location, {
      key: { projectId: request.location.projectId, branch: request.location.branch },
      workspaceRoot: request.workspaceRoot,
      headSha: request.headSha,
      mode: request.mode,
      graphifyVersion: resolved.version,
      builtAt: finishedAt,
      // Retention is keyed on *opened*, not *built*, so a rebuild must not
      // reset the clock — otherwise a background refresh would keep a graph
      // nobody looks at alive forever. A first build has no prior open.
      lastOpenedAt: previous?.lastOpenedAt ?? finishedAt,
      nodeCount: loaded?.index.nodes.size ?? 0,
      edgeCount: loaded?.index.edges.length ?? 0,
      sizeBytes,
    });

    yield* setStatus(key, {
      state: "succeeded",
      mode: request.mode,
      message: null,
      startedAt,
      finishedAt,
      detail:
        loaded === null
          ? // graphify exited 0 but produced nothing readable. Reporting success
            // with an empty graph would be a lie the panel cannot recover from.
            "graphify finished but produced no readable graph."
          : null,
    });

    yield* Effect.logInfo("graph build complete", {
      key,
      mode: request.mode,
      incremental,
      nodes: loaded?.index.nodes.size ?? 0,
      edges: loaded?.index.edges.length ?? 0,
      sizeBytes,
      durationMs: finishedAt - startedAt,
    });
  });

  const describeFailure = (
    error:
      | GraphDisabledError
      | GraphRuntimeUnavailableError
      | GraphCommandFailedError
      | GraphStorePathError
      | ServerSettingsError,
  ): string => {
    switch (error._tag) {
      case "GraphCommandFailedError":
        return error.exitCode === null
          ? error.detail
          : `graphify exited ${error.exitCode}.\n${error.detail}`;
      default:
        return error.message;
    }
  };

  const worker = yield* makeKeyedCoalescingWorker<string, GraphBuildRequest, never, never>({
    merge: mergeBuildRequests,
    process: (key, request) =>
      runBuild(key, request).pipe(
        Effect.catch((error) =>
          Effect.gen(function* () {
            const finishedAt = yield* Clock.currentTimeMillis;
            yield* patchStatus(key, {
              state: "failed",
              message: null,
              finishedAt,
              detail: describeFailure(error),
            });
            yield* Effect.logWarning("graph build failed", { key, error });
          }),
        ),
        // A defect must not take the worker fiber down with it: the next build
        // for any key would never run. Record it the same way as a failure.
        Effect.catchCause((cause) =>
          Effect.gen(function* () {
            const finishedAt = yield* Clock.currentTimeMillis;
            yield* patchStatus(key, {
              state: "failed",
              message: null,
              finishedAt,
              detail: Cause.pretty(cause),
            });
            yield* Effect.logError("graph build crashed", { key, cause });
          }),
        ),
      ),
  });

  const request: GraphBuildWorker["Service"]["request"] = Effect.fn("GraphBuildWorker.request")(
    function* (input) {
      const key = keyOf(input.location);
      const startedAt = yield* Clock.currentTimeMillis;
      const current = (yield* Ref.get(statusRef)).get(key);

      // An in-flight build absorbs the request rather than restarting: the
      // worker already merges the new parameters into the follow-up run.
      const queued: GraphBuildStatus =
        current?.state === "running"
          ? current
          : {
              state: "queued",
              mode: input.mode,
              message: null,
              startedAt,
              finishedAt: null,
              detail: null,
            };
      yield* setStatus(key, queued);
      yield* worker.enqueue(key, input);
      return queued;
    },
  );

  const statusFor: GraphBuildWorker["Service"]["statusFor"] = (location) =>
    Ref.get(statusRef).pipe(Effect.map((statuses) => statuses.get(keyOf(location)) ?? IDLE_STATUS));

  const drain: GraphBuildWorker["Service"]["drain"] = (location) =>
    worker.drainKey(keyOf(location));

  return GraphBuildWorker.of({ request, statusFor, drain });
});

export const layer = Layer.effect(GraphBuildWorker, make);
