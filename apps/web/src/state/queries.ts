import { useAtomValue } from "@effect/atom-react";
import {
  type CheckpointDiffTarget,
  type ComposerPathSearchTarget,
} from "@t3tools/client-runtime/state/threads";
import { type VcsRefTarget } from "@t3tools/client-runtime/state/vcs";
import type {
  EnvironmentId,
  OrchestrationThread,
  ProjectEntry,
  ThreadId,
  VcsListRefsResult,
  VcsRef,
} from "@t3tools/contracts";
import * as Cause from "effect/Cause";
import * as Option from "effect/Option";
import { AsyncResult, Atom } from "effect/unstable/reactivity";
import { useCallback, useMemo, useState } from "react";

import { useDebouncedValue } from "../hooks/useDebouncedValue";
import { appAtomRegistry } from "../rpc/atomRegistry";
import { orchestrationEnvironment } from "./orchestration";
import { projectEnvironment } from "./projects";
import { useEnvironmentQuery } from "./query";
import type { ThreadRoot } from "./threadRoots";
import { useEnvironmentThread } from "./threads";
import { vcsEnvironment } from "./vcs";

const COMPOSER_PATH_SEARCH_DEBOUNCE_MS = 120;
const COMPOSER_PATH_SEARCH_LIMIT = 80;
const MULTI_ROOT_COMPOSER_PATH_SEARCH_PER_ROOT_LIMIT = 30;
const VCS_REF_LIST_LIMIT = 100;
const EMPTY_REFS: ReadonlyArray<VcsRef> = [];
const INITIAL_BRANCH_CURSORS = [undefined] as const;

export interface ThreadDetailView {
  readonly data: OrchestrationThread | null;
  readonly error: string | null;
  readonly isPending: boolean;
  readonly isDeleted: boolean;
}

export function useThreadDetail(
  environmentId: EnvironmentId | null,
  threadId: ThreadId | null,
): ThreadDetailView {
  const state = useEnvironmentThread(environmentId, threadId);
  return {
    data: Option.getOrNull(state.data),
    error: Option.getOrNull(state.error),
    isPending: state.status === "synchronizing",
    isDeleted: state.status === "deleted",
  };
}

export function useBranches(target: VcsRefTarget) {
  const query = target.query?.trim() ?? "";
  return useEnvironmentQuery(
    target.environmentId !== null && target.cwd !== null
      ? vcsEnvironment.listRefs({
          environmentId: target.environmentId,
          input: {
            cwd: target.cwd,
            ...(query.length > 0 ? { query } : {}),
            limit: VCS_REF_LIST_LIMIT,
          },
        })
      : null,
  );
}

export function usePaginatedBranches(target: VcsRefTarget) {
  const query = target.query?.trim() ?? "";
  const targetKey =
    target.environmentId !== null && target.cwd !== null
      ? JSON.stringify([target.environmentId, target.cwd, query])
      : null;
  const [pagination, setPagination] = useState<{
    readonly targetKey: string | null;
    readonly cursors: ReadonlyArray<number | undefined>;
  }>({
    targetKey,
    cursors: INITIAL_BRANCH_CURSORS,
  });
  const cursors = pagination.targetKey === targetKey ? pagination.cursors : INITIAL_BRANCH_CURSORS;
  const pageAtoms = useMemo(
    () =>
      target.environmentId !== null && target.cwd !== null
        ? cursors.map((cursor) =>
            vcsEnvironment.listRefs({
              environmentId: target.environmentId!,
              input: {
                cwd: target.cwd!,
                ...(query.length > 0 ? { query } : {}),
                ...(cursor === undefined ? {} : { cursor }),
                limit: VCS_REF_LIST_LIMIT,
              },
            }),
          )
        : [],
    [cursors, query, target.cwd, target.environmentId],
  );
  const pagesAtom = useMemo(
    () =>
      Atom.make((get) => pageAtoms.map((atom) => get(atom))).pipe(
        Atom.withLabel(`web:vcs-ref-pages:${targetKey ?? "empty"}`),
      ),
    [pageAtoms, targetKey],
  );
  const results = useAtomValue(pagesAtom);
  const values = results.flatMap((result) => {
    const value = Option.getOrNull(AsyncResult.value(result));
    return value === null ? [] : [value];
  });
  const refs = new Map<string, VcsRef>();
  for (const value of values) {
    for (const ref of value.refs) {
      refs.set(ref.name, ref);
    }
  }
  const first = values[0] ?? null;
  const last = values.at(-1) ?? null;
  const data: VcsListRefsResult | null =
    first === null || last === null
      ? null
      : {
          refs: [...refs.values()],
          isRepo: first.isRepo,
          hasPrimaryRemote: first.hasPrimaryRemote,
          nextCursor: last.nextCursor,
          totalCount: Math.max(...values.map((value) => value.totalCount)),
        };
  const failed = results.find((result) => result._tag === "Failure");
  const error =
    failed?._tag === "Failure"
      ? (() => {
          const cause = Cause.squash(failed.cause);
          return cause instanceof Error && cause.message.trim().length > 0
            ? cause.message
            : "Failed to load refs.";
        })()
      : null;
  const refresh = useCallback(() => {
    const firstPage = pageAtoms[0];
    setPagination({ targetKey, cursors: INITIAL_BRANCH_CURSORS });
    if (firstPage !== undefined) {
      appAtomRegistry.refresh(firstPage);
    }
  }, [pageAtoms, targetKey]);
  const loadNext = useCallback(() => {
    if (targetKey === null || data?.nextCursor === null || data?.nextCursor === undefined) {
      return;
    }
    setPagination((current) => {
      const currentCursors =
        current.targetKey === targetKey ? current.cursors : INITIAL_BRANCH_CURSORS;
      return currentCursors.includes(data.nextCursor!)
        ? { targetKey, cursors: currentCursors }
        : { targetKey, cursors: [...currentCursors, data.nextCursor!] };
    });
  }, [data?.nextCursor, targetKey]);

  return {
    data,
    refs: data?.refs ?? EMPTY_REFS,
    error,
    isPending: results.some((result) => result.waiting),
    refresh,
    loadNext,
  };
}

