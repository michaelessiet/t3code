import { useAtomRefresh, useAtomValue } from "@effect/atom-react";
import type {
  EnvironmentId,
  ProjectListEntriesResult,
  ProjectReadFileResult,
} from "@t3tools/contracts";
import * as Cause from "effect/Cause";
import * as Option from "effect/Option";
import { AsyncResult } from "effect/unstable/reactivity";
import { useCallback, useEffect } from "react";

import { appAtomRegistry } from "~/rpc/atomRegistry";
import { projectEnvironment } from "~/state/projects";
import { executeAtomQuery } from "@t3tools/client-runtime/state/runtime";

const EMPTY_PROJECT_FILE_PATH = "";
function optimisticFileAtom(environmentId: EnvironmentId, cwd: string, relativePath: string) {
  return projectEnvironment.optimisticFile({ environmentId, cwd, relativePath });
}

interface ProjectQueryState<A> {
  readonly data: A | null;
  readonly error: string | null;
  readonly isPending: boolean;
  readonly refresh: () => void;
}

export function getProjectEntriesQueryAtom(environmentId: EnvironmentId, cwd: string) {
  return projectEnvironment.listEntries({ environmentId, input: { cwd } });
}

export function getProjectFileQueryAtom(
  environmentId: EnvironmentId,
  cwd: string,
  relativePath: string | null,
) {
  return projectEnvironment.readFile({
    environmentId,
    input: { cwd, relativePath: relativePath ?? EMPTY_PROJECT_FILE_PATH },
  });
}

export function setProjectFileQueryData(
  environmentId: EnvironmentId,
  cwd: string,
  relativePath: string,
  contents: string,
): void {
  appAtomRegistry.set(optimisticFileAtom(environmentId, cwd, relativePath), {
    confirmedAgainst: undefined,
    data: {
      relativePath,
      contents,
      byteLength: new TextEncoder().encode(contents).byteLength,
      truncated: false,
    },
  });
}

export function getOptimisticProjectFileQueryData(
  environmentId: EnvironmentId,
  cwd: string,
  relativePath: string,
): ProjectReadFileResult | null {
  return appAtomRegistry.get(optimisticFileAtom(environmentId, cwd, relativePath))?.data ?? null;
}

export function confirmProjectFileQueryData(
  environmentId: EnvironmentId,
  cwd: string,
  relativePath: string,
  contents: string,
): boolean {
  const atom = optimisticFileAtom(environmentId, cwd, relativePath);
  const optimisticFile = appAtomRegistry.get(atom);
  if (optimisticFile?.data.contents !== contents) return false;

  const queryAtom = getProjectFileQueryAtom(environmentId, cwd, relativePath);
  const confirmed = {
    ...optimisticFile,
    confirmedAgainst: appAtomRegistry.get(queryAtom),
  };
  appAtomRegistry.set(atom, confirmed);
  appAtomRegistry.refresh(queryAtom);
  void executeAtomQuery(appAtomRegistry, queryAtom, {
    reportDefect: false,
    reportFailure: false,
  }).then((result) => {
    if (result._tag === "Success" && appAtomRegistry.get(atom) === confirmed) {
      appAtomRegistry.set(atom, null);
    }
  });
  return true;
}

export function resolveProjectFileQueryData(
  environmentId: EnvironmentId,
  cwd: string,
  relativePath: string | null,
  data: ProjectReadFileResult | null,
): ProjectReadFileResult | null {
  if (relativePath === null) return data;
  return appAtomRegistry.get(optimisticFileAtom(environmentId, cwd, relativePath))?.data ?? data;
}

export function clearProjectFileQueryData(
  environmentId: EnvironmentId,
  cwd: string,
  relativePath: string,
): void {
  appAtomRegistry.set(optimisticFileAtom(environmentId, cwd, relativePath), null);
}

function errorMessage<A>(result: AsyncResult.AsyncResult<A, unknown>): string | null {
  if (result._tag !== "Failure") return null;
  const cause = Cause.squash(result.cause);
  return cause instanceof Error ? cause.message : "Workspace query failed.";
}

export function useProjectEntriesQuery(
  environmentId: EnvironmentId,
  cwd: string,
): ProjectQueryState<ProjectListEntriesResult> {
  const atom = getProjectEntriesQueryAtom(environmentId, cwd);
  const result = useAtomValue(atom);
  const refreshAtom = useAtomRefresh(atom);
  const refresh = useCallback(() => refreshAtom(), [refreshAtom]);
  return {
    data: Option.getOrNull(AsyncResult.value(result)),
    error: errorMessage(result),
    isPending: result.waiting,
    refresh,
  };
}

/**
 * Revision of the file as last read from disk (never the optimistic local
 * buffer, which carries no revision). `undefined` while loading or when the
 * server predates revision support.
 */
export function useProjectFileDiskRevision(
  environmentId: EnvironmentId,
  cwd: string,
  relativePath: string | null,
): string | undefined {
  const atom = getProjectFileQueryAtom(environmentId, cwd, relativePath);
  const result = useAtomValue(atom);
  if (relativePath === null) return undefined;
  return Option.getOrNull(AsyncResult.value(result))?.revision;
}

/**
 * Re-read the open file whenever the workspace watcher reports it may have
 * changed on disk. The refreshed query result (contents + revision) drives
 * buffer reconciliation: clean buffers re-render with the new contents, dirty
 * buffers surface a conflict.
 */
export function useWorkspaceFileWatch(
  environmentId: EnvironmentId,
  cwd: string,
  relativePath: string | null,
  refresh: () => void,
): void {
  const watchResult = useAtomValue(
    projectEnvironment.watchChanges({ environmentId, input: { cwd } }),
  );
  const event = Option.getOrNull(AsyncResult.value(watchResult));
  useEffect(() => {
    if (event === null || relativePath === null) return;
    if (event._tag === "overflow" || event.paths.includes(relativePath)) refresh();
  }, [event, relativePath, refresh]);
}

export function useProjectFileQuery(
  environmentId: EnvironmentId,
  cwd: string,
  relativePath: string | null,
): ProjectQueryState<ProjectReadFileResult> {
  const atom = getProjectFileQueryAtom(environmentId, cwd, relativePath);
  const result = useAtomValue(atom);
  const refreshAtom = useAtomRefresh(atom);
  const refresh = useCallback(() => refreshAtom(), [refreshAtom]);
  const data = Option.getOrNull(AsyncResult.value(result));
  const optimisticResult = useAtomValue(
    optimisticFileAtom(environmentId, cwd, relativePath ?? EMPTY_PROJECT_FILE_PATH),
  );
  const optimisticFile = relativePath === null ? null : optimisticResult;

  return {
    data: optimisticFile?.data ?? data,
    error: errorMessage(result),
    isPending: result.waiting,
    refresh,
  };
}
