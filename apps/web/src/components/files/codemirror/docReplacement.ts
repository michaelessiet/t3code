export interface DocReplacement {
  readonly from: number;
  readonly to: number;
  readonly insert: string;
}

/**
 * Minimal single-span replacement turning `current` into `next`, computed by
 * trimming the common prefix and suffix. Returns null when the strings are
 * equal. Applying external reloads as a minimal change lets CodeMirror map
 * the cursor and decorations through the edit instead of resetting them.
 */
export function computeDocReplacement(current: string, next: string): DocReplacement | null {
  if (current === next) return null;

  let prefixLength = 0;
  const maxPrefixLength = Math.min(current.length, next.length);
  while (prefixLength < maxPrefixLength && current[prefixLength] === next[prefixLength]) {
    prefixLength += 1;
  }

  let currentEnd = current.length;
  let nextEnd = next.length;
  while (
    currentEnd > prefixLength &&
    nextEnd > prefixLength &&
    current[currentEnd - 1] === next[nextEnd - 1]
  ) {
    currentEnd -= 1;
    nextEnd -= 1;
  }

  return {
    from: prefixLength,
    to: currentEnd,
    insert: next.slice(prefixLength, nextEnd),
  };
}
