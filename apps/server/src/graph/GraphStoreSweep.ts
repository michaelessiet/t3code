/**
 * GraphStoreSweep - the knowledge-graph store's garbage collector.
 *
 * Every eviction trigger is reconciliation against a source of truth, on a
 * timer, rather than a hook on the event that caused it. That is deliberate.
 * `project.delete` is a soft delete that touches no disk at all, and nothing
 * in the server observes branch deletion — there is no `deleteBranch` on
 * `GitVcsDriver` and nothing watches `.git/HEAD`. Adding observation machinery
 * for two rare events would be more code and more failure modes than one
 * idempotent pass that compares the store against the database and git.
 * `runAttachmentSideEffects` already establishes reconcile-against-the-
 * projection as the house pattern for disk cleanup; this is the same shape on
 * a schedule.
 *
 * ## Why a branch needs two passes
 *
 * A branch legitimately disappears mid-rebase, and during a `git checkout`
 * that recreates it. Dropping a graph the first time the branch is missing
 * would delete work a user is in the middle of. So a missing branch is
 * recorded, and only an entry missing on two *consecutive* sweeps is evicted.
 * The record lives in memory: a restart forgetting a strike costs one extra
 * sweep interval, which is the safe direction to be wrong in.
 *
 * ## What this deletes, and why it logs everything
 *
 * This is the only code in the feature that recursively deletes computed
 * paths, and its inputs — branch names, directory names read back off disk —
 * are not fully trusted. `GraphStore.evict` owns the containment guard; this
 * module owns the decision. Every removal is logged with its reason, so a
 * mistake is at least visible after the fact.
 *
 * Cleanup is best-effort throughout: a failed read or unlink logs and the
 * sweep moves to the next entry. A GC that aborts on the first stuck directory
 * would silently stop reclaiming anything.
 *
 * @module GraphStoreSweep
 */
import type { GraphStoreEntry, ProjectId } from "@t3tools/contracts";
import * as Clock from "effect/Clock";
import * as Context from "effect/Context";
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Option from "effect/Option";
import * as Ref from "effect/Ref";
import * as Schedule from "effect/Schedule";
import type * as Scope from "effect/Scope";

import { ProjectionSnapshotQuery } from "../orchestration/Services/ProjectionSnapshotQuery.ts";
import { ServerSettingsService } from "../serverSettings.ts";
import * as GitVcsDriver from "../vcs/GitVcsDriver.ts";
import { GRAPHIFY_PINNED_VERSION } from "./graphifyDetection.ts";
import { GraphStore, type GraphStoreListing } from "./GraphStore.ts";
import { WorkspaceGraph } from "./WorkspaceGraph.ts";

/**
 * Six hours.
 *
 * Retention is measured in days and the store is bounded in gigabytes, so
 * there is nothing here that needs minute-level reaction. A quiet sweep is a
 * handful of `stat` calls plus one `git branch` per project.
 */
const DEFAULT_SWEEP_INTERVAL_MS = 6 * 60 * 60 * 1000;

const MS_PER_DAY = 24 * 60 * 60 * 1000;

export interface GraphStoreSweepOptions {
  readonly sweepIntervalMs?: number;
}

export interface GraphStoreSweepReport {
  readonly scanned: number;
  readonly evicted: number;
  readonly bytesReclaimed: number;
}

export class GraphStoreSweep extends Context.Service<
  GraphStoreSweep,
  {
    /** Runs one pass. Exposed so tests can drive it without a scheduler. */
    readonly sweep: Effect.Effect<GraphStoreSweepReport>;
    /** Forks the recurring pass into the caller's scope. */
    readonly start: () => Effect.Effect<void, never, Scope.Scope>;
  }
>()("t3/graph/GraphStoreSweep") {}

/** Why an entry is being removed. Logged verbatim and used by tests. */
export type GraphEvictionReason =
  | "project-deleted"
  | "workspace-missing"
  | "branch-deleted"
  | "expired"
  | "version-mismatch"
  | "over-budget";

/**
 * Age-based verdict for one entry.
 *
 * Pure and exported so the retention rule can be table-tested without a
 * filesystem. An entry with no readable `meta.json` is *not* expired: it is a
 * half-built entry whose age is unknown, and guessing "old" there would delete
 * a build that is running right now.
 */
export function isExpired(input: {
  readonly entry: GraphStoreEntry | null;
  readonly retentionDays: number;
  readonly now: number;
}): boolean {
  if (input.retentionDays <= 0) return false;
  if (input.entry === null) return false;
  return input.now - input.entry.lastOpenedAt > input.retentionDays * MS_PER_DAY;
}

/**
 * Which entries to drop to get back under the size budget.
 *
 * Least-recently-opened first, and only as many as it takes. Entries without
 * metadata sort oldest — they carry no `lastOpenedAt` to defend themselves
 * with, and an entry that never finished building is the cheapest thing to
 * lose. Returns directory names so the caller stays in charge of the paths.
 */
