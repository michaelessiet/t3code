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
): string | null {
  const { path, line, column } = splitPathAndPosition(candidate);
  if (path.length === 0) return null;

  const relativePath = workspaceRelativeCandidate(path, cwd);
  if (relativePath === null || relativePath.length === 0) return null;
  if (!workspaceFiles.has(relativePath)) return null;

  const position = line ? `:${line}${column ? `:${column}` : ""}` : "";
  return `${resolvePathLinkTarget(relativePath, cwd)}${position}`;
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
