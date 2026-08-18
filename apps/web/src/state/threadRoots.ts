import type {
  RepositoryIdentity,
  ResolvedWorkspaceRoot,
  ScopedProjectRef,
  ScopedThreadRef,
} from "@t3tools/contracts";
import { scopeProjectRef } from "@t3tools/client-runtime/environment";
import { useMemo } from "react";

import { useComposerDraftStore, type DraftId } from "../composerDraftStore";
import { readProject, readThreadShell, useProject, useThread } from "./entities";

/**
 * One effective workspace root of a thread. `primary` is the thread's cwd
 * (worktree path or the owning project's workspace root); additional roots
 * come from the project's or the thread's `additionalRoots` attachments.
 *
 * v1 caveats surfaced in the UI: additional roots are used in place (no
 * worktrees) and are not checkpointed.
 */
export interface ThreadRoot {
  readonly path: string;
  readonly label: string;
  readonly isPrimary: boolean;
  readonly source: "primary" | "project" | "thread";
  readonly repositoryIdentity?: RepositoryIdentity | null | undefined;
}

export interface ThreadRoots {
  /** Null only when neither a server thread nor a draft project is known. */
  readonly primary: ThreadRoot | null;
  readonly additional: ReadonlyArray<ThreadRoot>;
  /** Primary first, then additional — the order every fan-out should use. */
  readonly all: ReadonlyArray<ThreadRoot>;
}

const EMPTY_THREAD_ROOTS: ThreadRoots = Object.freeze({
  primary: null,
  additional: Object.freeze([]),
  all: Object.freeze([]),
});

function normalizePathSeparators(path: string): string {
  return path.replaceAll("\\", "/");
}

function trimTrailingSeparators(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  return trimmed.length === 0 ? path : trimmed;
}

/** Comparison key for root paths. Case-insensitive to match workspace-path
    comparison elsewhere (Darwin/Windows default filesystems). */
export function normalizeRootPathForComparison(path: string): string {
  return trimTrailingSeparators(normalizePathSeparators(path.trim())).toLowerCase();
}

function basenameOfPath(path: string): string {
  const normalized = trimTrailingSeparators(normalizePathSeparators(path));
  const separatorIndex = normalized.lastIndexOf("/");
  return separatorIndex >= 0 ? normalized.slice(separatorIndex + 1) : normalized;
}

function parentQualifiedLabel(path: string): string {
  const normalized = trimTrailingSeparators(normalizePathSeparators(path));
  const segments = normalized.split("/").filter((segment) => segment.length > 0);
  if (segments.length >= 2) {
    return `${segments[segments.length - 2]}/${segments[segments.length - 1]}`;
  }
  return segments[segments.length - 1] ?? normalized;
}

/**
 * Deterministic display labels: the basename, disambiguated with the parent
 * directory when two roots share one.
 */