export function selectOverBudget(input: {
  readonly entries: ReadonlyArray<{
    readonly directoryName: string;
    readonly lastOpenedAt: number | null;
    readonly sizeBytes: number;
  }>;
  readonly budgetBytes: number;
}): ReadonlyArray<string> {
  if (input.budgetBytes <= 0) return [];
  let total = input.entries.reduce((sum, entry) => sum + entry.sizeBytes, 0);
  if (total <= input.budgetBytes) return [];

  const oldestFirst = [...input.entries].sort(
    (a, b) => (a.lastOpenedAt ?? 0) - (b.lastOpenedAt ?? 0),
  );
  const doomed: Array<string> = [];
  for (const entry of oldestFirst) {
    if (total <= input.budgetBytes) break;
    doomed.push(entry.directoryName);
    total -= entry.sizeBytes;
  }
  return doomed;
}

export const make = (options?: GraphStoreSweepOptions) =>
  Effect.gen(function* () {
    const store = yield* GraphStore;
    const graphs = yield* WorkspaceGraph;
    const projections = yield* ProjectionSnapshotQuery;
    const git = yield* GitVcsDriver.GitVcsDriver;
    const settingsService = yield* ServerSettingsService;
    const fs = yield* FileSystem.FileSystem;

    const sweepIntervalMs = Math.max(1, options?.sweepIntervalMs ?? DEFAULT_SWEEP_INTERVAL_MS);

    /**
     * Entries seen without their branch on the previous pass.
     *
     * Keyed by `<projectId>/<directoryName>`, which is what makes a strike
     * survive an unrelated entry appearing or disappearing between passes.
     */
    const branchStrikesRef = yield* Ref.make<ReadonlySet<string>>(new Set());

    const strikeKey = (listing: GraphStoreListing) =>
      `${listing.location.projectId}/${listing.location.directoryName}`;

    const drop = (listing: GraphStoreListing, reason: GraphEvictionReason) =>
      Effect.gen(function* () {
        // The recorded size, when there is one — the build worker measured it
        // already and walking the tree again just to log a number would make
        // every eviction cost a full directory scan.
        const sizeBytes =
          listing.entry !== null
            ? listing.entry.sizeBytes
            : yield* store.measure(listing.location).pipe(Effect.orElseSucceed(() => 0));
        const removed = yield* store
          .evict(listing.location, reason)
          .pipe(Effect.catchCause(() => Effect.succeed(false)));
        if (!removed) return 0;
        // The parsed graph outlives its files otherwise, and a reader holding
        // the cache would keep serving a graph that is no longer on disk.
        yield* graphs.forget(listing.location);
        yield* Effect.logInfo("graph.store.sweep.evicted", {
          projectId: listing.location.projectId,
          directoryName: listing.location.directoryName,
          branch: listing.location.branch,
          reason,
          sizeBytes,
        });
        return sizeBytes;
      });

    /** Live projects the store knows about, and where they live on disk. */
    const projectWorkspace = (projectId: ProjectId) =>
      projections.getProjectShellById(projectId).pipe(
        Effect.map(Option.getOrNull),
        // A projection read failure is not evidence that a project is gone,
        // and treating it as such would delete every graph during a database
        // hiccup. `undefined` means "do not judge this project this pass".
        Effect.catchCause((cause) =>
          Effect.logWarning("graph.store.sweep.project-lookup-failed", { projectId, cause }).pipe(
            Effect.as(undefined),
          ),
        ),
      );

    /**
     * Branch names in a checkout, or `null` when git could not answer.
     *
     * Null is load-bearing: "git failed" and "the branch list is empty" must
     * not look the same, because the second would evict every entry for the
     * project.
     */
    const localBranches = (cwd: string) =>
      git.listLocalBranchNames(cwd).pipe(
        Effect.map((names) => new Set(names) as ReadonlySet<string>),
        Effect.catchCause(() => Effect.succeed(null)),
      );

    const sweep: GraphStoreSweep["Service"]["sweep"] = Effect.gen(function* () {
      const settings = yield* settingsService.getSettings.pipe(
        Effect.catchCause(() => Effect.succeed(null)),
      );
      // Retention is a user setting, so a settings read failure means the
      // policy is unknown and nothing should be deleted under a guess.
      if (settings === null) return { scanned: 0, evicted: 0, bytesReclaimed: 0 };

      const listings = yield* store.list.pipe(Effect.catchCause(() => Effect.succeed([])));
      if (listings.length === 0) {
        yield* Ref.set(branchStrikesRef, new Set());
        return { scanned: 0, evicted: 0, bytesReclaimed: 0 };
      }

      const now = yield* Clock.currentTimeMillis;
      const previousStrikes = yield* Ref.get(branchStrikesRef);
      const nextStrikes = new Set<string>();
      const survivors: Array<GraphStoreListing> = [];
      let evicted = 0;
      let bytesReclaimed = 0;

      // One lookup per project, not per entry: a project with fifteen branches
      // would otherwise mean fifteen identical database reads and fifteen
      // `git branch` spawns.
      const projectIds = new Set(listings.map((listing) => listing.location.projectId));
      const branches = new Map<ProjectId, ReadonlySet<string> | null>();
      /** Projects to skip entirely this pass, because their state is unknown. */
      const unknown = new Set<ProjectId>();
      /** Projects whose whole directory has just gone. */
      const dropped = new Set<ProjectId>();
      for (const projectId of projectIds) {
        const project = yield* projectWorkspace(projectId);
        if (project === undefined) {
          unknown.add(projectId);
          continue;
        }
        const exists =
          project !== null &&
          (yield* fs.exists(project.workspaceRoot).pipe(Effect.orElseSucceed(() => false)));
        if (project !== null && exists) {
          branches.set(projectId, yield* localBranches(project.workspaceRoot));
          continue;
        }

        // The whole project directory goes at once rather than entry by entry:
        // every entry under it has already reached the same verdict, and
        // `evictProject` on an already-removed directory reports success, so
        // per-entry calls would inflate the counts and the log.
        const reason: GraphEvictionReason =
          project === null ? "project-deleted" : "workspace-missing";
        const removed = yield* store
          .evictProject(projectId, reason)
          .pipe(Effect.catchCause(() => Effect.succeed(false)));
        if (!removed) {
          unknown.add(projectId);
          continue;
        }
        dropped.add(projectId);
        for (const listing of listings) {
          if (listing.location.projectId !== projectId) continue;
          yield* graphs.forget(listing.location);
          evicted += 1;
          bytesReclaimed += listing.entry?.sizeBytes ?? 0;
        }
        yield* Effect.logInfo("graph.store.sweep.evicted-project", {
          projectId,
          reason,
        });
      }

      for (const listing of listings) {
        const projectId = listing.location.projectId;
        if (dropped.has(projectId)) continue;

        // Unknown project state this pass — leave the entry entirely alone,
        // including its branch strike, so a database blip cannot accumulate
        // strikes towards a deletion.
        if (unknown.has(projectId)) {
          if (previousStrikes.has(strikeKey(listing))) nextStrikes.add(strikeKey(listing));
          survivors.push(listing);
          continue;
        }

        if (listing.entry !== null && listing.entry.graphifyVersion !== GRAPHIFY_PINNED_VERSION) {
          bytesReclaimed += yield* drop(listing, "version-mismatch");
          evicted += 1;
          continue;
        }

        if (
          isExpired({
            entry: listing.entry,
            retentionDays: settings.knowledgeGraph.retentionDays,
            now,
          })
        ) {
          bytesReclaimed += yield* drop(listing, "expired");
          evicted += 1;
          continue;
        }

        const branch = listing.location.branch;
        const known = branches.get(projectId);
        // A detached-HEAD entry has no branch to compare against, and `known
        // === null` means git did not answer.
        if (branch !== null && known != null && !known.has(branch)) {
          if (previousStrikes.has(strikeKey(listing))) {
            bytesReclaimed += yield* drop(listing, "branch-deleted");
            evicted += 1;
            continue;
          }
          nextStrikes.add(strikeKey(listing));
        }

        survivors.push(listing);
      }

      const budgetBytes = settings.knowledgeGraph.maxStoreMegabytes * 1024 * 1024;
      const doomed = new Set(
        selectOverBudget({
          budgetBytes,
          entries: survivors.map((listing) => ({
            directoryName: `${listing.location.projectId}/${listing.location.directoryName}`,
            lastOpenedAt: listing.entry?.lastOpenedAt ?? null,
            sizeBytes: listing.entry?.sizeBytes ?? 0,
          })),
        }),
      );
      for (const listing of survivors) {
        if (!doomed.has(strikeKey(listing))) continue;
        bytesReclaimed += yield* drop(listing, "over-budget");
        nextStrikes.delete(strikeKey(listing));
        evicted += 1;
      }

      yield* Ref.set(branchStrikesRef, nextStrikes);

      if (evicted > 0) {
        yield* Effect.logInfo("graph.store.sweep-complete", {
          scanned: listings.length,
          evicted,
          bytesReclaimed,
        });
      }
      return { scanned: listings.length, evicted, bytesReclaimed };
    });

    const start: GraphStoreSweep["Service"]["start"] = () =>
      Effect.gen(function* () {
        yield* Effect.forkScoped(
          sweep.pipe(
            Effect.catchCause((cause) => Effect.logWarning("graph.store.sweep-failed", { cause })),
            // `Schedule.spaced` fires once immediately, so the store is
            // reconciled shortly after boot without blocking startup.
            Effect.repeat(Schedule.spaced(Duration.millis(sweepIntervalMs))),
          ),
        );
        yield* Effect.logInfo("graph.store.sweep.started", { sweepIntervalMs });
      });

    return GraphStoreSweep.of({ sweep, start });
  });

export const makeLayer = (options?: GraphStoreSweepOptions) =>
  Layer.effect(GraphStoreSweep, make(options));

export const layer = makeLayer();
