/**
 * GraphWorkspaceResolver - turns a thread's `cwd` into a store location.
 *
 * Every graph RPC takes a `cwd`, because that is what the client has. The store
 * is keyed by `(projectId, branch)`, so something has to bridge the two, and it
 * is deliberately the server's job: the client should never need to know the
 * store layout in order to name an entry.
 *
 * ## Why this is more than one database lookup
 *
 * `getActiveProjectByWorkspaceRoot` is an **exact string match** on
 * `projection_projects.workspace_root` with no normalisation
 * (`ProjectionSnapshotQuery.ts:126`). A thread whose `cwd` is a subdirectory, a
 * symlinked path, or a git worktree therefore matches nothing at all, and the
 * panel would report "not a T3 project" inside a project the user is plainly
 * looking at. So the resolver tries, in order:
 *
 * 1. the path as given,
 * 2. its real path, and each ancestor of that path,
 * 3. the *main* worktree's root, via `git rev-parse --git-common-dir`.
 *
 * Step 3 is what makes worktrees work. A worktree lives under
 * `<baseDir>/worktrees/…`, nowhere near the project root, so no amount of
 * walking upwards finds it — but `--git-common-dir` points at the original
 * repository's `.git`, whose parent is the checkout T3 registered.
 *
 * ## Which directory graphify actually scans
 *
 * The project id comes from the match above; the directory to scan does **not**.
 * A worktree is attributed to its parent project but holds a *different
 * branch's* files, so scanning the project root there would build a graph
 * labelled `feat/x` out of `main`'s content. The scan root is therefore
 * `git rev-parse --show-toplevel` — the checkout that really contains the branch
 * — falling back to the registered project root outside a repository. For a
 * thread started in a subdirectory the two coincide, which is also right: a
 * project's graph should not be a slice of the project.
 *
 * When nothing matches, that is a `GraphWorkspaceUnknownError` rather than a
 * silently-invented key. A graph stored under a made-up project id could never
 * be attributed or reclaimed by the sweep.
 *
 * @module GraphWorkspaceResolver
 */
import {
  type GraphStorePathError,
  GraphWorkspaceUnknownError,
  type ProjectId,
} from "@t3tools/contracts";
import * as Context from "effect/Context";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Option from "effect/Option";
import * as Path from "effect/Path";

import * as ProjectionSnapshotQuery from "../orchestration/Services/ProjectionSnapshotQuery.ts";
import * as GitVcsDriver from "../vcs/GitVcsDriver.ts";
import { GraphStore, type GraphStoreLocation } from "./GraphStore.ts";

/**
 * How far up the tree to look for a registered project root. Deep enough for a
 * thread started in `packages/x/src/y`, shallow enough not to walk to `/`.
 */
const MAX_ANCESTOR_LOOKUPS = 12;

export interface ResolvedGraphWorkspace {
  readonly projectId: ProjectId;
  /** Directory graphify scans — the checkout holding this branch's files. */
  readonly workspaceRoot: string;
  /** Null for a detached HEAD, or when the directory is not a git checkout. */
  readonly branch: string | null;
  readonly headSha: string | null;
  readonly location: GraphStoreLocation;
}

export class GraphWorkspaceResolver extends Context.Service<
  GraphWorkspaceResolver,
  {
    readonly resolve: (
      cwd: string,
    ) => Effect.Effect<ResolvedGraphWorkspace, GraphWorkspaceUnknownError | GraphStorePathError>;
  }
>()("t3/graph/GraphWorkspaceResolver") {}

