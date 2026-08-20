/**
 * GraphStore - sole owner of `<baseDir>/caches/graph/`.
 *
 * Nothing is ever written into the user's repository. Every graphify
 * invocation runs with `GRAPHIFY_OUT` pointed at a directory this service
 * computed, and no other module may compute a path inside the store.
 *
 * ```
 * <baseDir>/caches/graph/
 *   <projectId>/                     ProjectId — a UUIDv4, not a path hash
 *     <branchSlug>-<hash8>/
 *       meta.json                    T3-owned; graphify never touches it
 *       graphify-out/                GRAPHIFY_OUT points here
 * ```
 *
 * Entries are keyed by `(projectId, branch)` rather than by workspace path. A
 * single checkout keeps one path while its branch changes underneath it, so a
 * path key would serve a `main` graph after switching to a feature branch; a
 * worktree gets its own path per branch, so a path key would fragment. Git
 * forbids the same branch in two worktrees, which makes the pair unique across
 * both layouts.
 *
 * **`evict` is the most dangerous code in this feature** — a recursive delete
 * of a computed path, where the inputs (branch names, directory names read
 * back off disk) are not fully trusted. It therefore refuses unless the target
 * passes all four of: valid project-id segment, valid entry segment, real path
 * exactly equal to the expected path (which rules out symlinks), and a
 * strictly-inside-the-root containment check. A refusal is a
 * `GraphStorePathError` and is never swallowed.
 *
 * @module GraphStore
 */
import {
  type GraphStoreEntry,
  GraphStoreEntry as GraphStoreEntrySchema,
  GraphStorePathError,
  type ProjectId,
} from "@t3tools/contracts";
import * as Clock from "effect/Clock";
import * as Context from "effect/Context";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Path from "effect/Path";
import * as Schema from "effect/Schema";

import { writeFileStringAtomically } from "../atomicWrite.ts";
import { ServerConfig } from "../config.ts";
import * as GraphAvailability from "./GraphAvailability.ts";
import {
  GRAPH_DIRTY_FILE_NAME,
  GRAPH_META_FILE_NAME,
  GRAPHIFY_OUT_DIR_NAME,
  type GraphStoreKeyInput,
  graphStoreDirectoryName,
  isGraphStoreDirectoryName,
  isProjectIdDirectoryName,
} from "./graphStoreKey.ts";

/** Guard against a runaway walk if the store ever contains a symlink loop. */
const MAX_MEASURE_DEPTH = 16;

const decodeEntry = Schema.decodeUnknownEffect(Schema.fromJsonString(GraphStoreEntrySchema));
const encodeEntry = Schema.encodeEffect(Schema.fromJsonString(GraphStoreEntrySchema));

/** Contents of the dirty marker. Never read back; existence is the datum. */
const DirtyMarker = Schema.Struct({ markedAt: Schema.Number });
const encodeDirtyMarker = Schema.encodeEffect(Schema.fromJsonString(DirtyMarker));

/**
 * A validated place in the store. Only `GraphStore` can mint one, which is
 * what lets every other method treat its paths as already checked.
 */
export interface GraphStoreLocation {
  readonly projectId: ProjectId;
  readonly branch: string | null;
  readonly directoryName: string;
  /** `<root>/<projectId>/<directoryName>` */
  readonly entryDir: string;
  /** Where `GRAPHIFY_OUT` points. */
  readonly outDir: string;
  readonly metaPath: string;
}

export interface GraphStoreListing {
  readonly location: GraphStoreLocation;
  /** Null when the entry has no readable `meta.json` — a half-built entry. */
  readonly entry: GraphStoreEntry | null;
}

