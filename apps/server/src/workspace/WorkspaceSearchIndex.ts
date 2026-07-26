import { FileFinder, type MixedItem, type MixedSearchResult } from "@ff-labs/fff-node";
import * as Context from "effect/Context";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as LayerMap from "effect/LayerMap";
import * as Schedule from "effect/Schedule";
import * as Schema from "effect/Schema";

import type {
  ProjectEntry,
  ProjectListEntriesResult,
  ProjectSearchEntriesResult,
} from "@t3tools/contracts";

import * as WorkspaceIgnoredEntries from "./WorkspaceIgnoredEntries.ts";

const WORKSPACE_INDEX_MAX_ENTRIES = 25_000;
const WORKSPACE_INDEX_PAGE_SIZE = WORKSPACE_INDEX_MAX_ENTRIES + 2;
const WORKSPACE_INDEX_SCAN_TIMEOUT = "15 seconds";
const WORKSPACE_INDEX_IDLE_TTL = "15 minutes";
const WORKSPACE_INDEX_SCAN_POLL_INTERVAL = "50 millis";

export class WorkspaceSearchIndexCreateFailed extends Schema.TaggedErrorClass<WorkspaceSearchIndexCreateFailed>()(
  "WorkspaceSearchIndexCreateFailed",
  {
    cwd: Schema.String,
    reason: Schema.String,
    cause: Schema.optional(Schema.Defect()),
  },
) {
  override get message(): string {
    return `Failed to create the workspace search index for '${this.cwd}'.`;
  }
}

export class WorkspaceSearchIndexScanTimedOut extends Schema.TaggedErrorClass<WorkspaceSearchIndexScanTimedOut>()(
  "WorkspaceSearchIndexScanTimedOut",
  {
    cwd: Schema.String,
    timeout: Schema.String,
  },
) {
  override get message(): string {
    return `Workspace search index for '${this.cwd}' did not finish scanning within ${this.timeout}`;
  }
}

export class WorkspaceSearchIndexSearchFailed extends Schema.TaggedErrorClass<WorkspaceSearchIndexSearchFailed>()(
  "WorkspaceSearchIndexSearchFailed",
  {
    cwd: Schema.String,
    queryLength: Schema.Number,
    pageSize: Schema.Number,
    reason: Schema.String,
    cause: Schema.optional(Schema.Defect()),
  },
) {
  override get message(): string {
    return `Workspace search failed for '${this.cwd}'.`;
  }
}

export class WorkspaceSearchIndexRefreshFailed extends Schema.TaggedErrorClass<WorkspaceSearchIndexRefreshFailed>()(
  "WorkspaceSearchIndexRefreshFailed",
  {
    cwd: Schema.String,
    reason: Schema.String,
    cause: Schema.optional(Schema.Defect()),
  },
) {
  override get message(): string {
    return `Failed to refresh the workspace search index for '${this.cwd}'.`;
  }
}

export class WorkspaceSearchIndexDestroyFailed extends Schema.TaggedErrorClass<WorkspaceSearchIndexDestroyFailed>()(
  "WorkspaceSearchIndexDestroyFailed",
  {
    cwd: Schema.String,
    cause: Schema.Defect(),
  },
) {
  override get message(): string {
    return `Failed to destroy the workspace search index for '${this.cwd}'.`;
  }
}

export type WorkspaceSearchIndexError =
  | WorkspaceSearchIndexCreateFailed
  | WorkspaceSearchIndexScanTimedOut
  | WorkspaceSearchIndexSearchFailed
  | WorkspaceSearchIndexRefreshFailed;

export class WorkspaceSearchIndex extends Context.Service<
  WorkspaceSearchIndex,
  {
    readonly list: () => Effect.Effect<ProjectListEntriesResult, WorkspaceSearchIndexSearchFailed>;
    readonly search: (
      query: string,
      limit: number,
    ) => Effect.Effect<ProjectSearchEntriesResult, WorkspaceSearchIndexSearchFailed>;
    readonly refresh: () => Effect.Effect<
      void,
      WorkspaceSearchIndexRefreshFailed | WorkspaceSearchIndexScanTimedOut
    >;
  }
>()("t3/workspace/WorkspaceSearchIndex") {}

function toPosixPath(input: string): string {
  return input.replaceAll("\\", "/");
}

function trimDirectorySeparator(input: string): string {
  return input.endsWith("/") ? input.slice(0, -1) : input;
}

function parentPathOf(input: string): string | undefined {
  const separatorIndex = input.lastIndexOf("/");
  return separatorIndex === -1 ? undefined : input.slice(0, separatorIndex);
}

function toProjectEntry(item: MixedItem): ProjectEntry | null {
  const normalizedPath = trimDirectorySeparator(toPosixPath(item.item.relativePath));
  if (!normalizedPath) {
    return null;
  }

  return {
    path: normalizedPath,
    kind: item.type,
  };
}

