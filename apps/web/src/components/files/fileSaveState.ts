import type { EnvironmentId, ProjectWriteFileResult } from "@t3tools/contracts";
import type { AtomCommandResult } from "@t3tools/client-runtime/state/runtime";
import * as Option from "effect/Option";
import { AsyncResult } from "effect/unstable/reactivity";

import { fileContentRevision } from "./fileContentRevision";
import { isStaleRevisionWriteFailure } from "./fileBufferConflict";
import {
  confirmProjectFileQueryData,
  getOptimisticProjectFileQueryData,
} from "./projectFilesQueryState";

/**
 * Module-scoped save bookkeeping for editable file buffers.
 *
 * The editor mounts one file surface at a time, but with autosave off a
 * buffer can stay dirty across tab switches (it survives in the optimistic
 * file atom). The base revision its edits are relative to and the revisions
 * this editor has written must survive with it — a component ref dies on
 * unmount, so both live here, keyed per file.
 */

const KEY_SEPARATOR = "\u0000";

function fileKey(environmentId: EnvironmentId, cwd: string, relativePath: string): string {
  return `${environmentId}${KEY_SEPARATOR}${cwd}${KEY_SEPARATOR}${relativePath}`;
}

// ── Base revisions ─────────────────────────────────────────────

const baseRevisions = new Map<string, string>();

export function getFileBaseRevision(
  environmentId: EnvironmentId,
  cwd: string,
  relativePath: string,
): string | null {
  return baseRevisions.get(fileKey(environmentId, cwd, relativePath)) ?? null;
}

export function setFileBaseRevision(
  environmentId: EnvironmentId,
  cwd: string,
  relativePath: string,
  revision: string | null,
): void {
  const key = fileKey(environmentId, cwd, relativePath);
  if (revision === null) {
    baseRevisions.delete(key);
  } else {
    baseRevisions.set(key, revision);
  }
}

// ── Self-written revisions ─────────────────────────────────────

/**
 * Revisions this editor wrote (or has in flight). Recorded *before* the write
 * RPC is sent so a watcher-triggered refresh racing the write confirmation
 * still recognizes the new disk revision as our own save rather than an
 * external change. Revisions are content hashes, so a matching disk revision
 * means the disk bytes equal something this editor saved — never worth a
 * conflict warning. Bounded FIFO per file to cap memory.
 */
const SELF_WRITTEN_REVISION_LIMIT = 32;
const selfWrittenRevisions = new Map<string, string[]>();

export function recordSelfWrittenRevision(
  environmentId: EnvironmentId,
  cwd: string,
  relativePath: string,
  revision: string,
): void {
  const key = fileKey(environmentId, cwd, relativePath);
  const revisions = selfWrittenRevisions.get(key) ?? [];
  const existingIndex = revisions.indexOf(revision);
  if (existingIndex !== -1) revisions.splice(existingIndex, 1);
  revisions.push(revision);
  if (revisions.length > SELF_WRITTEN_REVISION_LIMIT) revisions.shift();
  selfWrittenRevisions.set(key, revisions);
}

export function isSelfWrittenRevision(
  environmentId: EnvironmentId,
  cwd: string,
  relativePath: string,
  revision: string,
): boolean {
  return (
    selfWrittenRevisions.get(fileKey(environmentId, cwd, relativePath))?.includes(revision) ?? false
  );
}

// ── Persisting unmounted buffers ───────────────────────────────

export type WriteProjectFile = (args: {
  readonly environmentId: EnvironmentId;
  readonly input: {
    readonly cwd: string;
    readonly relativePath: string;
    readonly contents: string;
    readonly baseRevision?: string;
  };
}) => Promise<AtomCommandResult<ProjectWriteFileResult, unknown>>;

export type PersistUnsavedBufferOutcome = "saved" | "clean" | "stale" | "error";

/**
 * Save a dirty buffer that is not currently mounted in the editor (e.g. a
 * background tab being closed via the unsaved-changes prompt). Reads the
 * buffer from the optimistic file atom, writes it with the base-revision
 * guard, and confirms the atom on success. "stale" means the file changed on
 * disk since the edits were made — the caller should keep the tab open so the
 * conflict banner can resolve it.
 */
export async function persistUnsavedBuffer(
  environmentId: EnvironmentId,
  cwd: string,
  relativePath: string,
  writeFile: WriteProjectFile,
): Promise<PersistUnsavedBufferOutcome> {
  const buffer = getOptimisticProjectFileQueryData(environmentId, cwd, relativePath);
  if (buffer === null) return "clean";

  const baseRevision = getFileBaseRevision(environmentId, cwd, relativePath);
  recordSelfWrittenRevision(environmentId, cwd, relativePath, fileContentRevision(buffer.contents));
  const result = await writeFile({
    environmentId,
    input: {
      cwd,
      relativePath,
      contents: buffer.contents,
      ...(baseRevision === null ? {} : { baseRevision }),
    },
  });
  if (result._tag === "Success") {
    const revision = Option.getOrNull(AsyncResult.value(result))?.revision ?? null;
    setFileBaseRevision(environmentId, cwd, relativePath, revision);
    if (revision !== null) {
      recordSelfWrittenRevision(environmentId, cwd, relativePath, revision);
    }
    confirmProjectFileQueryData(environmentId, cwd, relativePath, buffer.contents);
    return "saved";
  }
  return isStaleRevisionWriteFailure(result) ? "stale" : "error";
}
