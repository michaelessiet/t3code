/**
 * WorkspaceWatcher - Effect service for streaming workspace file changes.
 *
 * Maintains one recursive filesystem watcher per workspace root, shared by
 * all subscribers with refcounting (mirroring VcsStatusBroadcaster's poller
 * registry). Raw watch events are normalized to workspace-root-relative `/`
 * separated paths, filtered for high-churn directories, deduplicated, and
 * coalesced into batches before being published.
 *
 * Change events are advisory ("this path may have changed"): change-kind
 * classification is deliberately not exposed because it is unreliable across
 * watch backends and platforms. Consumers re-read paths and compare content
 * revisions (see `@t3tools/shared/fileRevision`).
 *
 * @module WorkspaceWatcher
 */
import type { ProjectWatchStreamEvent } from "@t3tools/contracts";
import * as Context from "effect/Context";
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as Exit from "effect/Exit";
import * as Fiber from "effect/Fiber";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Path from "effect/Path";
import * as PubSub from "effect/PubSub";
import * as Result from "effect/Result";
import * as Schedule from "effect/Schedule";
import * as Scope from "effect/Scope";
import * as Stream from "effect/Stream";
import * as SynchronizedRef from "effect/SynchronizedRef";

import * as WorkspacePaths from "./WorkspacePaths.ts";

/** Coalescing window for raw watch events. */
const WATCH_BATCH_WINDOW = Duration.millis(200);
/** Maximum raw events folded into one batch before it is flushed early. */
const WATCH_BATCH_MAX_EVENTS = 1024;
/** Unique-path cap per batch; larger batches degrade to an overflow event. */
const WATCH_BATCH_MAX_PATHS = 512;
/** Restart backoff after a watch backend failure. */
const WATCH_RESTART_BASE_DELAY = Duration.seconds(1);
const WATCH_RESTART_MAX_DELAY = Duration.minutes(1);

/**
 * Path segments whose churn is high and whose contents no open-buffer or
 * file-tree consumer edits directly. Events beneath them are dropped.
 */
const IGNORED_SEGMENTS: ReadonlySet<string> = new Set([".git", "node_modules"]);

type WatchChange = {
  readonly cwd: string;
  readonly event: ProjectWatchStreamEvent;
};

type ActiveWatch = {
  readonly fiber: Fiber.Fiber<void>;
  readonly subscriberCount: number;
};

/** Service tag for workspace change watching. */
export class WorkspaceWatcher extends Context.Service<
  WorkspaceWatcher,
  {
    /**
     * Subscribe to coalesced change events beneath a workspace root. The
     * stream starts with the subscription itself — no initial snapshot is
     * emitted; consumers already hold the state they care about.
     */
    readonly subscribe: (input: {
      readonly cwd: string;
    }) => Stream.Stream<
      ProjectWatchStreamEvent,
      | WorkspacePaths.WorkspaceRootNotExistsError
      | WorkspacePaths.WorkspaceRootCreateFailedError
      | WorkspacePaths.WorkspaceRootStatFailedError
      | WorkspacePaths.WorkspaceRootNotDirectoryError
    >;
  }
>()("t3/workspace/WorkspaceWatcher") {}

/**
 * Normalize a raw watch-event path to a workspace-root-relative posix path,
 * or `null` when the event should be dropped (outside the root, in an
 * ignored directory, or the root itself).
 */
export function normalizeWatchPath(
  path: Path.Path,
  workspaceRoot: string,
  eventPath: string,
): string | null {
  const relative = path.isAbsolute(eventPath)
    ? path.relative(workspaceRoot, eventPath)
    : path.normalize(eventPath);
  if (relative.length === 0 || relative === "." || relative.startsWith("..")) return null;
  const posixPath = relative.replaceAll("\\", "/");
  for (const segment of posixPath.split("/")) {
    if (IGNORED_SEGMENTS.has(segment)) return null;
  }
  return posixPath;
}

/**
 * Fold a batch of normalized paths into a single stream event, degrading to
 * `overflow` when the batch exceeds the unique-path cap.
 */
export function coalesceWatchPaths(paths: ReadonlyArray<string>): ProjectWatchStreamEvent | null {
  const unique = new Set(paths);
  if (unique.size === 0) return null;
  if (unique.size > WATCH_BATCH_MAX_PATHS) return { _tag: "overflow" };
  return { _tag: "changes", paths: [...unique] };
}