function mapMixedSearchResult(
  result: MixedSearchResult,
  limit: number,
): { readonly entries: ProjectEntry[]; readonly truncated: boolean } {
  const entries: ProjectEntry[] = [];
  for (const item of result.items) {
    const entry = toProjectEntry(item);
    if (entry) {
      entries.push(entry);
    }
    if (entries.length >= limit) {
      break;
    }
  }

  const rootDirectoryCount = result.items.some(
    (item) => item.type === "directory" && item.item.relativePath.length === 0,
  )
    ? 1
    : 0;
  return {
    entries,
    truncated: result.totalMatched - rootDirectoryCount > limit,
  };
}

/**
 * Append supplement entries (gitignored paths git enumerated but fff's
 * gitignore-respecting walker never indexed) to the fff results, deduplicated
 * by path with the fff entry winning.
 */
export function mergeSupplementEntries(
  entries: ReadonlyArray<ProjectEntry>,
  supplement: ReadonlyArray<ProjectEntry>,
): ProjectEntry[] {
  const seenPaths = new Set(entries.map((entry) => entry.path));
  const merged = [...entries];
  for (const entry of supplement) {
    if (seenPaths.has(entry.path)) continue;
    seenPaths.add(entry.path);
    merged.push(entry);
  }
  return merged;
}

/**
 * Rank merged search results so contiguous-substring matches outrank
 * scattered subsequence matches regardless of source. Without this, a broad
 * query fills the result limit with fff's fuzzy tail and slices off an
 * ignored file whose name is exactly what the user typed. Within each tier
 * the native index's relevance order is preserved and supplement entries
 * follow it; duplicates keep their first (highest-ranked) occurrence.
 */
export function rankMergedSearchEntries(
  indexed: ReadonlyArray<ProjectEntry>,
  supplementMatches: ReadonlyArray<ProjectEntry>,
  query: string,
): ProjectEntry[] {
  const containsQuery = (entry: ProjectEntry) => entry.path.toLowerCase().includes(query);
  const ordered = [
    ...indexed.filter(containsQuery),
    ...supplementMatches.filter(containsQuery),
    ...indexed.filter((entry) => !containsQuery(entry)),
    ...supplementMatches.filter((entry) => !containsQuery(entry)),
  ];
  const seenPaths = new Set<string>();
  const merged: ProjectEntry[] = [];
  for (const entry of ordered) {
    if (seenPaths.has(entry.path)) continue;
    seenPaths.add(entry.path);
    merged.push(entry);
  }
  return merged;
}

/**
 * Subsequence match (fzf-style) approximating fff's fuzzy matching for the
 * supplement, which never reaches the native matcher. `query` is expected
 * lowercased; an empty query matches everything.
 */
export function matchesSupplementQuery(path: string, query: string): boolean {
  const lowerPath = path.toLowerCase();
  let queryIndex = 0;
  for (let pathIndex = 0; pathIndex < lowerPath.length && queryIndex < query.length; pathIndex++) {
    if (lowerPath[pathIndex] === query[queryIndex]) {
      queryIndex++;
    }
  }
  return queryIndex === query.length;
}

function withDirectoryAncestors(entries: ReadonlyArray<ProjectEntry>): ProjectEntry[] {
  const entryByPath = new Map(entries.map((entry) => [entry.path, entry]));
  for (const entry of entries) {
    let parentPath = parentPathOf(entry.path);
    while (parentPath) {
      if (!entryByPath.has(parentPath)) {
        entryByPath.set(parentPath, { path: parentPath, kind: "directory" });
      }
      parentPath = parentPathOf(parentPath);
    }
  }
  return [...entryByPath.values()];
}

const createFinder = Effect.fn("WorkspaceSearchIndex.createFinder")(function* (cwd: string) {
  const result = yield* Effect.try({
    try: () =>
      FileFinder.create({
        basePath: cwd,
        disableMmapCache: true,
        disableContentIndexing: true,
        aiMode: false,
        enableFsRootScanning: true,
        enableHomeDirScanning: true,
      }),
    catch: (cause) =>
      new WorkspaceSearchIndexCreateFailed({
        cwd,
        reason: "FileFinder.create threw unexpectedly.",
        cause,
      }),
  });
  if (result.ok) return result.value;
  return yield* new WorkspaceSearchIndexCreateFailed({
    cwd,
    reason: result.error,
  });
});

const waitForScan = <E>(cwd: string, finder: FileFinder, onFailure: (cause: unknown) => E) =>
  Effect.try({
    try: () => finder.isScanning(),
    catch: onFailure,
  }).pipe(
    Effect.repeat({
      while: (scanning) => scanning,
      schedule: Schedule.spaced(WORKSPACE_INDEX_SCAN_POLL_INTERVAL),
    }),
    Effect.timeoutOrElse({
      duration: WORKSPACE_INDEX_SCAN_TIMEOUT,
      orElse: () =>
        new WorkspaceSearchIndexScanTimedOut({ cwd, timeout: WORKSPACE_INDEX_SCAN_TIMEOUT }),
    }),
    Effect.withSpan("WorkspaceSearchIndex.waitForScan"),
  );

