/**
 * Pure key derivation for the graph store.
 *
 * Split out of `GraphStore.ts` because every safety property of the store
 * rests on these three functions being correct, and they are far easier to
 * exhaustively table-test with no filesystem in the way.
 *
 * The inputs are not trusted. A branch name comes from the repository, and
 * `feat/../../..` is a legal-looking ref fragment; a project-id directory name
 * is read back off disk and could be anything. Everything here is written to
 * make an escape impossible rather than unlikely.
 *
 * @module graphStoreKey
 */
import * as NodeCrypto from "node:crypto";

/**
 * Basename of the directory `GRAPHIFY_OUT` points at.
 *
 * **This must stay `graphify-out`.** graphify's `detect.py` excludes its own
 * output from a scan by matching `GRAPHIFY_OUT_NAME` as a *path segment name*,
 * not as a full path. Name the leaf `out` or `main` instead and every
 * directory with that name anywhere in the user's repository silently vanishes
 * from the graph.
 */
export const GRAPHIFY_OUT_DIR_NAME = "graphify-out";

/** Name of the T3-owned sidecar. graphify never reads or writes it. */
export const GRAPH_META_FILE_NAME = "meta.json";

/**
 * Keeps a directory name readable in a terminal. The hash suffix carries
 * uniqueness, so truncating the slug is lossless for correctness.
 */
const MAX_SLUG_LENGTH = 48;
const HASH_LENGTH = 8;

/** A UUIDv4, which is what `ProjectId` is. Anything else is not ours. */
const PROJECT_ID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

/** `<slug>-<8 hex>`, the only shape `graphStoreDirectoryName` can produce. */
const ENTRY_DIRECTORY_PATTERN = new RegExp(`^[a-z0-9][a-z0-9._-]*-[0-9a-f]{${HASH_LENGTH}}$`);

export interface GraphStoreKeyInput {
  /** Null when HEAD is detached; `headSha` then supplies the identity. */
  readonly branch: string | null;
  readonly headSha: string | null;
}

/**
 * The string a store entry is really keyed on.
 *
 * Distinct from the directory name because sanitisation is lossy: `feat/x` and
 * `feat-x` slug identically, so the hash of *this* value is what keeps them
 * apart on disk.
 */
export function graphStoreIdentity(input: GraphStoreKeyInput): string {
  const branch = input.branch?.trim() ?? "";
  if (branch !== "") return `branch:${branch}`;
  const sha = input.headSha?.trim() ?? "";
  return sha === "" ? "detached:unknown" : `detached:${sha}`;
}

function slugify(value: string): string {
  const slug = value
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "-")
    // Leading dots would allow `.` and `..`; trailing punctuation is noise.
    .replace(/^[.\-_]+/, "")
    .replace(/[.\-_]+$/, "")
    .slice(0, MAX_SLUG_LENGTH)
    .replace(/[.\-_]+$/, "");
  return slug === "" ? "branch" : slug;
}

function shortHash(value: string): string {
  return NodeCrypto.createHash("sha256").update(value, "utf8").digest("hex").slice(0, HASH_LENGTH);
}

/**
 * Directory name for one stored graph: a human-greppable slug plus a digest of
 * the full identity, so no two distinct branches can ever collide after
 * sanitisation.
 */
export function graphStoreDirectoryName(input: GraphStoreKeyInput): string {
  const identity = graphStoreIdentity(input);
  const branch = input.branch?.trim() ?? "";
  const sha = input.headSha?.trim() ?? "";
  const base =
    branch !== ""
      ? slugify(branch)
      : sha === ""
        ? "detached"
        : slugify(`detached-${sha.slice(0, HASH_LENGTH)}`);
  return `${base}-${shortHash(identity)}`;
}

/** True when `value` could have been produced by `graphStoreDirectoryName`. */
export function isGraphStoreDirectoryName(value: string): boolean {
  return ENTRY_DIRECTORY_PATTERN.test(value);
}

/** True when `value` is a `ProjectId` and therefore a directory we minted. */
export function isProjectIdDirectoryName(value: string): boolean {
  return PROJECT_ID_PATTERN.test(value);
}
