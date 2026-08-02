/**
 * Session-scoped memory of cursor/scroll positions for closed file editors.
 *
 * Every file switch destroys the EditorView (the surface is keyed by path),
 * so position must be captured right before destroy and re-applied on the
 * next mount. Deliberately in-memory only: after an app restart contents
 * come fresh from disk and the persisted revealLine already gives a coarse
 * position, so persisting exact offsets would mostly restore stale state.
 */
export interface CachedEditorViewState {
  /** Selection as raw doc offsets; clamped to the doc length on restore. */
  readonly anchor: number;
  readonly head: number;
  /** Raw scrollDOM.scrollTop pixels; the DOM clamps out-of-range values. */
  readonly scrollTop: number;
  /**
   * revealRequestId in effect when captured. A different id on the next
   * mount means an explicit reveal (go-to-def, search jump) happened since —
   * that reveal must win over the remembered position.
   */
  readonly revealRequestId: number | undefined;
}

const MAX_ENTRIES = 200;

/** Map insertion order doubles as LRU order: re-saving bumps to newest. */
const cache = new Map<string, CachedEditorViewState>();

export function saveEditorViewState(key: string, state: CachedEditorViewState): void {
  cache.delete(key);
  cache.set(key, state);
  if (cache.size > MAX_ENTRIES) {
    const oldest = cache.keys().next().value;
    if (oldest !== undefined) cache.delete(oldest);
  }
}

export function loadEditorViewState(key: string): CachedEditorViewState | null {
  return cache.get(key) ?? null;
}

/**
 * Restore the remembered position unless a fresh explicit reveal is pending:
 * a bumped revealRequestId with a concrete revealLine. A bump with a null
 * line (plain file-tree re-open) has nothing to reveal, so restore wins.
 */
export function shouldRestoreEditorViewState(
  saved: CachedEditorViewState | null,
  revealLine: number | null,
  revealRequestId: number | undefined,
): saved is CachedEditorViewState {
  if (saved === null) return false;
  return revealLine === null || revealRequestId === saved.revealRequestId;
}
