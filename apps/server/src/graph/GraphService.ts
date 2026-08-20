/**
 * GraphService - what the `graph.*` RPCs call.
 *
 * The pieces underneath each do one thing: `GraphWorkspaceResolver` turns a
 * `cwd` into a store location, `GraphStore` owns the directory, `WorkspaceGraph`
 * parses, `GraphJson` slices, `GraphBuildWorker` builds. This is the seam that
 * joins them and applies the two rules every entry point shares.
 *
 * **The settings gate is here, not only in the UI.** Every method fails
 * `GraphDisabledError` when `knowledgeGraph.enabled` is off. Hiding the panel is
 * a courtesy; refusing the RPC is the actual gate, and it is the one an MCP tool
 * or a stale client also hits.
 *
 * **Every read touches `lastOpenedAt`.** Retention counts from the last open, so
 * this is what makes the 60-day rule mean "unused" rather than "unbuilt". It is
 * one small atomic write per read, which is cheap next to parsing a graph.
 *
 * @module GraphService
 */
import {
  type GraphBuildInput,
  type GraphBuildStatus,
  type GraphCommandFailedError,
  GraphDisabledError,
  type GraphExplanation,
  GraphNodeNotFoundError,
  type GraphNodeQueryInput,
  GraphNotBuiltError,
  type GraphPathInput,
  type GraphPathResult,
  type GraphQueryInput,
  type GraphRuntimeUnavailableError,
  type GraphSearchResult,
  type GraphSnapshot,
  type GraphStatus,
  type GraphStorePathError,
  type GraphSubgraph,
  type GraphSubgraphInput,
  type GraphWorkspaceUnknownError,
  GRAPH_EXPLAIN_MAX_NEIGHBORS,
  type ServerSettingsError,
} from "@t3tools/contracts";
import * as Context from "effect/Context";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";

import { ServerSettingsService } from "../serverSettings.ts";
import { GraphBuildWorker } from "./GraphBuildWorker.ts";
import {
  graphExplain,
  graphPath,
  graphSearch,
  graphSnapshot,
  graphSubgraph,
  resolveNodeReference,
} from "./GraphJson.ts";
import { GraphifyRuntime } from "./GraphifyRuntime.ts";
import { GraphStore } from "./GraphStore.ts";
import { WorkspaceGraph } from "./WorkspaceGraph.ts";
import { GraphWorkspaceResolver, type ResolvedGraphWorkspace } from "./GraphWorkspaceResolver.ts";

/** Errors any graph read can produce before it even reaches a graph. */
type GraphReadError =
  | GraphDisabledError
  | GraphWorkspaceUnknownError
  | GraphStorePathError
  | ServerSettingsError;

export class GraphService extends Context.Service<
  GraphService,
  {
    readonly status: (cwd: string) => Effect.Effect<GraphStatus, GraphReadError>;
    readonly build: (
      input: GraphBuildInput,
    ) => Effect.Effect<
      GraphBuildStatus,
      GraphReadError | GraphRuntimeUnavailableError | GraphCommandFailedError
    >;
    readonly snapshot: (
      cwd: string,
    ) => Effect.Effect<GraphSnapshot, GraphReadError | GraphNotBuiltError>;
    readonly subgraph: (
      input: GraphSubgraphInput,
    ) => Effect.Effect<GraphSubgraph, GraphReadError | GraphNotBuiltError>;
    readonly search: (
      input: GraphQueryInput,
    ) => Effect.Effect<GraphSearchResult, GraphReadError | GraphNotBuiltError>;
    readonly explain: (
      input: GraphNodeQueryInput,
    ) => Effect.Effect<
      GraphExplanation,
      GraphReadError | GraphNotBuiltError | GraphNodeNotFoundError
    >;
    readonly path: (
      input: GraphPathInput,
    ) => Effect.Effect<
      GraphPathResult,
      GraphReadError | GraphNotBuiltError | GraphNodeNotFoundError
    >;
  }
>()("t3/graph/GraphService") {}

