import { scopedThreadKey } from "@t3tools/client-runtime/environment";
import type { ScopedThreadRef } from "@t3tools/contracts";
import { create } from "zustand";

interface GitRootStoreState {
  /** Absolute path of the selected git root per thread; absent/null = primary. */
  selectedRootPathByThreadKey: Record<string, string | null>;
  selectGitRoot: (ref: ScopedThreadRef, rootPath: string | null) => void;
  removeThread: (ref: ScopedThreadRef) => void;
}

/**
 * Which workspace root the git surfaces (diff panel, git actions control)
 * operate on for a multi-root thread. Session-scoped: a stale selection after
 * a root is detached simply falls back to the primary via
 * `selectSelectedGitRootPath`.
 */
export const useGitRootStore = create<GitRootStoreState>()((set) => ({
  selectedRootPathByThreadKey: {},
  selectGitRoot: (ref, rootPath) =>
    set((state) => ({
      selectedRootPathByThreadKey: {
        ...state.selectedRootPathByThreadKey,
        [scopedThreadKey(ref)]: rootPath,
      },
    })),
  removeThread: (ref) =>
    set((state) => {
      const threadKey = scopedThreadKey(ref);
      if (!(threadKey in state.selectedRootPathByThreadKey)) return state;
      const { [threadKey]: _removed, ...selectedRootPathByThreadKey } =
        state.selectedRootPathByThreadKey;
      return { selectedRootPathByThreadKey };
    }),
}));

/** Validated selection: only a currently-attached non-primary root sticks. */
export function selectSelectedGitRootPath(
  selectedRootPathByThreadKey: Record<string, string | null>,
  ref: ScopedThreadRef | null | undefined,
  roots: ReadonlyArray<{ readonly path: string; readonly isPrimary: boolean }>,
): string | null {
  if (!ref) return null;
  const selected = selectedRootPathByThreadKey[scopedThreadKey(ref)] ?? null;
  if (selected === null) return null;
  return roots.some((root) => !root.isPrimary && root.path === selected) ? selected : null;
}