export const make = Effect.gen(function* () {
  const fileSystem = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;
  const workspacePaths = yield* WorkspacePaths.WorkspacePaths;

  const changesPubSub = yield* Effect.acquireRelease(PubSub.unbounded<WatchChange>(), (pubsub) =>
    PubSub.shutdown(pubsub),
  );
  const watcherScope = yield* Effect.acquireRelease(Scope.make(), (scope) =>
    Scope.close(scope, Exit.void),
  );
  const watchesRef = yield* SynchronizedRef.make(new Map<string, ActiveWatch>());

  const publish = (cwd: string, event: ProjectWatchStreamEvent) =>
    PubSub.publish(changesPubSub, { cwd, event }).pipe(Effect.asVoid);

  /**
   * One shared watch loop per workspace root. Watch backend failures publish
   * an `overflow` (so subscribers refresh anything they hold) and the watch
   * is restarted with backoff; the subscription streams stay alive across
   * restarts.
   */
  const makeWatchLoop = (cwd: string) =>
    fileSystem.watch(cwd).pipe(
      Stream.filterMap((event) => {
        const normalized = normalizeWatchPath(path, cwd, event.path);
        return normalized === null ? Result.failVoid : Result.succeed(normalized);
      }),
      Stream.groupedWithin(WATCH_BATCH_MAX_EVENTS, WATCH_BATCH_WINDOW),
      Stream.filterMap((batch) => {
        const event = coalesceWatchPaths(batch);
        return event === null ? Result.failVoid : Result.succeed(event);
      }),
      Stream.runForEach((event) => publish(cwd, event)),
      Effect.tapError((cause) =>
        Effect.logWarning("WorkspaceWatcher watch backend failed; restarting", {
          cwd,
          cause,
        }).pipe(Effect.andThen(publish(cwd, { _tag: "overflow" }))),
      ),
      Effect.retry(
        Schedule.exponential(WATCH_RESTART_BASE_DELAY).pipe(
          Schedule.either(Schedule.spaced(WATCH_RESTART_MAX_DELAY)),
        ),
      ),
      // The retry schedule never gives up, but close the error channel anyway
      // so an impossible escape is at least logged instead of killing the
      // shared watcher fiber silently.
      Effect.catchCause((cause) =>
        Effect.logError("WorkspaceWatcher watch loop terminated unexpectedly", { cwd, cause }),
      ),
      Effect.asVoid,
    );

  const retainWatch = Effect.fn("WorkspaceWatcher.retainWatch")(function* (cwd: string) {
    yield* SynchronizedRef.modifyEffect(watchesRef, (activeWatches) => {
      const existing = activeWatches.get(cwd);
      if (existing) {
        const nextWatches = new Map(activeWatches);
        nextWatches.set(cwd, {
          ...existing,
          subscriberCount: existing.subscriberCount + 1,
        });
        return Effect.succeed([undefined, nextWatches] as const);
      }

      return makeWatchLoop(cwd).pipe(
        Effect.forkIn(watcherScope),
        Effect.map((fiber) => {
          const nextWatches = new Map(activeWatches);
          nextWatches.set(cwd, { fiber, subscriberCount: 1 });
          return [undefined, nextWatches] as const;
        }),
      );
    });
  });

  const releaseWatch = Effect.fn("WorkspaceWatcher.releaseWatch")(function* (cwd: string) {
    const watchToInterrupt = yield* SynchronizedRef.modify(watchesRef, (activeWatches) => {
      const existing = activeWatches.get(cwd);
      if (!existing) {
        return [null, activeWatches] as const;
      }

      if (existing.subscriberCount > 1) {
        const nextWatches = new Map(activeWatches);
        nextWatches.set(cwd, {
          ...existing,
          subscriberCount: existing.subscriberCount - 1,
        });
        return [null, nextWatches] as const;
      }

      const nextWatches = new Map(activeWatches);
      nextWatches.delete(cwd);
      return [existing.fiber, nextWatches] as const;
    });

    if (watchToInterrupt) {
      yield* Fiber.interrupt(watchToInterrupt).pipe(Effect.ignore);
    }
  });

  const subscribe: WorkspaceWatcher["Service"]["subscribe"] = (input) =>
    Stream.unwrap(
      Effect.gen(function* () {
        const normalizedCwd = yield* workspacePaths.normalizeWorkspaceRoot(input.cwd);
        // Watch the real path: watch backends (notably FSEvents on macOS)
        // report realpath'd locations, and Node strips the watched-path
        // prefix from event paths — a symlinked root (e.g. `/var` →
        // `/private/var`) would otherwise break that prefix match and yield
        // useless event paths. Realpathing also keys the shared-watch
        // registry consistently across symlinked spellings of one root.
        const cwd = yield* fileSystem
          .realPath(normalizedCwd)
          .pipe(Effect.catchCause(() => Effect.succeed(normalizedCwd)));
        const subscription = yield* PubSub.subscribe(changesPubSub);
        yield* retainWatch(cwd);
        const release = releaseWatch(cwd).pipe(Effect.ignore, Effect.asVoid);

        return Stream.fromSubscription(subscription).pipe(
          Stream.filter((change) => change.cwd === cwd),
          Stream.map((change) => change.event),
          Stream.ensuring(release),
        );
      }),
    );

  return WorkspaceWatcher.of({ subscribe });
});

export const layer = Layer.effect(WorkspaceWatcher, make);