export const make = Effect.gen(function* () {
  const settingsService = yield* ServerSettingsService;
  const resolver = yield* GraphWorkspaceResolver;
  const store = yield* GraphStore;
  const graphs = yield* WorkspaceGraph;
  const runtime = yield* GraphifyRuntime;
  const worker = yield* GraphBuildWorker;

  const requireEnabled = Effect.gen(function* () {
    const settings = yield* settingsService.getSettings;
    if (!settings.knowledgeGraph.enabled) return yield* new GraphDisabledError();
    return settings.knowledgeGraph;
  });

  /**
   * A graph is stale when the commit it was built from is not the commit that
   * is checked out now.
   *
   * Deliberately coarse. A finer signal — has any tracked file changed since
   * `builtAt` — would need a working-tree scan on every status poll, and the
   * honest answer the panel needs is binary: trust it, or rebuild. An unknown
   * HEAD on either side counts as stale, because "we cannot tell" and "it is
   * current" must not look the same to an agent about to cite it.
   */
  const isStale = (resolved: ResolvedGraphWorkspace, entryHeadSha: string | null): boolean =>
    resolved.headSha === null || entryHeadSha === null || entryHeadSha !== resolved.headSha;

  /**
   * The consume-time half of auto-rebuild's consumer gating.
   *
   * `GraphAutoRebuild` defers rebuilds for graphs nobody has read within its
   * recency window, leaving a dirty mark in the store instead of spending CPU.
   * The first read after that settles the debt: serve the graph on disk *now*
   * and queue one structural refresh in the background. Serving stale is the
   * contract every read already has — the response carries a `stale` flag and
   * the worker is fire-and-forget by design, so awaiting a minutes-long
   * extraction here would only produce RPC timeouts.
   *
   * Deliberately total: a read must never fail because its background refresh
   * could not be arranged. What cannot be arranged now (runtime missing,
   * settings unreadable) leaves the mark in place for a later read.
   */
  const settleDeferredRebuild = Effect.fn("GraphService.settleDeferredRebuild")(function* (
    resolved: ResolvedGraphWorkspace,
  ) {
    if (!(yield* store.isDirty(resolved.location))) return;

    // The mark was made while auto-rebuild was on; honour the setting as it
    // is *now* — switching it off must stop background builds immediately.
    const autoRebuild = yield* settingsService.getSettings.pipe(
      Effect.map((settings) => settings.knowledgeGraph.autoRebuild === true),
      Effect.orElseSucceed(() => false),
    );
    if (!autoRebuild) return;

    // A build already in flight will produce a fresh graph; leave the mark
    // alone rather than clearing it against work that predates this read.
    const status = yield* worker.statusFor(resolved.location);
    if (status.state === "queued" || status.state === "running") return;

    // Same rule as the watcher path: never queue a build that can only fail.
    const ready = yield* runtime.resolve.pipe(
      Effect.as(true),
      Effect.catchCause(() => Effect.succeed(false)),
    );
    if (!ready) return;

    yield* store.clearDirty(resolved.location).pipe(Effect.ignore);
    yield* worker.request({
      location: resolved.location,
      workspaceRoot: resolved.workspaceRoot,
      headSha: resolved.headSha,
      mode: "structural",
      force: false,
    });
    yield* Effect.logInfo("graph.autoRebuild.on-consume", {
      projectId: resolved.location.projectId,
      branch: resolved.branch,
    });
  });

  /** Reads and stamps the open — the part every read shares after resolution. */
  const openGraph = Effect.fn("GraphService.openGraph")(function* (
    resolved: ResolvedGraphWorkspace,
  ) {
    const loaded = yield* graphs.load(resolved.location);
    if (loaded === null) return { loaded: null, snapshot: null } as const;

    yield* store.touch(resolved.location);
    yield* settleDeferredRebuild(resolved);
    const entry = yield* store.readEntry(resolved.location);
    const snapshot = graphSnapshot(loaded.index, {
      // `meta.json` is T3's record of the build; `graph.json`'s mtime is the
      // fallback when the sidecar is missing, which happens if a build was
      // interrupted between graphify finishing and the entry being written.
      builtAt: entry?.builtAt ?? loaded.modifiedAt,
      stale: isStale(resolved, entry?.headSha ?? loaded.index.builtAtCommit),
    });
    return { loaded, snapshot } as const;
  });

  /** Gate, then resolve — in that order, so a disabled feature resolves nothing. */
  const enter = Effect.fn("GraphService.enter")(function* (cwd: string) {
    yield* requireEnabled;
    return yield* resolver.resolve(cwd);
  });

  const status: GraphService["Service"]["status"] = Effect.fn("GraphService.status")(
    function* (cwd) {
      const resolved = yield* enter(cwd);
      const opened = yield* openGraph(resolved);
      return {
        enabled: true,
        runtime: yield* runtime.status,
        build: yield* worker.statusFor(resolved.location),
        branch: resolved.branch,
        snapshot: opened.snapshot,
      };
    },
  );

  const build: GraphService["Service"]["build"] = Effect.fn("GraphService.build")(
    function* (input) {
      const resolved = yield* enter(input.cwd);
      // Fail fast on a missing toolchain rather than queueing a job that can
      // only fail: the caller asked for a build and deserves the reason now.
      yield* runtime.resolve;
      const status = yield* worker.request({
        location: resolved.location,
        workspaceRoot: resolved.workspaceRoot,
        headSha: resolved.headSha,
        mode: input.mode,
        force: input.force,
      });
      // An explicit build settles any deferred auto-rebuild owed on the entry.
      yield* store.clearDirty(resolved.location).pipe(Effect.ignore);
      return status;
    },
  );

  const snapshot: GraphService["Service"]["snapshot"] = Effect.fn("GraphService.snapshot")(
    function* (cwd) {
      const opened = yield* openGraph(yield* enter(cwd));
      if (opened.snapshot === null) return yield* new GraphNotBuiltError({ cwd });
      return opened.snapshot;
    },
  );

  const subgraph: GraphService["Service"]["subgraph"] = Effect.fn("GraphService.subgraph")(
    function* (input) {
      const opened = yield* openGraph(yield* enter(input.cwd));
      if (opened.loaded === null) return yield* new GraphNotBuiltError({ cwd: input.cwd });
      return graphSubgraph(opened.loaded.index, {
        nodeId: input.nodeId,
        communityId: input.communityId,
        depth: input.depth,
        limit: input.limit,
      });
    },
  );

  const search: GraphService["Service"]["search"] = Effect.fn("GraphService.search")(
    function* (input) {
      const opened = yield* openGraph(yield* enter(input.cwd));
      if (opened.loaded === null) return yield* new GraphNotBuiltError({ cwd: input.cwd });
      const found = graphSearch(opened.loaded.index, {
        question: input.question,
        limit: input.limit,
      });
      return { ...found, stale: opened.snapshot.stale };
    },
  );

  const explain: GraphService["Service"]["explain"] = Effect.fn("GraphService.explain")(
    function* (input) {
      const opened = yield* openGraph(yield* enter(input.cwd));
      if (opened.loaded === null) return yield* new GraphNotBuiltError({ cwd: input.cwd });
      const node = resolveNodeReference(opened.loaded.index, input.node);
      if (node === null) return yield* new GraphNodeNotFoundError({ reference: input.node });
      return {
        ...graphExplain(opened.loaded.index, node, GRAPH_EXPLAIN_MAX_NEIGHBORS),
        stale: opened.snapshot.stale,
      };
    },
  );

  const path: GraphService["Service"]["path"] = Effect.fn("GraphService.path")(function* (input) {
    const opened = yield* openGraph(yield* enter(input.cwd));
    if (opened.loaded === null) return yield* new GraphNotBuiltError({ cwd: input.cwd });
    const from = resolveNodeReference(opened.loaded.index, input.from);
    if (from === null) return yield* new GraphNodeNotFoundError({ reference: input.from });
    const to = resolveNodeReference(opened.loaded.index, input.to);
    if (to === null) return yield* new GraphNodeNotFoundError({ reference: input.to });
    return { ...graphPath(opened.loaded.index, from, to), stale: opened.snapshot.stale };
  });

  return GraphService.of({ status, build, snapshot, subgraph, search, explain, path });
});

export const layer = Layer.effect(GraphService, make);