export class GraphStore extends Context.Service<
  GraphStore,
  {
    /** Absolute path of the store root. Exposed for logging and tests. */
    readonly root: string;
    readonly locate: (
      input: GraphStoreKeyInput & { readonly projectId: ProjectId },
    ) => Effect.Effect<GraphStoreLocation, GraphStorePathError>;
    /** Creates `graphify-out/`, so a build can be handed a real directory. */
    readonly ensure: (location: GraphStoreLocation) => Effect.Effect<void, GraphStorePathError>;
    readonly readEntry: (
      location: GraphStoreLocation,
    ) => Effect.Effect<GraphStoreEntry | null, GraphStorePathError>;
    readonly writeEntry: (
      location: GraphStoreLocation,
      entry: GraphStoreEntry,
    ) => Effect.Effect<void, GraphStorePathError>;
    /** Stamps `lastOpenedAt`; a no-op when the entry has no metadata yet. */
    readonly touch: (location: GraphStoreLocation) => Effect.Effect<void, GraphStorePathError>;
    /** True when a deferred auto-rebuild is owed. See {@link markDirty}. */
    readonly isDirty: (location: GraphStoreLocation) => Effect.Effect<boolean>;
    /**
     * Records that the checkout changed after the graph was built but the
     * rebuild was deferred because nobody had read the graph recently. A
     * marker file beside `meta.json`, so the debt survives a restart; the
     * next consumer read settles it (see `GraphService`).
     */
    readonly markDirty: (location: GraphStoreLocation) => Effect.Effect<void, GraphStorePathError>;
    /** Settles the debt. Called whenever a rebuild for the entry is queued. */
    readonly clearDirty: (location: GraphStoreLocation) => Effect.Effect<void, GraphStorePathError>;
    /** Every well-formed entry on disk. Malformed names are skipped and logged. */
    readonly list: Effect.Effect<ReadonlyArray<GraphStoreListing>, GraphStorePathError>;
    /** Bytes on disk under `entryDir`. Walks the tree, so call it sparingly. */
    readonly measure: (location: GraphStoreLocation) => Effect.Effect<number, GraphStorePathError>;
    /**
     * Recursively deletes one entry. Fails only when the guard refuses;
     * an I/O failure is logged and reported as `false` so a sweep can
     * continue rather than abort on one stuck directory.
     */
    readonly evict: (
      location: GraphStoreLocation,
      reason: string,
    ) => Effect.Effect<boolean, GraphStorePathError>;
    /** Drops a whole project directory, for a deleted or vanished project. */
    readonly evictProject: (
      projectId: string,
      reason: string,
    ) => Effect.Effect<boolean, GraphStorePathError>;
  }
>()("t3/graph/GraphStore") {}

/** The subset of an entry that {@link GraphAvailability} advertises. */
function availabilityOf(entry: GraphStoreEntry): GraphAvailability.GraphAvailability & {
  readonly workspaceRoot: string;
} {
  return {
    workspaceRoot: entry.workspaceRoot,
    branch: entry.key.branch,
    nodeCount: entry.nodeCount,
    edgeCount: entry.edgeCount,
    builtAt: entry.builtAt,
  };
}