export function useComposerPathSearch(target: ComposerPathSearchTarget) {
  const normalizedTarget = useMemo(
    () => ({
      environmentId: target.environmentId,
      cwd: target.cwd,
      query: target.query?.trim() ?? "",
    }),
    [target.cwd, target.environmentId, target.query],
  );
  const debouncedTarget = useDebouncedValue(normalizedTarget, COMPOSER_PATH_SEARCH_DEBOUNCE_MS);
  const result = useEnvironmentQuery(
    debouncedTarget.environmentId !== null &&
      debouncedTarget.cwd !== null &&
      debouncedTarget.query.length > 0
      ? projectEnvironment.searchEntries({
          environmentId: debouncedTarget.environmentId,
          input: {
            cwd: debouncedTarget.cwd,
            query: debouncedTarget.query,
            limit: COMPOSER_PATH_SEARCH_LIMIT,
          },
        })
      : null,
  );

  return {
    entries: result.data?.entries ?? [],
    error: result.error,
    isPending: normalizedTarget.query !== debouncedTarget.query || result.isPending,
    refresh: result.refresh,
  };
}

export interface MultiRootComposerPathSearchTarget {
  readonly environmentId: EnvironmentId | null;
  /** Effective thread roots, primary first (`ThreadRoots.all`). */
  readonly roots: ReadonlyArray<ThreadRoot>;
  readonly query: string | null;
}

export interface MultiRootComposerPathEntry {
  readonly entry: ProjectEntry;
  readonly root: ThreadRoot;
}

const EMPTY_THREAD_ROOTS: ReadonlyArray<ThreadRoot> = [];

/**
 * Reuse one array identity per root-path set. Callers often rebuild the roots
 * array every render (or pass `[]` literals); feeding those identities into
 * the atom-fan-out memos below would mint a fresh `Atom.make` per render and
 * loop `useAtomValue` resubscriptions into a maximum-update-depth error.
 * Labels/ordering are derived from the path set, so the key is sufficient.
 */
function useStableThreadRoots(
  roots: ReadonlyArray<ThreadRoot>,
  rootPathsKey: string,
): ReadonlyArray<ThreadRoot> {
  return useMemo(
    () => (roots.length === 0 ? EMPTY_THREAD_ROOTS : roots),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [rootPathsKey],
  );
}

/**
 * Multi-root variant of `useComposerPathSearch`: one `searchEntries` query per
 * workspace root, merged in root order (primary first). With a single root it
 * behaves like the single-cwd hook, including the request limit.
 */
