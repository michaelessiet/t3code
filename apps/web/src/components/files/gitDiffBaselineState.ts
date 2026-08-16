import { useAtomRefresh, useAtomValue } from "@effect/atom-react";
import type { EnvironmentId, VcsFileBaselineResult } from "@t3tools/contracts";
import * as Option from "effect/Option";
import { AsyncResult, Atom } from "effect/unstable/reactivity";
import { useCallback, useEffect } from "react";

import { useWorkspaceFileWatch } from "~/components/files/projectFilesQueryState";
import { vcsEnvironment } from "~/state/vcs";

const EMPTY_BASELINE_ATOM = Atom.make(
  AsyncResult.initial<VcsFileBaselineResult, never>(false),
).pipe(Atom.withLabel("git-diff-baseline:empty"));

/**
 * HEAD + index baseline blobs for the file open in the editor, feeding the
 * git diff gutter. Refreshes when the workspace watcher reports the file may
 * have changed and on every VCS status emission (commit, branch switch,
 * discard, stage/unstage through the app — external staging is caught by the
 * status poller). The status stream carries no HEAD sha, so refreshes are
 * unconditional; the atom's stale time plus the gutter's oid no-op guard keep
 * that cheap.
 *
 * Returns null while loading or on error — the gutter simply stays empty.
 */
export function useGitDiffBaseline(
  environmentId: EnvironmentId,
  cwd: string,
  relativePath: string | null,
): VcsFileBaselineResult | null {
  const atom =
    relativePath === null
      ? EMPTY_BASELINE_ATOM
      : vcsEnvironment.fileBaseline({ environmentId, input: { cwd, relativePath } });
  const result = useAtomValue(atom);
  const refreshAtom = useAtomRefresh(atom);
  const refresh = useCallback(() => {
    if (relativePath !== null) refreshAtom();
  }, [refreshAtom, relativePath]);

  useWorkspaceFileWatch(environmentId, cwd, relativePath, refresh);

  const statusResult = useAtomValue(vcsEnvironment.status({ environmentId, input: { cwd } }));
  const status = Option.getOrNull(AsyncResult.value(statusResult));
  useEffect(() => {
    if (status === null) return;
    refresh();
  }, [status, refresh]);

  return Option.getOrNull(AsyncResult.value(result));
}
