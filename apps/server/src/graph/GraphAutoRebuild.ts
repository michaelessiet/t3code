/**
 * GraphAutoRebuild - keeps an already-built graph in step with the code.
 *
 * Off by default, behind `knowledgeGraph.autoRebuild`. When it is on, every
 * store entry's checkout gets a watch subscription, and a quiet period after
 * the last file change queues a structural `graphify update` for whichever
 * branch is checked out at that moment.
 *
 * ## It never builds a graph nobody asked for
 *
 * The watched set is derived from what is *already in the store*, and a change
 * is dropped unless the branch currently checked out has an entry with
 * metadata. A structural extraction of a monorepo is minutes of CPU; deciding
 * to spend that on the user's behalf, because they happened to save a file in
 * a project they once built a graph for on another branch, is not a decision
 * this reactor gets to make. "Keep it fresh" is the whole mandate.
 *
 * For the same reason the mode is always `structural`. Semantic extraction
 * costs tokens, and a file save is not consent to spend them.
 *
 * ## Why reconciliation on a timer rather than an event
 *
 * The set of things to watch changes when a graph is first built, when the
 * sweep evicts one, and when the setting is toggled — three unrelated places,
 * none of which has a hook to hang this on. One idempotent pass that diffs the
 * store against the live watches covers all three, and is the same shape
 * `GraphStoreSweep` already uses. The cost of the timer is that switching the
 * setting on takes up to one interval to take effect; the cost of the
 * alternative is three subscriptions that each have to be right.
 *
 * ## Debounce, and what it deliberately gives up
 *
 * `Stream.debounce` is trailing-edge: it fires once the watcher has been quiet
 * for {@link DEFAULT_QUIET_PERIOD_MS}, so a save storm, a branch checkout or a
 * `pnpm install` collapse into one rebuild. There is no maximum wait, which
 * means continuous editing with never a quiet gap never triggers a rebuild.
 * That is the safe direction: a graph rebuilt underneath an active edit is
 * stale again before it lands, and the panel already shows a `Stale` badge with
 * a Rebuild button next to it. Auto-rebuild is for the pause, not the flurry.
 *
 * A rebuild cannot retrigger itself: graphify writes only into T3's store,
 * which is outside the watched root.
 *
 * ## Only while someone is actually reading it
 *
 * Keeping a graph fresh is only worth CPU while something consumes it. Every
 * consumer entry point — the `graph.*` RPCs and the MCP graph tools, all of
 * which funnel through `GraphService` — stamps `lastOpenedAt` on each read,
 * and a rebuild deliberately preserves it, so the field is precisely "last
 * consumed" and it lives in `meta.json`, surviving restarts. An edit inside
 * {@link DEFAULT_CONSUMER_RECENCY_WINDOW_MS} of the last read rebuilds at full
 * cadence as before; outside it, the entry is marked dirty in the store
 * instead, and the *next read* settles the debt by queueing one background
 * refresh (see `GraphService`). A manual `graph.build` is unaffected — it
 * never passes through here.
 *
 * @module GraphAutoRebuild
 */
import * as Clock from "effect/Clock";
import * as Context from "effect/Context";
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as Fiber from "effect/Fiber";
import * as Layer from "effect/Layer";
import * as Ref from "effect/Ref";
import * as Schedule from "effect/Schedule";
import type * as Scope from "effect/Scope";
import * as Stream from "effect/Stream";

import { ServerSettingsService } from "../serverSettings.ts";
import { WorkspaceWatcher } from "../workspace/WorkspaceWatcher.ts";
import { GraphBuildWorker } from "./GraphBuildWorker.ts";
import { GraphifyRuntime } from "./GraphifyRuntime.ts";
import { GraphStore } from "./GraphStore.ts";
import { GraphWorkspaceResolver } from "./GraphWorkspaceResolver.ts";

/**
 * One minute.
 *
 * This only bounds how long a *newly relevant* checkout waits for its watch to
 * start — a graph built a moment ago, or the setting just switched on. A quiet
 * pass is one `readdir` of the store plus one settings read.
 */
const DEFAULT_RECONCILE_INTERVAL_MS = 60 * 1000;

/**
 * Thirty seconds of no file changes before a rebuild is queued.
 *
 * Short enough that stepping away from the keyboard refreshes the graph;
 * long enough that saving on every keystroke, or a checkout that rewrites
 * thousands of files, produces one build rather than a queue of them.
 */
const DEFAULT_QUIET_PERIOD_MS = 30 * 1000;