export const make = Effect.fn("WorkspaceSearchIndex.make")(function* (cwd: string) {
  const finder = yield* Effect.acquireRelease(createFinder(cwd), (finder) =>
    Effect.try({
      try: () => finder.destroy(),
      catch: (cause) => new WorkspaceSearchIndexDestroyFailed({ cwd, cause }),
    }).pipe(Effect.orDie),
  );
  // The gitignored-entries supplement makes files fff's gitignore-respecting
  // walker skips (build output, .env, generated code) visible in the entries
  // index; enumerated concurrently with the initial scan.
  const [, initialSupplement] = yield* Effect.all(
    [
      waitForScan(
        cwd,
        finder,
        (cause) =>
          new WorkspaceSearchIndexCreateFailed({
            cwd,
            reason: "FileFinder.isScanning threw while creating the index.",
            cause,
          }),
      ),
      WorkspaceIgnoredEntries.listIgnoredEntries(cwd),
    ],
    { concurrency: 2 },
  );
  let supplement = initialSupplement;

  const runMixedSearch = Effect.fn("WorkspaceSearchIndex.runMixedSearch")(function* (
    query: string,
    pageSize: number,
  ) {
    const result = yield* Effect.try({
      try: () => finder.mixedSearch(query, { pageSize }),
      catch: (cause) =>
        new WorkspaceSearchIndexSearchFailed({
          cwd,
          queryLength: query.length,
          pageSize,
          reason: "FileFinder.mixedSearch threw unexpectedly.",
          cause,
        }),
    });
    if (!result.ok) {
      return yield* new WorkspaceSearchIndexSearchFailed({
        cwd,
        queryLength: query.length,
        pageSize,
        reason: result.error,
      });
    }
    return result.value;
  });

  const refresh: WorkspaceSearchIndex["Service"]["refresh"] = Effect.fn(
    "WorkspaceSearchIndex.refresh",
  )(function* () {
    const result = yield* Effect.try({
      try: () => finder.scanFiles(),
      catch: (cause) =>
        new WorkspaceSearchIndexRefreshFailed({
          cwd,
          reason: "FileFinder.scanFiles threw unexpectedly.",
          cause,
        }),
    });
    if (!result.ok) {
      return yield* new WorkspaceSearchIndexRefreshFailed({
        cwd,
        reason: result.error,
      });
    }
    const [, refreshedSupplement] = yield* Effect.all(
      [
        waitForScan(
          cwd,
          finder,
          (cause) =>
            new WorkspaceSearchIndexRefreshFailed({
              cwd,
              reason: "FileFinder.isScanning threw while refreshing the index.",
              cause,
            }),
        ),
        WorkspaceIgnoredEntries.listIgnoredEntries(cwd),
      ],
      { concurrency: 2 },
    );
    supplement = refreshedSupplement;
  });

  const list: WorkspaceSearchIndex["Service"]["list"] = Effect.fn("WorkspaceSearchIndex.list")(
    function* () {
      const result = yield* runMixedSearch("", WORKSPACE_INDEX_PAGE_SIZE);
      const mapped = mapMixedSearchResult(result, WORKSPACE_INDEX_MAX_ENTRIES);
      const merged = mergeSupplementEntries(mapped.entries, supplement.entries);
      const sortedEntries = withDirectoryAncestors(merged).toSorted((left, right) =>
        left.path.localeCompare(right.path),
      );
      const entries = sortedEntries.slice(0, WORKSPACE_INDEX_MAX_ENTRIES);
      return {
        entries,
        truncated:
          mapped.truncated || supplement.truncated || entries.length < sortedEntries.length,
      };
    },
  );

  const search: WorkspaceSearchIndex["Service"]["search"] = Effect.fn(
    "WorkspaceSearchIndex.search",
  )(function* (query, limit) {
    const result = yield* runMixedSearch(query, Math.max(1, limit + 1));
    const mapped = mapMixedSearchResult(result, limit);
    const supplementMatches = supplement.entries.filter((entry) =>
      matchesSupplementQuery(entry.path, query),
    );
    const merged = rankMergedSearchEntries(mapped.entries, supplementMatches, query);
    const entries = merged.slice(0, limit);
    return {
      entries,
      truncated: mapped.truncated || entries.length < merged.length,
    };
  });

  return WorkspaceSearchIndex.of({ list, refresh, search });
});

/**
 * A layer factory is required because every index is scoped to a concrete
 * workspace root. WorkspaceSearchIndexMap owns memoization and idle cleanup;
 * using a default cwd here would mix resources from different workspaces.
 */
export const layer = (cwd: string) => Layer.effect(WorkspaceSearchIndex, make(cwd));

export class WorkspaceSearchIndexMap extends LayerMap.Service<WorkspaceSearchIndexMap>()(
  "t3/workspace/WorkspaceSearchIndexMap",
  {
    lookup: layer,
    idleTimeToLive: WORKSPACE_INDEX_IDLE_TTL,
  },
) {}
