import type { AtomCommandResult } from "@t3tools/client-runtime/state/runtime";
import { squashAtomCommandFailure } from "@t3tools/client-runtime/state/runtime";

/**
 * Concurrent-edit conflict state for an editable file buffer.
 *
 * - `external-change`: the workspace watcher reported the file changed on
 *   disk (e.g. an agent or terminal command rewrote it) while the buffer held
 *   unsaved edits.
 * - `stale-save`: the server rejected a save because the buffer's base
 *   revision no longer matches the disk contents.
 *
 * Both resolve the same way: reload from disk (discard local edits) or keep
 * the local buffer (force-write over the disk state).
 */
export type FileBufferConflictReason = "external-change" | "stale-save";

export function isStaleRevisionWriteFailure(result: AtomCommandResult<unknown, unknown>): boolean {
  if (result._tag !== "Failure") return false;
  const failure = squashAtomCommandFailure(result);
  return (
    typeof failure === "object" &&
    failure !== null &&
    "_tag" in failure &&
    failure._tag === "ProjectWriteFileError" &&
    "failure" in failure &&
    failure.failure === "stale_revision"
  );
}

/**
 * A dirty buffer conflicts with the disk when the disk revision moved away
 * from the revision the buffer's edits are based on. Clean buffers follow the
 * disk instead, and unknown revisions (older servers) never conflict.
 */
export function detectsExternalConflict(input: {
  readonly dirty: boolean;
  readonly baseRevision: string | null;
  readonly diskRevision: string | undefined;
}): boolean {
  return (
    input.dirty &&
    input.baseRevision !== null &&
    input.diskRevision !== undefined &&
    input.diskRevision !== input.baseRevision
  );
}