/**
 * How long to wait before resubscribing after the watch stream fails.
 *
 * `WorkspaceWatcher` already restarts its own backend with backoff, so this
 * only catches the outer failures — a root that has gone missing, most likely
 * mid-rebase or on an unmounted volume. Retrying forever rather than giving up
 * matters because nothing else would ever restart the fiber; reconciliation
 * only notices roots appearing and disappearing from the store, not dead
 * watches.
 */
const WATCH_RESTART_DELAY_MS = 30 * 1000;

/**
 * Thirty minutes — how recently the graph must have been *read* for an edit
 * to trigger a rebuild rather than a dirty mark.
 *
 * Long enough to span the pauses of an agent session that is actively citing
 * the graph (queries land minutes apart, not seconds); short enough that an
 * idle-but-open project stops burning CPU on save bursts within one window.
 * The trade-off for a stale first answer after a long gap is deliberate:
 * every read already carries a `stale` flag, and the deferred rebuild is
 * queued the moment that first read arrives.
 */
const DEFAULT_CONSUMER_RECENCY_WINDOW_MS = 30 * 60 * 1000;

export interface GraphAutoRebuildOptions {
  readonly reconcileIntervalMs?: number;
  readonly quietPeriodMs?: number;
  readonly consumerRecencyWindowMs?: number;
}

export class GraphAutoRebuild extends Context.Service<
  GraphAutoRebuild,
  {
    /**
     * Runs one reconciliation and returns the roots being watched afterwards.
     * Exposed so tests can drive it without a scheduler.
     */
    readonly reconcile: Effect.Effect<ReadonlyArray<string>, never, Scope.Scope>;
    /** Forks the recurring reconciliation into the caller's scope. */
    readonly start: () => Effect.Effect<void, never, Scope.Scope>;
  }
>()("t3/graph/GraphAutoRebuild") {}