export function rootLabels(paths: ReadonlyArray<string>): ReadonlyArray<string> {
  const basenames = paths.map(basenameOfPath);
  const counts = new Map<string, number>();
  for (const name of basenames) {
    const key = name.toLowerCase();
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return basenames.map((name, index) => {
    const path = paths[index] ?? name;
    return (counts.get(name.toLowerCase()) ?? 0) > 1 ? parentQualifiedLabel(path) : name;
  });
}

export interface ComposeThreadRootsInput {
  readonly primaryPath: string | null;
  readonly projectRoots?: ReadonlyArray<ResolvedWorkspaceRoot> | undefined;
  readonly threadRoots?: ReadonlyArray<ResolvedWorkspaceRoot> | undefined;
}

/**
 * Compose the effective root list: primary first, then project-sourced, then
 * thread-sourced attachments, deduped on comparison-normalized paths.
 * Unresolvable attachments (dangling project refs) are dropped here; the
 * roots-management UI reads the raw refs instead so it can surface them.
 */
export function composeThreadRoots(input: ComposeThreadRootsInput): ThreadRoots {
  if (input.primaryPath === null || input.primaryPath.trim().length === 0) {
    return EMPTY_THREAD_ROOTS;
  }

  const entries: Array<Omit<ThreadRoot, "label">> = [
    { path: input.primaryPath, isPrimary: true, source: "primary" },
  ];
  const seen = new Set<string>([normalizeRootPathForComparison(input.primaryPath)]);
  const appendResolved = (
    roots: ReadonlyArray<ResolvedWorkspaceRoot> | undefined,
    source: "project" | "thread",
  ) => {
    for (const root of roots ?? []) {
      if (root.status !== "ok" || root.path === undefined) continue;
      const key = normalizeRootPathForComparison(root.path);
      if (seen.has(key)) continue;
      seen.add(key);
      entries.push({
        path: root.path,
        isPrimary: false,
        source,
        repositoryIdentity: root.repositoryIdentity,
      });
    }
  };
  appendResolved(input.projectRoots, "project");
  appendResolved(input.threadRoots, "thread");

  const labels = rootLabels(entries.map((entry) => entry.path));
  const all = entries.map(
    (entry, index): ThreadRoot => ({
      ...entry,
      label: labels[index] ?? basenameOfPath(entry.path),
    }),
  );
  return {
    primary: all[0] ?? null,
    additional: all.slice(1),
    all,
  };
}

export interface RelativizedRootPath {
  readonly root: ThreadRoot;
  readonly relativePath: string;
}

/**
 * Match an absolute path to the root that contains it — primary wins ties by
 * ordering, and deeper (more specific) roots are never attached inside each
 * other by server invariant, so first prefix match is safe.
 */
export function relativizeAgainstRoots(
  path: string,
  roots: ReadonlyArray<ThreadRoot>,
): RelativizedRootPath | null {
  const normalizedPath = normalizePathSeparators(path.trim());
  const pathForCompare = normalizedPath.toLowerCase();
  for (const root of roots) {
    const rootForCompare = normalizeRootPathForComparison(root.path);
    if (pathForCompare === rootForCompare) {
      return { root, relativePath: "" };
    }
    if (pathForCompare.startsWith(`${rootForCompare}/`)) {
      return { root, relativePath: normalizedPath.slice(rootForCompare.length + 1) };
    }
  }
  return null;
}

interface ThreadRootsSourceShapes {
  readonly serverThread: {
    readonly projectId: ScopedProjectRef["projectId"];
    readonly environmentId: ScopedProjectRef["environmentId"];
    readonly worktreePath: string | null;
    readonly resolvedAdditionalRoots?: ReadonlyArray<ResolvedWorkspaceRoot> | undefined;
  } | null;
  readonly draftThread: {
    readonly projectId: ScopedProjectRef["projectId"];
    readonly environmentId: ScopedProjectRef["environmentId"];
    readonly worktreePath: string | null;
  } | null;
  readonly project: {
    readonly workspaceRoot: string;
    readonly resolvedAdditionalRoots?: ReadonlyArray<ResolvedWorkspaceRoot> | undefined;
  } | null;
}

function composeFromSources(sources: ThreadRootsSourceShapes): ThreadRoots {
  const primaryPath =
    sources.serverThread?.worktreePath ??
    sources.draftThread?.worktreePath ??
    sources.project?.workspaceRoot ??
    null;
  return composeThreadRoots({
    primaryPath,
    projectRoots: sources.project?.resolvedAdditionalRoots,
    threadRoots: sources.serverThread?.resolvedAdditionalRoots,
  });
}

/**
 * The effective workspace roots of a thread (or composer draft). Falls back
 * to the single primary root when the server predates multi-root support
 * (the shell fields simply decode absent).
 */
export function useThreadRoots(threadRef: ScopedThreadRef | null, draftId?: DraftId): ThreadRoots {
  const serverThread = useThread(threadRef);
  const draftThread = useComposerDraftStore((store) =>
    draftId
      ? store.getDraftSession(draftId)
      : threadRef !== null
        ? store.getDraftThreadByRef(threadRef)
        : null,
  );
  const projectRef: ScopedProjectRef | null = serverThread
    ? scopeProjectRef(serverThread.environmentId, serverThread.projectId)
    : draftThread
      ? scopeProjectRef(draftThread.environmentId, draftThread.projectId)
      : null;
  const project = useProject(projectRef);
  return useMemo(
    () => composeFromSources({ serverThread, draftThread, project }),
    [serverThread, draftThread, project],
  );
}

/** Imperative (non-hook) counterpart of `useThreadRoots` for action code. */
export function readThreadRoots(threadRef: ScopedThreadRef): ThreadRoots {
  const shell = readThreadShell(threadRef);
  const draftThread =
    shell === null ? useComposerDraftStore.getState().getDraftThreadByRef(threadRef) : null;
  const projectRef = shell
    ? scopeProjectRef(shell.environmentId, shell.projectId)
    : draftThread
      ? scopeProjectRef(draftThread.environmentId, draftThread.projectId)
      : null;
  const project = projectRef ? readProject(projectRef) : null;
  return composeFromSources({ serverThread: shell, draftThread, project });
}
