/**
 * WorkspaceGraph - the parsed `graph.json` for a store entry, cached.
 *
 * Parsing is the expensive part of serving the panel: a monorepo `graph.json`
 * is tens of megabytes and every `graph.snapshot` / `graph.subgraph` call would
 * otherwise re-read and re-index it. The cache is keyed on the file's
 * `(path, mtime, size)`, so a rebuild invalidates it without anyone having to
 * remember to call an invalidation hook — including a rebuild that happened in
 * another process.
 *
 * Bounded to `MAX_CACHED_GRAPHS` entries, least-recently-used first. A user with
 * six projects open should not hold six large indexes resident, and re-parsing
 * a graph is seconds at worst while an unbounded cache is unbounded.
 *
 * Failures degrade to `null` — "no graph" — rather than propagating. A
 * `graph.json` that cannot be read or decoded means the entry needs rebuilding,
 * which is the safe direction and the one the UI can act on.
 *
 * @module WorkspaceGraph
 */
import * as Context from "effect/Context";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Path from "effect/Path";
import * as Ref from "effect/Ref";

import { buildGraphIndex, decodeGraphJson, type GraphIndex } from "./GraphJson.ts";
import type { GraphStoreLocation } from "./GraphStore.ts";

/** graphify's own filename inside the output directory. */
export const GRAPH_JSON_FILE_NAME = "graph.json";

/** Enough for the projects a person switches between; not enough to be a leak. */
const MAX_CACHED_GRAPHS = 3;

interface CacheEntry {
  /** `(mtime, size)` of the file the index was parsed from. */
  readonly stamp: string;
  readonly index: GraphIndex;
  /** Monotonic counter, for least-recently-used eviction. */
  readonly usedAt: number;
}

export interface LoadedGraph {
  readonly index: GraphIndex;
  /** `graph.json`'s mtime — the build time the store's `meta.json` may disagree with. */
  readonly modifiedAt: number;
}

export class WorkspaceGraph extends Context.Service<
  WorkspaceGraph,
  {
    /** The parsed graph for a store entry, or null when there is not one. */
    readonly load: (location: GraphStoreLocation) => Effect.Effect<LoadedGraph | null>;
    /** Forget a cached entry, for an eviction or a forced rebuild. */
    readonly forget: (location: GraphStoreLocation) => Effect.Effect<void>;
  }
>()("t3/graph/WorkspaceGraph") {}

export const make = Effect.gen(function* () {
  const fs = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;

  const cacheRef = yield* Ref.make<ReadonlyMap<string, CacheEntry>>(new Map());
  const clockRef = yield* Ref.make(0);

  const graphPathFor = (location: GraphStoreLocation) =>
    path.join(location.outDir, GRAPH_JSON_FILE_NAME);

  const load: WorkspaceGraph["Service"]["load"] = Effect.fn("WorkspaceGraph.load")(
    function* (location) {
      const graphPath = graphPathFor(location);
      const info = yield* fs.stat(graphPath).pipe(Effect.catchCause(() => Effect.succeed(null)));
      if (info === null || info.type !== "File") return null;

      const modifiedAt = info.mtime._tag === "Some" ? info.mtime.value.getTime() : 0;
      const stamp = `${modifiedAt}:${info.size}`;

      const tick = yield* Ref.updateAndGet(clockRef, (value) => value + 1);
      const cached = (yield* Ref.get(cacheRef)).get(graphPath);
      if (cached !== undefined && cached.stamp === stamp) {
        yield* Ref.update(cacheRef, (cache) =>
          new Map(cache).set(graphPath, { ...cached, usedAt: tick }),
        );
        return { index: cached.index, modifiedAt };
      }

      const contents = yield* fs
        .readFileString(graphPath)
        .pipe(Effect.catchCause(() => Effect.succeed(null)));
      if (contents === null) return null;

      const decoded = yield* Effect.fromResult(decodeGraphJson(contents)).pipe(
        Effect.catchCause((cause) =>
          // Not fatal: an unreadable graph means "rebuild me", and saying so is
          // more useful than failing the request the panel made to find out.
          Effect.logWarning("graph decode failed", { graphPath, cause }).pipe(Effect.as(null)),
        ),
      );
      if (decoded === null) return null;

      const index = buildGraphIndex(decoded);
      if (index.skipped.nodes > 0 || index.skipped.edges > 0) {
        yield* Effect.logWarning("graph decoded with skipped entries", {
          graphPath,
          skippedNodes: index.skipped.nodes,
          skippedEdges: index.skipped.edges,
        });
      }

      yield* Ref.update(cacheRef, (cache) => {
        const next = new Map(cache).set(graphPath, { stamp, index, usedAt: tick });
        if (next.size <= MAX_CACHED_GRAPHS) return next;
        let oldestKey: string | null = null;
        let oldestUsedAt = Number.POSITIVE_INFINITY;
        for (const [key, entry] of next) {
          if (entry.usedAt < oldestUsedAt) {
            oldestUsedAt = entry.usedAt;
            oldestKey = key;
          }
        }
        if (oldestKey !== null) next.delete(oldestKey);
        return next;
      });

      return { index, modifiedAt };
    },
  );

  const forget: WorkspaceGraph["Service"]["forget"] = (location) =>
    Ref.update(cacheRef, (cache) => {
      const next = new Map(cache);
      next.delete(graphPathFor(location));
      return next;
    });

  return WorkspaceGraph.of({ load, forget });
});

export const layer = Layer.effect(WorkspaceGraph, make);
