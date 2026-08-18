import type { GitStatus, GitStatusEntry } from "@pierre/trees";
import type { VcsFileStatusEntry } from "@t3tools/contracts";

const TREE_STATUS_BY_VCS_STATUS: Readonly<Record<VcsFileStatusEntry["status"], GitStatus>> = {
  added: "added",
  // The tree has no conflicted status; conflicts read as modified and carry a
  // "!" in the decoration lane instead.
  conflicted: "modified",
  deleted: "deleted",
  modified: "modified",
  renamed: "renamed",
  untracked: "untracked",
};

/**
 * A folder takes the most attention-worthy status among its descendants, so a
 * directory holding one edit plus ten new files still reads as modified.
 */
const FOLDER_STATUS_PRIORITY: readonly GitStatus[] = [
  "modified",
  "renamed",
  "deleted",
  "added",
  "untracked",
  "ignored",
];

function folderStatusRank(status: GitStatus): number {
  const rank = FOLDER_STATUS_PRIORITY.indexOf(status);
  return rank === -1 ? FOLDER_STATUS_PRIORITY.length : rank;
}

function withoutTrailingSlash(path: string): string {
  return path.endsWith("/") ? path.slice(0, -1) : path;
}

/** Every directory above `path`, shallowest first, without trailing slashes. */
function ancestorDirectories(path: string): readonly string[] {
  const segments = withoutTrailingSlash(path).split("/");
  const ancestors: Array<string> = [];
  for (let index = 1; index < segments.length; index += 1) {
    ancestors.push(segments.slice(0, index).join("/"));
  }
  return ancestors;
}

export interface FileTreeGitDecorations {
  /** Ready to hand to the tree model; directory paths carry a trailing slash. */
  readonly entries: readonly GitStatusEntry[];
  /** File paths git reports as unmerged, keyed without a trailing slash. */
  readonly conflictedPaths: ReadonlySet<string>;
}

export const EMPTY_FILE_TREE_GIT_DECORATIONS: FileTreeGitDecorations = {
  entries: [],
  conflictedPaths: new Set<string>(),
};

/**
 * Project `vcs.getFileStatuses` output onto file-tree decorations: per-file
 * statuses plus an aggregated status for every ancestor directory, so a change
 * buried deep in the tree stays visible from the collapsed root.
 *
 * `treePaths` are the explorer's own rows (directories slash-terminated); they
 * are only needed to expand the single record git emits for a wholly untracked
 * directory over the files inside it.
 */
export function buildFileTreeGitDecorations(
  statuses: readonly VcsFileStatusEntry[],
  treePaths: readonly string[],
): FileTreeGitDecorations {
  if (statuses.length === 0) {
    return EMPTY_FILE_TREE_GIT_DECORATIONS;
  }

  const fileStatuses = new Map<string, GitStatus>();
  const folderStatuses = new Map<string, GitStatus>();
  const conflictedPaths = new Set<string>();
  const untrackedDirectories: Array<string> = [];

  const raiseFolder = (path: string, status: GitStatus): void => {
    const current = folderStatuses.get(path);
    if (current === undefined || folderStatusRank(status) < folderStatusRank(current)) {
      folderStatuses.set(path, status);
    }
  };
  const assignPath = (path: string, status: GitStatus, isDirectory: boolean): void => {
    const normalized = withoutTrailingSlash(path);
    if (normalized.length === 0) return;
    if (isDirectory) {
      raiseFolder(normalized, status);
    } else {
      fileStatuses.set(normalized, status);
    }
    for (const ancestor of ancestorDirectories(normalized)) {
      raiseFolder(ancestor, status);
    }
  };

  for (const entry of statuses) {
    const status = TREE_STATUS_BY_VCS_STATUS[entry.status];
    const isDirectory = entry.path.endsWith("/");
    if (isDirectory) {
      // git collapses a wholly untracked directory into one record.
      untrackedDirectories.push(entry.path);
    } else if (entry.status === "conflicted") {
      conflictedPaths.add(entry.path);
    }
    assignPath(entry.path, status, isDirectory);
  }

  if (untrackedDirectories.length > 0) {
    for (const treePath of treePaths) {
      const isDirectory = treePath.endsWith("/");
      const normalized = withoutTrailingSlash(treePath);
      if (normalized.length === 0) continue;
      if (isDirectory ? folderStatuses.has(normalized) : fileStatuses.has(normalized)) {
        continue;
      }
      if (untrackedDirectories.some((directory) => treePath.startsWith(directory))) {
        assignPath(treePath, "untracked", isDirectory);
      }
    }
  }

  const entries: Array<GitStatusEntry> = [];
  for (const [path, status] of fileStatuses) {
    entries.push({ path, status });
  }
  for (const [path, status] of folderStatuses) {
    entries.push({ path: `${path}/`, status });
  }
  return { entries, conflictedPaths };
}
