import * as Schema from "effect/Schema";

import { NonNegativeInt, PositiveInt, TrimmedNonEmptyString } from "./baseSchemas.ts";

const SEARCH_CONTENT_QUERY_MAX_LENGTH = 512;
const SEARCH_CONTENT_GLOB_MAX_LENGTH = 256;
const SEARCH_CONTENT_MAX_RESULTS_LIMIT = 2000;

/**
 * Project-wide content search (ripgrep-backed).
 *
 * Replace is deliberately not a server mutation: clients apply replacements
 * per file through `projects.writeFile` with a `baseRevision` guard, which
 * reuses the optimistic-concurrency safety net instead of introducing a
 * second bulk-write surface.
 */
export const ProjectSearchContentInput = Schema.Struct({
  cwd: TrimmedNonEmptyString,
  query: Schema.String.check(
    Schema.isMinLength(1),
    Schema.isMaxLength(SEARCH_CONTENT_QUERY_MAX_LENGTH),
  ),
  /** Treat `query` as a regular expression (ripgrep syntax). Default: literal. */
  regex: Schema.optional(Schema.Boolean),
  /** Case-sensitive matching. Default: smart case (ripgrep --smart-case). */
  caseSensitive: Schema.optional(Schema.Boolean),
  /** Match whole words only. */
  wholeWord: Schema.optional(Schema.Boolean),
  /** Only search paths matching this glob (ripgrep glob syntax). */
  includeGlob: Schema.optional(
    TrimmedNonEmptyString.check(Schema.isMaxLength(SEARCH_CONTENT_GLOB_MAX_LENGTH)),
  ),
  /** Skip paths matching this glob. */
  excludeGlob: Schema.optional(
    TrimmedNonEmptyString.check(Schema.isMaxLength(SEARCH_CONTENT_GLOB_MAX_LENGTH)),
  ),
  /** Cap on returned matches; the server clamps to its own maximum. */
  maxResults: Schema.optional(
    PositiveInt.check(Schema.isLessThanOrEqualTo(SEARCH_CONTENT_MAX_RESULTS_LIMIT)),
  ),
});
export type ProjectSearchContentInput = typeof ProjectSearchContentInput.Type;

export const ProjectSearchContentMatch = Schema.Struct({
  /** Workspace-root-relative path, `/` separated. */
  path: TrimmedNonEmptyString,
  /** 1-based line number of the match. */
  line: PositiveInt,
  /** Text of the matched line (may be truncated for very long lines). */
  lineText: Schema.String,
  /** True when `lineText` was truncated. */
  lineTruncated: Schema.Boolean,
  /** 0-based char offset of the match within `lineText`. */
  matchStart: NonNegativeInt,
  /** 0-based char offset of the end of the match within `lineText`. */
  matchEnd: NonNegativeInt,
});
export type ProjectSearchContentMatch = typeof ProjectSearchContentMatch.Type;

export const ProjectSearchContentResult = Schema.Struct({
  matches: Schema.Array(ProjectSearchContentMatch),
  /** Number of distinct files with at least one match. */
  fileCount: NonNegativeInt,
  /** True when the match cap was hit; more matches exist. */
  truncated: Schema.Boolean,
});
export type ProjectSearchContentResult = typeof ProjectSearchContentResult.Type;

export const ProjectSearchContentFailure = Schema.Literals([
  "workspace_root_not_found",
  "invalid_pattern",
  "search_spawn_failed",
  "search_failed",
]);
export type ProjectSearchContentFailure = typeof ProjectSearchContentFailure.Type;

export class ProjectSearchContentError extends Schema.TaggedErrorClass<ProjectSearchContentError>()(
  "ProjectSearchContentError",
  {
    cwd: Schema.optional(TrimmedNonEmptyString),
    failure: Schema.optional(ProjectSearchContentFailure),
    detail: Schema.optional(TrimmedNonEmptyString),
    message: TrimmedNonEmptyString,
    cause: Schema.optional(Schema.Defect()),
  },
) {
  // Structured fields stay optional on the wire for cross-version decoding;
  // application code supplies them through this constructor.
  // @effect-diagnostics-next-line overriddenSchemaConstructor:off
  constructor(props: {
    readonly cwd: string;
    readonly failure: ProjectSearchContentFailure;
    readonly detail?: string;
    readonly cause?: unknown;
  }) {
    super({
      ...props,
      message: `Failed to search workspace contents in '${props.cwd}'.`,
    } as any);
  }
}
