import { useAtomValue } from "@effect/atom-react";
import type { EnvironmentId, ProjectListEntriesResult } from "@t3tools/contracts";
import * as Option from "effect/Option";
import { AsyncResult, Atom } from "effect/unstable/reactivity";

import { getProjectEntriesQueryAtom } from "./components/files/projectFilesQueryState";
import { workspaceRelativePath } from "./markdown-links";
import { resolvePathLinkTarget, splitPathAndPosition } from "./terminal-links";

const WINDOWS_DRIVE_PATH_PATTERN = /^[A-Za-z]:\//;

/**
 * Built once per entries result and shared by every consumer — the same list
 * already backs the file browser, so membership checks cost no extra request.
 */
const filePathSetByEntriesResult = new WeakMap<ProjectListEntriesResult, ReadonlySet<string>>();

/** Workspace-relative paths of every file (not directory) the workspace reported. */
export function workspaceFilePathSet(result: ProjectListEntriesResult): ReadonlySet<string> {
  const cached = filePathSetByEntriesResult.get(result);
  if (cached) return cached;

  const filePaths = new Set<string>();
  for (const entry of result.entries) {
    if (entry.kind !== "file") continue;
    filePaths.add(entry.path);
  }
  filePathSetByEntriesResult.set(result, filePaths);
  return filePaths;
}

const basenameIndexByEntriesResult = new WeakMap<
  ProjectListEntriesResult,
  ReadonlyMap<string, string | null>
>();

/**
 * Basename → the single workspace-relative path bearing it, or null once a
 * second file shares the name. The null sentinel is what keeps a mention of
 * `index.ts` from linking to an arbitrary one of many.
 */
export function workspaceBasenameIndex(
  result: ProjectListEntriesResult,
): ReadonlyMap<string, string | null> {
  const cached = basenameIndexByEntriesResult.get(result);
  if (cached) return cached;

  const pathsByBasename = new Map<string, string | null>();
  for (const entry of result.entries) {
    if (entry.kind !== "file") continue;
    const separatorIndex = entry.path.lastIndexOf("/");
    const basename = separatorIndex >= 0 ? entry.path.slice(separatorIndex + 1) : entry.path;
    pathsByBasename.set(basename, pathsByBasename.has(basename) ? null : entry.path);
  }
  basenameIndexByEntriesResult.set(result, pathsByBasename);
  return pathsByBasename;
}

function workspaceRelativeCandidate(path: string, cwd: string): string | null {
  const normalized = path.replaceAll("\\", "/");
  if (normalized.startsWith("./")) return normalized.slice(2);
  if (
    normalized.startsWith("/") ||
    normalized.startsWith("~/") ||
    WINDOWS_DRIVE_PATH_PATTERN.test(normalized)
  ) {
    return workspaceRelativePath(resolvePathLinkTarget(normalized, cwd), cwd);
  }
  // `../` escapes the workspace, so the entry list cannot confirm it.
  if (normalized.startsWith("../")) return null;
  return normalized;
}

/**
 * Resolves a path-shaped token to the absolute file it names, but only when
 * that file is actually present in the workspace. Returns null for anything
 * unverifiable — outside the workspace, a directory, or simply not there —
 * so callers can leave the token as plain text instead of offering a link
 * that would fail on click.
 *
 * A truncated entry list yields false negatives rather than false positives,
 * which is the safer direction for a link the user is invited to trust.
 */
export function resolveWorkspaceFilePath(
  candidate: string,
  workspaceFiles: ReadonlySet<string>,
  cwd: string,
  basenames?: ReadonlyMap<string, string | null>,
): string | null {
  const { path, line, column, endLine } = splitPathAndPosition(candidate);
  if (path.length === 0) return null;

  const relativePath = workspaceRelativeCandidate(path, cwd);
  if (relativePath === null || relativePath.length === 0) return null;

  // The exact check runs first so a root-level `package.json` keeps meaning
  // the root file even when nested ones make the basename ambiguous.
  let resolvedRelativePath: string | null = workspaceFiles.has(relativePath) ? relativePath : null;
  if (resolvedRelativePath === null && basenames && !/[\\/]/.test(path)) {
    resolvedRelativePath = basenames.get(path) ?? null;
  }
  if (resolvedRelativePath === null) return null;

  const position = line
    ? endLine
      ? `:${line}-${endLine}`
      : `:${line}${column ? `:${column}` : ""}`
    : "";
  return `${resolvePathLinkTarget(resolvedRelativePath, cwd)}${position}`;
}

const EMPTY_PROJECT_ENTRIES_ATOM = Atom.make(
  AsyncResult.initial<ProjectListEntriesResult, never>(false),
).pipe(Atom.withLabel("workspace-file-path-index:empty"));

/**
 * The workspace's file paths, or null while unknown (no environment, no cwd,
 * or the listing has not arrived yet).
 */
export function useWorkspaceFilePathSet(
  environmentId: EnvironmentId | null,
  cwd: string | undefined,
): ReadonlySet<string> | null {
  const result = useAtomValue(
    environmentId && cwd
      ? getProjectEntriesQueryAtom(environmentId, cwd)
      : EMPTY_PROJECT_ENTRIES_ATOM,
  );
  const entries = Option.getOrNull(AsyncResult.value(result));
  return entries ? workspaceFilePathSet(entries) : null;
}

export interface WorkspaceFilePathIndex {
  readonly files: ReadonlySet<string>;
  readonly basenames: ReadonlyMap<string, string | null>;
}

/**
 * The workspace's file paths plus a unique-basename lookup, or null while
 * unknown. Both structures are cached per entries result, so the returned
 * object is referentially stable across renders for the same listing.
 */
export function useWorkspaceFilePathIndex(
  environmentId: EnvironmentId | null,
  cwd: string | undefined,
): WorkspaceFilePathIndex | null {
  const result = useAtomValue(
    environmentId && cwd
      ? getProjectEntriesQueryAtom(environmentId, cwd)
      : EMPTY_PROJECT_ENTRIES_ATOM,
  );
  const entries = Option.getOrNull(AsyncResult.value(result));
  return entries ? workspaceFilePathIndexOf(entries) : null;
}

const indexByEntriesResult = new WeakMap<ProjectListEntriesResult, WorkspaceFilePathIndex>();

function workspaceFilePathIndexOf(result: ProjectListEntriesResult): WorkspaceFilePathIndex {
  const cached = indexByEntriesResult.get(result);
  if (cached) return cached;
  const index: WorkspaceFilePathIndex = {
    files: workspaceFilePathSet(result),
    basenames: workspaceBasenameIndex(result),
  };
  indexByEntriesResult.set(result, index);
  return index;
}
