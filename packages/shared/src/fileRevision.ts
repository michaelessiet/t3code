/**
 * Content-derived revision tokens for workspace files.
 *
 * A revision is a cheap FNV-1a hash of a file's decoded text contents. Both
 * the server (when reading/writing workspace files) and clients (when editing
 * buffers locally) compute revisions with this function, so a buffer can be
 * compared against disk state without shipping file contents around.
 *
 * Content hashes are used instead of mtimes because they stay reliable across
 * WSL/network filesystems and identical rewrites.
 *
 * @module fileRevision
 */
export function fileContentRevision(contents: string): string {
  let hash = 2_166_136_261;
  for (let index = 0; index < contents.length; index += 1) {
    hash ^= contents.charCodeAt(index);
    hash = Math.imul(hash, 16_777_619);
  }
  return `${contents.length}:${(hash >>> 0).toString(36)}`;
}