export const make = Effect.gen(function* () {
  const config = yield* ServerConfig;
  const fs = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;

  const root = path.resolve(config.graphStoreDir);

  const refuse = (target: string, reason: string) =>
    new GraphStorePathError({ path: target, reason });

  /**
   * Resolves symlinks, tolerating a path that does not exist yet.
   *
   * Falls back to resolving the nearest existing *ancestor* and re-appending
   * the missing tail, rather than to the literal input. Both matter: a
   * not-yet-created entry must still resolve (nothing is built on first run),
   * and the store root routinely sits under a symlinked ancestor — every macOS
   * temp directory is `/var/...` → `/private/var/...`, so comparing an
   * unresolved path against a resolved one would refuse everything.
   */
  const realPathTolerant = (target: string): Effect.Effect<string> =>
    Effect.gen(function* () {
      const resolved = path.resolve(target);
      const direct = yield* fs
        .realPath(resolved)
        .pipe(Effect.catchCause(() => Effect.succeed(null)));
      if (direct !== null) return direct;
      const parent = path.dirname(resolved);
      if (parent === resolved) return resolved;
      return path.join(yield* realPathTolerant(parent), path.basename(resolved));
    });

  const isStrictlyInside = (candidate: string, container: string) => {
    const relative = path.relative(container, candidate);
    return relative !== "" && !relative.startsWith("..") && !path.isAbsolute(relative);
  };

  /**
   * The whole safety argument, in one place.
   *
   * Rebuilds the path from validated segments rather than trusting the one it
   * was handed, then requires the real path to equal that rebuild. Equality is
   * what rules out a symlinked entry: `realPath` would resolve it elsewhere,
   * and `stat` — which follows symlinks — could not tell the difference.
   */
  const guardedTargetPath = Effect.fn("GraphStore.guardedTargetPath")(function* (
    segments: ReadonlyArray<string>,
    original: string,
  ) {
    if (segments.length === 0 || segments.length > 2) {
      return yield* refuse(original, `expected 1 or 2 path segments, got ${segments.length}`);
    }
    for (const segment of segments) {
      if (segment === "" || segment === "." || segment === ".." || /[/\\]/.test(segment)) {
        return yield* refuse(original, `segment '${segment}' is not a single safe name`);
      }
    }
    const [projectSegment, entrySegment] = segments;
    if (projectSegment === undefined || !isProjectIdDirectoryName(projectSegment)) {
      return yield* refuse(original, "first segment is not a project id");
    }
    if (entrySegment !== undefined && !isGraphStoreDirectoryName(entrySegment)) {
      return yield* refuse(original, "second segment is not a store entry name");
    }

    const realRoot = yield* realPathTolerant(root);
    const expected = path.join(realRoot, ...segments);
    const actual = yield* realPathTolerant(path.join(root, ...segments));
    if (actual !== expected) {
      return yield* refuse(original, "path resolves outside its expected location (symlink?)");
    }
    if (!isStrictlyInside(actual, realRoot)) {
      return yield* refuse(original, "path is not strictly inside the graph store root");
    }
    return actual;
  });

  const locate: GraphStore["Service"]["locate"] = Effect.fn("GraphStore.locate")(function* (input) {
    if (!isProjectIdDirectoryName(input.projectId)) {
      return yield* refuse(input.projectId, "project id is not a UUID");
    }
    const directoryName = graphStoreDirectoryName(input);
    const entryDir = path.join(root, input.projectId, directoryName);
    return {
      projectId: input.projectId,
      branch: input.branch,
      directoryName,
      entryDir,
      outDir: path.join(entryDir, GRAPHIFY_OUT_DIR_NAME),
      metaPath: path.join(entryDir, GRAPH_META_FILE_NAME),
    };
  });

  const ensure: GraphStore["Service"]["ensure"] = Effect.fn("GraphStore.ensure")(
    function* (location) {
      yield* guardedTargetPath([location.projectId, location.directoryName], location.entryDir);
      yield* fs.makeDirectory(location.outDir, { recursive: true }).pipe(
        Effect.catchCause((cause) =>
          Effect.logWarning("graph-store create-entry-directory failed", {
            entryDir: location.entryDir,
            cause,
          }),
        ),
      );
    },
  );

  const readEntry: GraphStore["Service"]["readEntry"] = Effect.fn("GraphStore.readEntry")(
    function* (location) {
      const contents = yield* fs
        .readFileString(location.metaPath)
        .pipe(Effect.catchCause(() => Effect.succeed(null)));
      if (contents === null) return null;
      return yield* decodeEntry(contents).pipe(
        Effect.catchCause((cause) =>
          // A meta.json we cannot decode is treated as absent rather than
          // fatal: the entry is then rebuilt, which is the safe direction.
          Effect.logWarning("graph-store meta-decode failed", {
            metaPath: location.metaPath,
            cause,
          }).pipe(Effect.as(null)),
        ),
      );
    },
  );

  const writeEntry: GraphStore["Service"]["writeEntry"] = Effect.fn("GraphStore.writeEntry")(
    function* (location, entry) {
      yield* guardedTargetPath([location.projectId, location.directoryName], location.entryDir);
      const contents = yield* encodeEntry(entry).pipe(
        Effect.catchCause((cause) =>
          Effect.logWarning("graph-store meta-encode failed", {
            metaPath: location.metaPath,
            cause,
          }).pipe(Effect.as(null)),
        ),
      );
      if (contents === null) return;
      yield* writeFileStringAtomically({ filePath: location.metaPath, contents }).pipe(
        // Same shape as `serverSettings.ts`: the helper resolves its own
        // services, so hand it the ones this layer already holds rather than
        // leaking `FileSystem | Path` into the service's requirements.
        Effect.provideService(FileSystem.FileSystem, fs),
        Effect.provideService(Path.Path, path),
        Effect.catchCause((cause) =>
          Effect.logWarning("graph-store meta-write failed", {
            metaPath: location.metaPath,
            cause,
          }),
        ),
      );
      // Publish on write so a freshly built graph is advertised to the next
      // prompt rather than to the next sweep, which may be an hour out.
      GraphAvailability.recordGraphAvailability(entry.workspaceRoot, availabilityOf(entry));
    },
  );

  const touch: GraphStore["Service"]["touch"] = Effect.fn("GraphStore.touch")(function* (location) {
    const entry = yield* readEntry(location);
    if (entry === null) return;
    const now = yield* Clock.currentTimeMillis;
    yield* writeEntry(location, { ...entry, lastOpenedAt: now });
  });

  const dirtyPathOf = (location: GraphStoreLocation) =>
    path.join(location.entryDir, GRAPH_DIRTY_FILE_NAME);

  // A pure existence probe on a path this module computed, same trust level
  // as `readEntry`'s read of `meta.json` — hence no guard and no error.
  const isDirty: GraphStore["Service"]["isDirty"] = (location) =>
    fs.exists(dirtyPathOf(location)).pipe(Effect.orElseSucceed(() => false));

  const markDirty: GraphStore["Service"]["markDirty"] = Effect.fn("GraphStore.markDirty")(
    function* (location) {
      yield* guardedTargetPath([location.projectId, location.directoryName], location.entryDir);
      const now = yield* Clock.currentTimeMillis;
      // The timestamp is for a human reading the store, not for logic: the
      // marker's existence is the datum, so an empty fallback is still a mark.
      const contents = yield* encodeDirtyMarker({ markedAt: now }).pipe(
        Effect.orElseSucceed(() => "{}"),
      );
      yield* fs.writeFileString(dirtyPathOf(location), contents).pipe(
        Effect.catchCause((cause) =>
          Effect.logWarning("graph-store dirty-mark failed", {
            entryDir: location.entryDir,
            cause,
          }),
        ),
      );
    },
  );

  const clearDirty: GraphStore["Service"]["clearDirty"] = Effect.fn("GraphStore.clearDirty")(
    function* (location) {
      yield* guardedTargetPath([location.projectId, location.directoryName], location.entryDir);
      yield* fs.remove(dirtyPathOf(location), { force: true }).pipe(
        Effect.catchCause((cause) =>
          Effect.logWarning("graph-store dirty-clear failed", {
            entryDir: location.entryDir,
            cause,
          }),
        ),
      );
    },
  );

  const readDirectoryOrEmpty = (target: string) =>
    fs.readDirectory(target).pipe(Effect.catchCause(() => Effect.succeed([] as Array<string>)));

  const list: GraphStore["Service"]["list"] = Effect.gen(function* () {
    const listings: Array<GraphStoreListing> = [];
    for (const projectSegment of yield* readDirectoryOrEmpty(root)) {
      if (!isProjectIdDirectoryName(projectSegment)) {
        yield* Effect.logWarning("graph-store skipping unrecognised project directory", {
          root,
          name: projectSegment,
        });
        continue;
      }
      const projectDir = path.join(root, projectSegment);
      for (const entrySegment of yield* readDirectoryOrEmpty(projectDir)) {
        if (!isGraphStoreDirectoryName(entrySegment)) {
          yield* Effect.logWarning("graph-store skipping unrecognised entry directory", {
            projectDir,
            name: entrySegment,
          });
          continue;
        }
        const entryDir = path.join(projectDir, entrySegment);
        // The directory name is a one-way slug, so the branch cannot be
        // recovered from it; `meta.json` is the authority and `branch` stays
        // null until it is read.
        const location: GraphStoreLocation = {
          projectId: projectSegment as ProjectId,
          branch: null,
          directoryName: entrySegment,
          entryDir,
          outDir: path.join(entryDir, GRAPHIFY_OUT_DIR_NAME),
          metaPath: path.join(entryDir, GRAPH_META_FILE_NAME),
        };
        const entry = yield* readEntry(location);
        listings.push({
          location: entry === null ? location : { ...location, branch: entry.key.branch },
          entry,
        });
      }
    }
    // A full listing is the one moment this module knows the whole truth, so it
    // is where the availability map gets reconciled rather than merged: an
    // entry evicted since the last pass simply stops being present. The sweep
    // calls this on a schedule that fires immediately at boot, which is what
    // makes the map warm before the first prompt.
    GraphAvailability.reconcileGraphAvailability(
      listings.flatMap((listing) =>
        listing.entry === null ? [] : [availabilityOf(listing.entry)],
      ),
    );
    return listings;
  });

  const measureDirectory = (target: string, depth: number): Effect.Effect<number> =>
    Effect.gen(function* () {
      if (depth > MAX_MEASURE_DEPTH) return 0;
      let total = 0;
      for (const name of yield* readDirectoryOrEmpty(target)) {
        const child = path.join(target, name);
        const info = yield* fs.stat(child).pipe(Effect.catchCause(() => Effect.succeed(null)));
        if (info === null) continue;
        total +=
          info.type === "Directory" ? yield* measureDirectory(child, depth + 1) : Number(info.size);
      }
      return total;
    });

  const measure: GraphStore["Service"]["measure"] = Effect.fn("GraphStore.measure")(
    function* (location) {
      const target = yield* guardedTargetPath(
        [location.projectId, location.directoryName],
        location.entryDir,
      );
      return yield* measureDirectory(target, 0);
    },
  );

  /** Shared tail of `evict` / `evictProject`: log, delete, report. */
  const removeGuarded = Effect.fn("GraphStore.removeGuarded")(function* (
    target: string,
    reason: string,
  ) {
    // Logged before the delete so a mistake is visible even if the process
    // dies mid-removal. This is T3's first retention policy; being able to
    // reconstruct what it took is worth one line per eviction.
    yield* Effect.logInfo("graph-store evicting", { path: target, reason });
    const removed = yield* fs.remove(target, { recursive: true, force: true }).pipe(
      Effect.as(true),
      Effect.catchCause((cause) =>
        Effect.logWarning("graph-store evict failed", { path: target, reason, cause }).pipe(
          Effect.as(false),
        ),
      ),
    );
    return removed;
  });

  const evict: GraphStore["Service"]["evict"] = Effect.fn("GraphStore.evict")(
    function* (location, reason) {
      const target = yield* guardedTargetPath(
        [location.projectId, location.directoryName],
        location.entryDir,
      );
      return yield* removeGuarded(target, reason);
    },
  );

  const evictProject: GraphStore["Service"]["evictProject"] = Effect.fn("GraphStore.evictProject")(
    function* (projectId, reason) {
      const target = yield* guardedTargetPath([projectId], path.join(root, projectId));
      return yield* removeGuarded(target, reason);
    },
  );

  return GraphStore.of({
    root,
    locate,
    ensure,
    readEntry,
    writeEntry,
    touch,
    isDirty,
    markDirty,
    clearDirty,
    list,
    measure,
    evict,
    evictProject,
  });
});

export const layer = Layer.effect(GraphStore, make);