export const make = Effect.gen(function* () {
  const store = yield* GraphStore;
  const projections = yield* ProjectionSnapshotQuery.ProjectionSnapshotQuery;
  const git = yield* GitVcsDriver.GitVcsDriver;
  const fs = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;

  /** Runs one git command for its stdout, treating any failure as "no answer". */
  const gitLine = (
    operation: string,
    cwd: string,
    args: ReadonlyArray<string>,
  ): Effect.Effect<string | null> =>
    git.execute({ operation, cwd, args }).pipe(
      Effect.map((result) => {
        const line = result.stdout.trim();
        return line === "" ? null : line;
      }),
      // Not a repository, no commits yet, git missing, a stubbed driver in a
      // test: all mean "no answer". `catchCause` rather than `orElseSucceed`
      // because an unimplemented or crashing driver is a defect, and a probe
      // whose whole contract is "answer or don't" should not take a status
      // read down with it.
      Effect.catchCause(() => Effect.succeed(null)),
    );

  const projectAt = (
    candidate: string,
  ): Effect.Effect<{ readonly id: ProjectId; readonly workspaceRoot: string } | null> =>
    projections.getActiveProjectByWorkspaceRoot(candidate).pipe(
      Effect.map(
        Option.match({
          onNone: () => null,
          onSome: (project) => ({ id: project.id, workspaceRoot: project.workspaceRoot }),
        }),
      ),
      Effect.catchCause((cause) =>
        // A projection read failure is not the same as "this is not a project",
        // but the caller cannot act on the difference. Log the real reason and
        // let the candidate loop continue.
        Effect.logWarning("graph project lookup failed", { candidate, cause }).pipe(
          Effect.as(null),
        ),
      ),
    );

  /**
   * Root of the *main* worktree, for a `cwd` inside a linked one.
   *
   * `--git-common-dir` is the shared `.git` directory; its parent is the
   * checkout T3 registered. In the main worktree it answers `.git` (relative),
   * which resolves back to the same root and is harmless.
   */
  const mainWorktreeRoot = (cwd: string) =>
    gitLine("graph.resolveCommonDir", cwd, ["rev-parse", "--git-common-dir"]).pipe(
      Effect.map((commonDir) => {
        if (commonDir === null) return null;
        const absolute = path.isAbsolute(commonDir) ? commonDir : path.join(cwd, commonDir);
        // A bare repository has no working tree, so there is nothing to scan.
        return path.basename(absolute) === ".git" ? path.dirname(absolute) : null;
      }),
    );

  const candidateRoots = (cwd: string, real: string): ReadonlyArray<string> => {
    const candidates = [cwd];
    let current = real;
    for (let depth = 0; depth < MAX_ANCESTOR_LOOKUPS; depth += 1) {
      if (!candidates.includes(current)) candidates.push(current);
      const parent = path.dirname(current);
      if (parent === current) break;
      current = parent;
    }
    return candidates;
  };

  const resolve: GraphWorkspaceResolver["Service"]["resolve"] = Effect.fn(
    "GraphWorkspaceResolver.resolve",
  )(function* (cwd) {
    const real = yield* fs.realPath(cwd).pipe(Effect.catchCause(() => Effect.succeed(cwd)));

    let matched: { readonly id: ProjectId; readonly workspaceRoot: string } | null = null;
    for (const candidate of candidateRoots(cwd, real)) {
      matched = yield* projectAt(candidate);
      if (matched !== null) break;
    }

    if (matched === null) {
      const worktreeRoot = yield* mainWorktreeRoot(cwd);
      if (worktreeRoot !== null) matched = yield* projectAt(worktreeRoot);
    }

    if (matched === null) return yield* new GraphWorkspaceUnknownError({ cwd });

    // Branch and HEAD come from the *thread's* directory, not the project root:
    // in a worktree those are exactly what differ, and they are half the key.
    const status = yield* git
      .statusDetailsLocal(cwd)
      .pipe(Effect.catchCause(() => Effect.succeed({ branch: null as string | null })));
    const headSha = yield* gitLine("graph.resolveHead", cwd, ["rev-parse", "--verify", "HEAD"]);
    const topLevel = yield* gitLine("graph.resolveTopLevel", cwd, ["rev-parse", "--show-toplevel"]);

    const location = yield* store.locate({
      projectId: matched.id,
      branch: status.branch,
      headSha,
    });

    return {
      projectId: matched.id,
      workspaceRoot: topLevel ?? matched.workspaceRoot,
      branch: status.branch,
      headSha,
      location,
    };
  });

  return GraphWorkspaceResolver.of({ resolve });
});

export const layer = Layer.effect(GraphWorkspaceResolver, make);