export function useMultiRootComposerPathSearch(target: MultiRootComposerPathSearchTarget) {
  const query = target.query?.trim() ?? "";
  const rootPathsKey = target.roots.map((root) => root.path).join("\n");
  const normalizedTarget = useMemo(
    () => ({ environmentId: target.environmentId, rootPathsKey, query }),
    [target.environmentId, rootPathsKey, query],
  );
  const debouncedTarget = useDebouncedValue(normalizedTarget, COMPOSER_PATH_SEARCH_DEBOUNCE_MS);
  const stableRoots = useStableThreadRoots(target.roots, rootPathsKey);
  // Only pair atoms with roots when the debounced key matches the live roots;
  // during the debounce window the stale combination reports as pending.
  const activeRoots =
    debouncedTarget.environmentId !== null &&
    debouncedTarget.query.length > 0 &&
    debouncedTarget.rootPathsKey === rootPathsKey
      ? stableRoots
      : EMPTY_THREAD_ROOTS;
  const perRootLimit =
    activeRoots.length > 1
      ? MULTI_ROOT_COMPOSER_PATH_SEARCH_PER_ROOT_LIMIT
      : COMPOSER_PATH_SEARCH_LIMIT;
  const searchAtoms = useMemo(
    () =>
      activeRoots.map((root) =>
        projectEnvironment.searchEntries({
          environmentId: debouncedTarget.environmentId!,
          input: {
            cwd: root.path,
            query: debouncedTarget.query,
            limit: perRootLimit,
          },
        }),
      ),
    [activeRoots, debouncedTarget.environmentId, debouncedTarget.query, perRootLimit],
  );
  const combinedAtom = useMemo(
    () =>
      Atom.make((get) => searchAtoms.map((atom) => get(atom))).pipe(
        Atom.withLabel(
          `web:composer-path-search-multi:${debouncedTarget.environmentId ?? "none"}:${searchAtoms.length}`,
        ),
      ),
    [searchAtoms, debouncedTarget.environmentId],
  );
  const results = useAtomValue(combinedAtom);

  const entries: Array<MultiRootComposerPathEntry> = [];
  results.forEach((result, index) => {
    const root = activeRoots[index];
    if (root === undefined) return;
    const value = Option.getOrNull(AsyncResult.value(result));
    if (value === null) return;
    for (const entry of value.entries) {
      entries.push({ entry, root });
    }
  });
  const failed = results.find((result) => result._tag === "Failure");
  const error =
    failed?._tag === "Failure"
      ? (() => {
          const cause = Cause.squash(failed.cause);
          return cause instanceof Error && cause.message.trim().length > 0
            ? cause.message
            : "Failed to search files.";
        })()
      : null;
  const refresh = useCallback(() => {
    for (const atom of searchAtoms) {
      appAtomRegistry.refresh(atom);
    }
  }, [searchAtoms]);

  return {
    entries,
    error,
    isPending:
      normalizedTarget.query !== debouncedTarget.query ||
      normalizedTarget.rootPathsKey !== debouncedTarget.rootPathsKey ||
      results.some((result) => result.waiting),
    refresh,
  };
}

export interface MultiRootGitStatusSummary {
  readonly root: ThreadRoot;
  /** Null while the status subscription is still loading. */
  readonly isRepo: boolean | null;
  /** Null when loading or when the root is not a git repository. */
  readonly changedFileCount: number | null;
}

/**
 * Live working-tree summaries for every workspace root of a thread, one
 * cached `vcsEnvironment.status` subscription per root. The atom fan-out is
 * keyed on the root-path set, so unstable `roots` identities are safe.
 */
export function useMultiRootGitStatusSummaries(
  environmentId: EnvironmentId | null,
  roots: ReadonlyArray<ThreadRoot>,
): ReadonlyArray<MultiRootGitStatusSummary> {
  const stableRoots = useStableThreadRoots(roots, roots.map((root) => root.path).join("\n"));
  const statusAtoms = useMemo(
    () =>
      environmentId === null
        ? []
        : stableRoots.map((root) =>
            vcsEnvironment.status({ environmentId, input: { cwd: root.path } }),
          ),
    [environmentId, stableRoots],
  );
  const combinedAtom = useMemo(
    () =>
      Atom.make((get) => statusAtoms.map((atom) => get(atom))).pipe(
        Atom.withLabel(
          `web:multi-root-git-status:${environmentId ?? "none"}:${statusAtoms.length}`,
        ),
      ),
    [statusAtoms, environmentId],
  );
  const results = useAtomValue(combinedAtom);
  return stableRoots.map((root, index) => {
    const result = results[index];
    const value = result === undefined ? null : Option.getOrNull(AsyncResult.value(result));
    return {
      root,
      isRepo: value?.isRepo ?? null,
      changedFileCount: value === null || !value.isRepo ? null : value.workingTree.files.length,
    };
  });
}

export function useCheckpointDiff(
  target: CheckpointDiffTarget,
  options?: { readonly enabled?: boolean },
) {
  const enabled =
    options?.enabled !== false &&
    target.environmentId !== null &&
    target.threadId !== null &&
    target.fromTurnCount !== null &&
    target.toTurnCount !== null;
  const fullThreadTarget =
    enabled && target.fromTurnCount === 0
      ? {
          environmentId: target.environmentId!,
          input: {
            threadId: target.threadId!,
            toTurnCount: target.toTurnCount!,
            ignoreWhitespace: target.ignoreWhitespace,
          },
        }
      : null;
  const turnTarget =
    enabled && target.fromTurnCount !== 0
      ? {
          environmentId: target.environmentId!,
          input: {
            threadId: target.threadId!,
            fromTurnCount: target.fromTurnCount!,
            toTurnCount: target.toTurnCount!,
            ignoreWhitespace: target.ignoreWhitespace,
          },
        }
      : null;
  const fullThread = useEnvironmentQuery(
    fullThreadTarget === null ? null : orchestrationEnvironment.fullThreadDiff(fullThreadTarget),
  );
  const turn = useEnvironmentQuery(
    turnTarget === null ? null : orchestrationEnvironment.turnDiff(turnTarget),
  );
  return fullThreadTarget === null ? turn : fullThread;
}
