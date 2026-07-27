/**
 * "Is there a graph for this checkout?", answerable without a service.
 *
 * The Claude adapter needs this to decide whether to tell the model a knowledge
 * graph exists, and it cannot ask `GraphService`: in the `provideMerge` chain in
 * `server.ts` later entries provide to earlier ones, and the whole graph stack
 * sits *above* `ProviderRuntimeLayerLive` precisely because it consumes
 * `GitVcsDriver`, `ProjectionSnapshotQuery` and `ServerSettingsService` from
 * below it. Pulling the dependency the other way would mean reordering a chain
 * already at the twenty-argument `.pipe()` limit.
 *
 * So the graph stack *pushes* instead, exactly like `McpProviderSession` — a
 * plain module-level map that the MCP layer writes and the adapter reads on the
 * line above this one's call site. `GraphStore` is the only writer, because it
 * is the only module allowed to know the store's layout.
 *
 * Freshness comes for free: `GraphStoreSweep` calls `GraphStore.list` on a
 * `Schedule.spaced`, which fires once immediately at boot, so the map is
 * populated before the first prompt without anything having to open a panel.
 *
 * @module graph/GraphAvailability
 */

/** What a caller can learn about the graph covering one checkout. */
export interface GraphAvailability {
  /** Branch the graph was built from; null for a detached HEAD. */
  readonly branch: string | null;
  readonly nodeCount: number;
  readonly edgeCount: number;
  /** Epoch millis the graph was built. */
  readonly builtAt: number;
}

/**
 * Keyed by workspace root rather than by store key, because the reader has a
 * `cwd` and no way to resolve it to a `(projectId, branch)` pair — resolving it
 * is what needs the services that are out of reach.
 */
const byWorkspaceRoot = new Map<string, GraphAvailability>();

/**
 * Canonical form of a workspace root.
 *
 * Only trailing separators are stripped. Resolving symlinks or case would need
 * disk access on a path that may no longer exist, and the costs are lopsided:
 * being too strict loses a prompt note, while being too loose would describe
 * some other checkout's graph as this one's.
 */
export function normalizeWorkspaceRoot(root: string): string {
  const trimmed = root.trim().replace(/[/\\]+$/, "");
  return trimmed === "" ? root.trim() : trimmed;
}

/** Whether `candidate` should displace `existing` as the entry for a checkout. */
function supersedes(
  candidate: GraphAvailability,
  existing: GraphAvailability | undefined,
): boolean {
  return existing === undefined || candidate.builtAt >= existing.builtAt;
}

/**
 * Record one entry, keeping the most recently built per checkout.
 *
 * A checkout accumulates an entry per branch it has been built on, and the
 * reader cannot tell which branch is current. Newest-wins is the honest choice,
 * and {@link GraphAvailability.branch} lets the reader say which one it got.
 */
export function recordGraphAvailability(
  workspaceRoot: string,
  availability: GraphAvailability,
): void {
  const key = normalizeWorkspaceRoot(workspaceRoot);
  if (key === "") return;
  if (supersedes(availability, byWorkspaceRoot.get(key))) {
    byWorkspaceRoot.set(key, availability);
  }
}

/**
 * Replace the whole map from a full listing of the store.
 *
 * Reconciling rather than merging is what lets an eviction disappear from here:
 * the sweep that deleted the directory is the same pass that re-lists it, so a
 * dropped entry stops being advertised on the next sweep without eviction
 * needing to notify anyone.
 */
export function reconcileGraphAvailability(
  entries: Iterable<{ readonly workspaceRoot: string } & GraphAvailability>,
): void {
  byWorkspaceRoot.clear();
  for (const entry of entries) {
    recordGraphAvailability(entry.workspaceRoot, entry);
  }
}

/**
 * The graph covering `directory`, or undefined when there is none.
 *
 * Walks up to the filesystem root rather than requiring an exact hit. Entries
 * are keyed by `git rev-parse --show-toplevel`, so a thread whose cwd is a
 * subdirectory of the repository would otherwise never find its own graph — and
 * that graph does cover the subdirectory, so the miss would be pure loss.
 *
 * Ascent is segment-by-segment, never by string prefix, so `/repo/app` cannot
 * answer for `/repo/app-2`.
 */
export function readGraphAvailability(directory: string): GraphAvailability | undefined {
  let key = normalizeWorkspaceRoot(directory);
  for (;;) {
    const found = byWorkspaceRoot.get(key);
    if (found !== undefined) return found;
    const cut = Math.max(key.lastIndexOf("/"), key.lastIndexOf("\\"));
    // No separator left: a bare name, or a Windows drive like `C:`.
    if (cut < 0) return undefined;
    // `cut === 0` means the parent is the posix root, which normalizes to
    // itself rather than to the empty string — check it once and stop.
    if (cut === 0) return byWorkspaceRoot.get(key.slice(0, 1));
    key = key.slice(0, cut);
  }
}

/** Drop everything. For tests, which must not leak state between cases. */
export function clearGraphAvailability(): void {
  byWorkspaceRoot.clear();
}