export const make = (options?: GraphAutoRebuildOptions) =>
  Effect.gen(function* () {
    const settingsService = yield* ServerSettingsService;
    const store = yield* GraphStore;
    const resolver = yield* GraphWorkspaceResolver;
    const runtime = yield* GraphifyRuntime;
    const worker = yield* GraphBuildWorker;
    const watcher = yield* WorkspaceWatcher;

    const reconcileIntervalMs = Math.max(
      1,
      options?.reconcileIntervalMs ?? DEFAULT_RECONCILE_INTERVAL_MS,
    );
    const quietPeriod = Duration.millis(
      Math.max(1, options?.quietPeriodMs ?? DEFAULT_QUIET_PERIOD_MS),
    );
    const consumerRecencyWindowMs = Math.max(
      1,
      options?.consumerRecencyWindowMs ?? DEFAULT_CONSUMER_RECENCY_WINDOW_MS,
    );

    /** Watch fibers by workspace root, owned by the scope `start` was given. */
    const watchesRef = yield* Ref.make<ReadonlyMap<string, Fiber.Fiber<void>>>(new Map());

    /**
     * Both switches, read fresh.
     *
     * A settings read that fails is treated as off, the same way every other
     * gate in this feature treats it: an unreadable policy is not a licence to
     * spawn builds.
     */
    const armed = settingsService.getSettings.pipe(
      Effect.map(
        (settings) =>
          settings.knowledgeGraph.enabled === true && settings.knowledgeGraph.autoRebuild === true,
      ),
      Effect.orElseSucceed(() => false),
    );

    const rebuild = Effect.fn("GraphAutoRebuild.rebuild")(function* (workspaceRoot: string) {
      // Re-read rather than trusting the last reconciliation: the interval is
      // a minute, and a user who has just switched auto-rebuild off should not
      // get one more build out of it.
      if (!(yield* armed)) return;

      // Resolved now, not when the watch started: after a branch switch the
      // files that changed belong to a different entry, and building the old
      // one would write this branch's content into that branch's graph.
      const resolved = yield* resolver
        .resolve(workspaceRoot)
        .pipe(Effect.catchCause(() => Effect.succeed(null)));
      if (resolved === null) return;

      const entry = yield* store
        .readEntry(resolved.location)
        .pipe(Effect.orElseSucceed(() => null));
      if (entry === null) return;

      // A graph nobody has read within the recency window is not worth CPU on
      // every save burst. Record the debt instead and stand down; the next
      // consumer read settles it (see `GraphService`). `lastOpenedAt` is
      // stamped by every read and preserved across rebuilds, so it is exactly
      // "last consumed".
      const now = yield* Clock.currentTimeMillis;
      if (now - entry.lastOpenedAt > consumerRecencyWindowMs) {
        yield* store
          .markDirty(resolved.location)
          .pipe(
            Effect.catchCause((cause) =>
              Effect.logWarning("graph.autoRebuild.mark-dirty-failed", { workspaceRoot, cause }),
            ),
          );
        yield* Effect.logInfo("graph.autoRebuild.deferred", {
          projectId: resolved.location.projectId,
          branch: resolved.branch,
        });
        return;
      }

      const status = yield* worker.statusFor(resolved.location);
      if (status.state === "queued" || status.state === "running") return;

      // Queueing a job that can only fail would leave the panel showing a
      // build failure nobody asked for. A manual build reports a missing
      // toolchain to the person who clicked the button; this one stays quiet.
      const ready = yield* runtime.resolve.pipe(
        Effect.as(true),
        Effect.catchCause(() => Effect.succeed(false)),
      );
      if (!ready) return;

      // A rebuild being queued settles any dirty mark left by an earlier
      // deferral: whatever debt existed, this build pays it.
      yield* store.clearDirty(resolved.location).pipe(Effect.ignore);
      yield* worker.request({
        location: resolved.location,
        workspaceRoot: resolved.workspaceRoot,
        headSha: resolved.headSha,
        mode: "structural",
        force: false,
      });
      yield* Effect.logInfo("graph.autoRebuild.requested", {
        projectId: resolved.location.projectId,
        branch: resolved.branch,
      });
    });

    const watchRoot = (workspaceRoot: string) =>
      watcher.subscribe({ cwd: workspaceRoot }).pipe(
        Stream.debounce(quietPeriod),
        Stream.runForEach(() => rebuild(workspaceRoot)),
        Effect.tapError((cause) =>
          Effect.logWarning("graph.autoRebuild.watch-failed", { workspaceRoot, cause }),
        ),
        Effect.retry(Schedule.spaced(Duration.millis(WATCH_RESTART_DELAY_MS))),
        // Unreachable while the schedule above never gives up, but it closes
        // the error channel rather than leaving a fiber that can die silently.
        Effect.catchCause((cause) =>
          Effect.logError("graph.autoRebuild.watch-stopped", { workspaceRoot, cause }),
        ),
        Effect.asVoid,
      );

    /**
     * Checkouts worth watching: those with a built entry in the store.
     *
     * `entry.workspaceRoot` rather than the project root, because that is the
     * directory graphify actually scanned — for a worktree the two differ, and
     * watching the project root would miss every change the worktree makes.
     */
    const desiredRoots = Effect.fn("GraphAutoRebuild.desiredRoots")(function* () {
      if (!(yield* armed)) return [] as ReadonlyArray<string>;
      const listings = yield* store.list.pipe(Effect.orElseSucceed(() => []));
      const roots = new Set<string>();
      for (const listing of listings) {
        if (listing.entry === null) continue;
        roots.add(listing.entry.workspaceRoot);
      }
      return [...roots] as ReadonlyArray<string>;
    });

    const reconcile: GraphAutoRebuild["Service"]["reconcile"] = Effect.gen(function* () {
      const scope = yield* Effect.scope;
      const desired = new Set(yield* desiredRoots());
      const active = yield* Ref.get(watchesRef);
      const next = new Map(active);

      for (const [root, fiber] of active) {
        if (desired.has(root)) continue;
        yield* Fiber.interrupt(fiber).pipe(Effect.ignore);
        next.delete(root);
      }
      for (const root of desired) {
        if (next.has(root)) continue;
        next.set(root, yield* watchRoot(root).pipe(Effect.forkIn(scope)));
      }

      yield* Ref.set(watchesRef, next);
      return [...next.keys()];
    });

    const start: GraphAutoRebuild["Service"]["start"] = () =>
      Effect.gen(function* () {
        yield* Effect.forkScoped(
          reconcile.pipe(
            Effect.catchCause((cause) =>
              Effect.logWarning("graph.autoRebuild.reconcile-failed", { cause }),
            ),
            // `Schedule.spaced` fires once immediately, so a graph built before
            // the last restart is watched shortly after boot.
            Effect.repeat(Schedule.spaced(Duration.millis(reconcileIntervalMs))),
          ),
        );
        yield* Effect.logInfo("graph.autoRebuild.started", { reconcileIntervalMs });
      });

    return GraphAutoRebuild.of({ reconcile, start });
  });

export const makeLayer = (options?: GraphAutoRebuildOptions) =>
  Layer.effect(GraphAutoRebuild, make(options));

export const layer = makeLayer();
