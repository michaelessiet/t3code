import type { ProjectSearchContentInput, ProjectSearchContentMatch } from "@t3tools/contracts";

/**
 * The subset of the search input that defines what a match is. Mirrors the
 * server's ripgrep semantics closely enough that client-side replacement
 * rewrites the same spans the server reported.
 */
export type SearchPatternOptions = Pick<
  ProjectSearchContentInput,
  "query" | "regex" | "caseSensitive" | "wholeWord"
>;

const REGEXP_SPECIALS = /[\\^$.*+?()[\]{}|]/g;

export function escapeRegExpLiteral(text: string): string {
  return text.replace(REGEXP_SPECIALS, "\\$&");
}

/**
 * Mirrors ripgrep's `--smart-case` default: matching is case-insensitive
 * unless the query contains an uppercase letter or case sensitivity was
 * requested explicitly.
 */
export function isEffectivelyCaseSensitive(options: SearchPatternOptions): boolean {
  return options.caseSensitive === true || /[A-Z]/.test(options.query);
}

/**
 * Builds the client-side equivalent of the server's search pattern as a
 * global JS RegExp. Returns null for empty queries and for regex-mode queries
 * that are not valid JS regular expressions.
 */
export function buildSearchRegExp(options: SearchPatternOptions): RegExp | null {
  if (options.query.length === 0) return null;
  const source = options.regex === true ? options.query : escapeRegExpLiteral(options.query);
  const wrapped = options.wholeWord === true ? `\\b(?:${source})\\b` : source;
  const flags = isEffectivelyCaseSensitive(options) ? "g" : "gi";
  try {
    return new RegExp(wrapped, flags);
  } catch {
    return null;
  }
}

export interface ReplacementComputation {
  readonly contents: string;
  readonly replacedCount: number;
}

/**
 * Applies the search pattern to full file contents client-side. Literal mode
 * inserts `replaceText` verbatim (no `$` expansion); regex mode supports JS
 * replacement patterns such as `$1` and `$&`. Returns null when the pattern
 * is empty or invalid.
 */
export function computeReplacements(
  contents: string,
  input: SearchPatternOptions,
  replaceText: string,
): ReplacementComputation | null {
  const pattern = buildSearchRegExp(input);
  if (pattern === null) return null;

  let replacedCount = 0;
  for (const _match of contents.matchAll(pattern)) {
    replacedCount += 1;
  }
  if (replacedCount === 0) {
    return { contents, replacedCount: 0 };
  }

  pattern.lastIndex = 0;
  const nextContents =
    input.regex === true
      ? contents.replace(pattern, replaceText)
      : contents.replace(pattern, () => replaceText);
  return { contents: nextContents, replacedCount };
}

export interface SearchMatchFileGroup {
  readonly path: string;
  readonly matches: ReadonlyArray<ProjectSearchContentMatch>;
}

/** Groups matches by file path, preserving the server's result order. */
export function groupMatchesByFile(
  matches: ReadonlyArray<ProjectSearchContentMatch>,
): SearchMatchFileGroup[] {
  const groups = new Map<string, ProjectSearchContentMatch[]>();
  for (const match of matches) {
    const existing = groups.get(match.path);
    if (existing === undefined) {
      groups.set(match.path, [match]);
    } else {
      existing.push(match);
    }
  }
  return [...groups.entries()].map(([path, fileMatches]) => ({ path, matches: fileMatches }));
}

export interface MatchLineSegments {
  readonly before: string;
  readonly matched: string;
  readonly after: string;
  /** True when leading context was cut to keep the match visible. */
  readonly beforeClipped: boolean;
}

const MATCH_CONTEXT_PREFIX_MAX = 32;

/**
 * Splits a match line into before/matched/after display segments. Leading
 * whitespace is dropped and long prefixes are clipped so the highlighted span
 * stays visible in a narrow panel. Offsets are clamped defensively because
 * `lineText` may be truncated by the server.
 */
export function matchLineSegments(
  match: Pick<ProjectSearchContentMatch, "lineText" | "matchStart" | "matchEnd">,
): MatchLineSegments {
  const { lineText } = match;
  const start = Math.min(Math.max(0, match.matchStart), lineText.length);
  const end = Math.min(Math.max(start, match.matchEnd), lineText.length);

  let before = lineText.slice(0, start).trimStart();
  let beforeClipped = false;
  if (before.length > MATCH_CONTEXT_PREFIX_MAX) {
    before = before.slice(before.length - MATCH_CONTEXT_PREFIX_MAX);
    beforeClipped = true;
  }
  return {
    before,
    matched: lineText.slice(start, end),
    after: lineText.slice(end),
    beforeClipped,
  };
}

/** "src/lib/utils.ts" → { name: "utils.ts", directory: "src/lib" } */
export function splitSearchResultPath(path: string): {
  readonly name: string;
  readonly directory: string;
} {
  const separatorIndex = path.lastIndexOf("/");
  if (separatorIndex < 0) return { name: path, directory: "" };
  return { name: path.slice(separatorIndex + 1), directory: path.slice(0, separatorIndex) };
}
