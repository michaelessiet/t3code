import * as Encoding from "effect/Encoding";
import {
  CheckpointRef,
  ProjectId,
  type ThreadId,
  type ResolvedWorkspaceRoot,
} from "@t3tools/contracts";
import { normalizeProjectPathForComparison } from "@t3tools/shared/path";

export const CHECKPOINT_REFS_PREFIX = "refs/t3/checkpoints";

export function checkpointRefForThreadTurn(threadId: ThreadId, turnCount: number): CheckpointRef {
  return CheckpointRef.make(
    `${CHECKPOINT_REFS_PREFIX}/${Encoding.encodeBase64Url(threadId)}/turn/${turnCount}`,
  );
}

export function resolveThreadWorkspaceCwd(input: {
  readonly thread: {
    readonly projectId: ProjectId;
    readonly worktreePath: string | null;
  };
  readonly projects: ReadonlyArray<{
    readonly id: ProjectId;
    readonly workspaceRoot: string;
  }>;
}): string | undefined {
  const worktreeCwd = input.thread.worktreePath ?? undefined;
  if (worktreeCwd) {
    return worktreeCwd;
  }

  return input.projects.find((project) => project.id === input.thread.projectId)?.workspaceRoot;
}

/**
 * Effective workspace roots for a thread: the primary cwd (worktree path or
 * the owning project's workspace root) plus additional roots attached at the
 * project and thread level, in that order. Callers supply the read-time
 * `resolvedAdditionalRoots` from the projection shells; project refs were
 * already dereferenced there, so dangling refs arrive as `missing-project`
 * and are dropped. `additional` is deduped against the primary and against
 * itself using comparison-normalized paths.
 */
export function resolveThreadWorkspaceRoots(input: {
  readonly thread: {
    readonly projectId: ProjectId;
    readonly worktreePath: string | null;
    readonly resolvedAdditionalRoots?: ReadonlyArray<ResolvedWorkspaceRoot> | undefined;
  };
  readonly projects: ReadonlyArray<{
    readonly id: ProjectId;
    readonly workspaceRoot: string;
    readonly resolvedAdditionalRoots?: ReadonlyArray<ResolvedWorkspaceRoot> | undefined;
  }>;
}): { readonly primary: string | undefined; readonly additional: ReadonlyArray<string> } {
  const primary = resolveThreadWorkspaceCwd(input);
  const ownerProject = input.projects.find((project) => project.id === input.thread.projectId);

  const resolvedRoots = [
    ...(ownerProject?.resolvedAdditionalRoots ?? []),
    ...(input.thread.resolvedAdditionalRoots ?? []),
  ];

  const seen = new Set<string>(
    primary === undefined ? [] : [normalizeProjectPathForComparison(primary)],
  );
  const additional: string[] = [];
  for (const root of resolvedRoots) {
    if (root.status !== "ok" || root.path === undefined) {
      continue;
    }
    const comparable = normalizeProjectPathForComparison(root.path);
    if (seen.has(comparable)) {
      continue;
    }
    seen.add(comparable);
    additional.push(root.path);
  }

  return { primary, additional };
}
