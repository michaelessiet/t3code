// @effect-diagnostics nodeBuiltinImport:off
/**
 * Supplement for the fff-backed workspace search index: fff's native walker
 * hard-wires `.gitignore` filtering, so gitignored-but-present files (build
 * output, `.env`, generated code) never reach the entries index and stay
 * invisible in the file explorer and quick search. This module enumerates
 * them with `git ls-files --others --ignored --exclude-standard --directory`
 * — `--directory` collapses fully-ignored directories to a single path so
 * git never descends into `node_modules` — then expands the collapsed
 * directories that are NOT known dependency/cache stores with a bounded
 * filesystem walk.
 *
 * Non-git workspaces (or a missing git binary) simply have no gitignored
 * files: the supplement resolves empty instead of failing.
 */
import * as NodeChildProcess from "node:child_process";
import type * as NodeFS from "node:fs";
import * as NodeFSP from "node:fs/promises";
import * as NodePath from "node:path";

import * as Effect from "effect/Effect";
import * as Schema from "effect/Schema";

import type { ProjectEntry } from "@t3tools/contracts";

const IGNORED_SUPPLEMENT_MAX_ENTRIES = 5000;
const GIT_LS_FILES_TIMEOUT_MS = 15_000;
const GIT_LS_FILES_MAX_OUTPUT_BYTES = 32 * 1024 * 1024;

/**
 * Ignored directories that are dependency or cache stores: the collapsed
 * directory itself is listed (so the tree shows it exists) but its contents
 * are not expanded — they are huge and never what a user is looking for.
 */
const UNEXPANDED_DIRECTORY_NAMES = new Set([
  ".git",
  "node_modules",
  ".pnpm-store",
  ".venv",
  "venv",
  "__pycache__",
  ".turbo",
  ".convex",
  ".next",
  ".nuxt",
  ".cache",
  ".gradle",
  ".expo",
  "DerivedData",
  "Pods",
  "target",
]);

class WorkspaceIgnoredEntriesFailed extends Schema.TaggedErrorClass<WorkspaceIgnoredEntriesFailed>()(
  "WorkspaceIgnoredEntriesFailed",
  {
    cwd: Schema.String,
    cause: Schema.Defect(),
  },
) {
  override get message(): string {
    return `Failed to enumerate gitignored entries for '${this.cwd}'.`;
  }
}

export interface IgnoredEntriesSupplement {
  readonly entries: ReadonlyArray<ProjectEntry>;
  readonly truncated: boolean;
}

export const emptyIgnoredEntriesSupplement: IgnoredEntriesSupplement = {
  entries: [],
  truncated: false,
};

function shouldExpandDirectory(relativePath: string): boolean {
  return !UNEXPANDED_DIRECTORY_NAMES.has(NodePath.posix.basename(relativePath));
}

function listGitIgnoredPaths(cwd: string): Promise<ReadonlyArray<string>> {
  return new Promise((resolve, reject) => {
    NodeChildProcess.execFile(
      "git",
      ["-C", cwd, "ls-files", "--others", "--ignored", "--exclude-standard", "--directory", "-z"],
      { timeout: GIT_LS_FILES_TIMEOUT_MS, maxBuffer: GIT_LS_FILES_MAX_OUTPUT_BYTES },
      (error, stdout) => {
        if (error) {
          reject(error);
          return;
        }
        resolve(stdout.split("\0").filter((path) => path.length > 0));
      },
    );
  });
}

/**
 * Bounded depth-first expansion of one collapsed ignored directory. Nested
 * dependency stores are re-collapsed (listed, not descended into), and the
 * shared budget caps total work no matter how large a build output tree is.
 */
async function expandIgnoredDirectory(
  cwd: string,
  relativePath: string,
  entries: ProjectEntry[],
  budget: { remaining: number },
): Promise<void> {
  const pendingDirectories = [relativePath];
  while (pendingDirectories.length > 0 && budget.remaining > 0) {
    const directory = pendingDirectories.shift()!;
    let dirents: NodeFS.Dirent[];
    try {
      dirents = await NodeFSP.readdir(NodePath.join(cwd, directory), { withFileTypes: true });
    } catch {
      continue;
    }
    for (const dirent of dirents) {
      if (budget.remaining <= 0) return;
      const childPath = `${directory}/${dirent.name}`;
      if (dirent.isDirectory()) {
        entries.push({ path: childPath, kind: "directory", ignored: true });
        budget.remaining -= 1;
        if (shouldExpandDirectory(childPath)) {
          pendingDirectories.push(childPath);
        }
      } else if (dirent.isFile()) {
        entries.push({ path: childPath, kind: "file", ignored: true });
        budget.remaining -= 1;
      }
    }
  }
}

async function collectIgnoredEntries(cwd: string): Promise<IgnoredEntriesSupplement> {
  const paths = await listGitIgnoredPaths(cwd);
  const entries: ProjectEntry[] = [];
  const budget = { remaining: IGNORED_SUPPLEMENT_MAX_ENTRIES };
  const expandable: string[] = [];

  for (const rawPath of paths) {
    if (budget.remaining <= 0) break;
    if (rawPath.endsWith("/")) {
      const directoryPath = rawPath.slice(0, -1);
      entries.push({ path: directoryPath, kind: "directory", ignored: true });
      budget.remaining -= 1;
      if (shouldExpandDirectory(directoryPath)) {
        expandable.push(directoryPath);
      }
    } else {
      entries.push({ path: rawPath, kind: "file", ignored: true });
      budget.remaining -= 1;
    }
  }

  // Expand after the full collapsed listing so top-level ignored files are
  // never starved out of the budget by one large build directory.
  for (const directoryPath of expandable) {
    if (budget.remaining <= 0) break;
    await expandIgnoredDirectory(cwd, directoryPath, entries, budget);
  }

  return { entries, truncated: budget.remaining <= 0 };
}

/**
 * Enumerate gitignored entries under `cwd`. Failures (not a git repository,
 * git missing, timeout) degrade to an empty supplement — the index still
 * serves tracked files, matching the behavior before this supplement existed.
 */
export const listIgnoredEntries = Effect.fn("WorkspaceIgnoredEntries.list")(function* (
  cwd: string,
) {
  return yield* Effect.tryPromise({
    try: () => collectIgnoredEntries(cwd),
    catch: (cause) => new WorkspaceIgnoredEntriesFailed({ cwd, cause }),
  }).pipe(
    Effect.catchCause((cause) =>
      Effect.logDebug("Skipping gitignored-entries supplement", { cwd, cause }).pipe(
        Effect.as(emptyIgnoredEntriesSupplement),
      ),
    ),
  );
});
