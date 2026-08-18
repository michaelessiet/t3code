import { useAtomRefresh, useAtomValue } from "@effect/atom-react";
import type { EnvironmentId, VcsFileStatusEntry } from "@t3tools/contracts";
import * as Option from "effect/Option";
import { AsyncResult } from "effect/unstable/reactivity";
import { useCallback, useEffect } from "react";

import { projectEnvironment } from "~/state/projects";
import { vcsEnvironment } from "~/state/vcs";

const EMPTY_STATUSES: readonly VcsFileStatusEntry[] = [];

/**
 * Per-path git status for one workspace root, feeding the file explorer's
 * change decorations. Refreshed on every workspace watcher event (a write,
 * create or delete is exactly what changes a file's status) and on every VCS
 * status emission, which covers commits, branch switches and staging done
 * through the app; external staging is caught by the status poller.
 *
 * Returns an empty list while loading, on error and outside a repository — the
 * tree then simply renders undecorated.
 */
export function useFileTreeGitStatuses(
  environmentId: EnvironmentId,
  cwd: string,
): readonly VcsFileStatusEntry[] {
  const atom = vcsEnvironment.fileStatuses({ environmentId, input: { cwd } });
  const result = useAtomValue(atom);
  const refreshAtom = useAtomRefresh(atom);
  const refresh = useCallback(() => refreshAtom(), [refreshAtom]);

  const watchResult = useAtomValue(
    projectEnvironment.watchChanges({ environmentId, input: { cwd } }),
  );
  const event = Option.getOrNull(AsyncResult.value(watchResult));
  useEffect(() => {
    if (event === null) return;
    refresh();
  }, [event, refresh]);

  const statusResult = useAtomValue(vcsEnvironment.status({ environmentId, input: { cwd } }));
  const status = Option.getOrNull(AsyncResult.value(statusResult));
  useEffect(() => {
    if (status === null) return;
    refresh();
  }, [status, refresh]);

  return Option.getOrNull(AsyncResult.value(result))?.entries ?? EMPTY_STATUSES;
}
