// @effect-diagnostics nodeBuiltinImport:off
// @effect-diagnostics globalTimers:off
/**
 * WorkspaceContentSearch - ripgrep-backed project-wide content search.
 *
 * The plain timeout timer is intentional: it bounds a callback-based child
 * process from inside a promise, outside any Effect runtime.
 *
 * Streams ripgrep's `--json` output and folds match events into the
 * ProjectSearchContentResult contract shape, killing the child process as
 * soon as the match cap is reached. Search is read-only by design: replace
 * flows go through `projects.writeFile` with its `baseRevision` guard so
 * bulk edits inherit the same optimistic-concurrency safety as single-file
 * saves.
 *
 * @module WorkspaceContentSearch
 */
import * as NodeChildProcess from "node:child_process";
import * as NodeReadline from "node:readline";

import type { ProjectSearchContentInput, ProjectSearchContentResult } from "@t3tools/contracts";
import * as Context from "effect/Context";
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Schema from "effect/Schema";

import * as WorkspacePaths from "./WorkspacePaths.ts";

const SEARCH_DEFAULT_MAX_RESULTS = 500;
const SEARCH_HARD_MAX_RESULTS = 2000;
const SEARCH_TIMEOUT = Duration.seconds(20);
/** Longest lineText shipped to clients; longer lines are windowed around the match. */
const SEARCH_MAX_LINE_CHARS = 500;
/** Chars of context kept before the match when windowing an over-long line. */
const SEARCH_WINDOW_LEAD_CHARS = 100;

export class WorkspaceContentSearchInvalidPatternError extends Schema.TaggedErrorClass<WorkspaceContentSearchInvalidPatternError>()(
  "WorkspaceContentSearchInvalidPatternError",
  {
    workspaceRoot: Schema.String,
    detail: Schema.String,
  },
) {
  override get message(): string {
    return `Invalid search pattern for workspace '${this.workspaceRoot}': ${this.detail}`;
  }
}

export class WorkspaceContentSearchSpawnError extends Schema.TaggedErrorClass<WorkspaceContentSearchSpawnError>()(
  "WorkspaceContentSearchSpawnError",
  {
    workspaceRoot: Schema.String,
    cause: Schema.Defect(),
  },
) {
  override get message(): string {
    return `Failed to launch the search process for workspace '${this.workspaceRoot}'.`;
  }
}

export class WorkspaceContentSearchFailedError extends Schema.TaggedErrorClass<WorkspaceContentSearchFailedError>()(
  "WorkspaceContentSearchFailedError",
  {
    workspaceRoot: Schema.String,
    exitCode: Schema.NullOr(Schema.Number),
    detail: Schema.String,
  },
) {
  override get message(): string {
    return `Search process failed for workspace '${this.workspaceRoot}' (exit ${this.exitCode ?? "signal"}): ${this.detail}`;
  }
}

export const WorkspaceContentSearchError = Schema.Union([
  WorkspaceContentSearchInvalidPatternError,
  WorkspaceContentSearchSpawnError,
  WorkspaceContentSearchFailedError,
]);
export type WorkspaceContentSearchError = typeof WorkspaceContentSearchError.Type;

/** Service tag for workspace content search. */
export class WorkspaceContentSearch extends Context.Service<
  WorkspaceContentSearch,
  {
    readonly search: (
      input: ProjectSearchContentInput,
    ) => Effect.Effect<
      ProjectSearchContentResult,
      | WorkspaceContentSearchError
      | WorkspacePaths.WorkspaceRootNotExistsError
      | WorkspacePaths.WorkspaceRootCreateFailedError
      | WorkspacePaths.WorkspaceRootStatFailedError
      | WorkspacePaths.WorkspaceRootNotDirectoryError
    >;
  }
>()("t3/workspace/WorkspaceContentSearch") {}

/** Build the ripgrep argument list for a search input. Exported for tests. */
export function ripgrepArguments(input: ProjectSearchContentInput): Array<string> {
  const args = ["--json", "--no-config", "--max-columns", "10000"];
  if (input.regex !== true) args.push("--fixed-strings");
  if (input.caseSensitive === true) {
    args.push("--case-sensitive");
  } else {
    args.push("--smart-case");
  }
  if (input.wholeWord === true) args.push("--word-regexp");
  if (input.includeGlob !== undefined) args.push("--glob", input.includeGlob);
  if (input.excludeGlob !== undefined) args.push("--glob", `!${input.excludeGlob}`);
  args.push("--", input.query, ".");
  return args;
}

/**
 * Convert a byte offset in the UTF-8 encoding of `text` to a UTF-16 char
 * offset (ripgrep submatch offsets are byte-based; clients live in UTF-16).
 */
export function byteOffsetToCharOffset(text: string, byteOffset: number): number {
  if (byteOffset <= 0) return 0;
  const bytes = Buffer.from(text, "utf8");
  if (byteOffset >= bytes.length) return text.length;
  return bytes.subarray(0, byteOffset).toString("utf8").length;
}

export interface NormalizedMatchLine {
  readonly lineText: string;
  readonly lineTruncated: boolean;
  readonly matchStart: number;
  readonly matchEnd: number;
}

/**
 * Clamp an over-long matched line to a window around the match so huge
 * minified lines don't blow up the payload. Offsets are UTF-16 chars.
 */
export function normalizeMatchLine(
  fullLine: string,
  matchStartChar: number,
  matchEndChar: number,
): NormalizedMatchLine {
  const line = fullLine.replace(/\r?\n$/, "");
  const matchStart = Math.min(matchStartChar, line.length);
  const matchEnd = Math.min(Math.max(matchEndChar, matchStart), line.length);
  if (line.length <= SEARCH_MAX_LINE_CHARS) {
    return { lineText: line, lineTruncated: false, matchStart, matchEnd };
  }

  const windowStart = Math.max(0, matchStart - SEARCH_WINDOW_LEAD_CHARS);
  const windowEnd = Math.min(line.length, windowStart + SEARCH_MAX_LINE_CHARS);
  return {
    lineText: line.slice(windowStart, windowEnd),
    lineTruncated: true,
    matchStart: matchStart - windowStart,
    matchEnd: Math.min(matchEnd, windowEnd) - windowStart,
  };
}

type RipgrepSubmatch = { readonly start: number; readonly end: number };
type RipgrepMatchData = {
  readonly path?: { readonly text?: string };
  readonly line_number?: number;
  readonly lines?: { readonly text?: string };
  readonly submatches?: ReadonlyArray<RipgrepSubmatch>;
};

type MutableMatch = {
  path: string;
  line: number;
  lineText: string;
  lineTruncated: boolean;
  matchStart: number;
  matchEnd: number;
};

/** Parse one `--json` event line into matches. Exported for tests. */
export function parseRipgrepEventLine(jsonLine: string): Array<MutableMatch> {
  let parsed: unknown;
  try {
    parsed = JSON.parse(jsonLine);
  } catch {
    return [];
  }
  if (
    typeof parsed !== "object" ||
    parsed === null ||
    !("type" in parsed) ||
    parsed.type !== "match" ||
    !("data" in parsed)
  ) {
    return [];
  }
  const data = parsed.data as RipgrepMatchData;
  const rawPath = data.path?.text;
  const lineNumber = data.line_number;
  const rawLine = data.lines?.text;
  if (rawPath === undefined || lineNumber === undefined || rawLine === undefined) return [];
  const path = rawPath.replace(/^\.\//, "").replaceAll("\\", "/");

  const submatches = data.submatches ?? [];
  const matches: Array<MutableMatch> = [];
  for (const submatch of submatches) {
    const startChar = byteOffsetToCharOffset(rawLine, submatch.start);
    const endChar = byteOffsetToCharOffset(rawLine, submatch.end);
    const normalized = normalizeMatchLine(rawLine, startChar, endChar);
    matches.push({
      path,
      line: lineNumber,
      lineText: normalized.lineText,
      lineTruncated: normalized.lineTruncated,
      matchStart: normalized.matchStart,
      matchEnd: normalized.matchEnd,
    });
  }
  return matches;
}

async function resolveRipgrepPath(): Promise<string> {
  const module = await import("@vscode/ripgrep");
  return module.rgPath;
}

function runRipgrep(
  workspaceRoot: string,
  input: ProjectSearchContentInput,
  maxResults: number,
): Promise<ProjectSearchContentResult> {
  return new Promise((resolve, reject) => {
    void resolveRipgrepPath()
      .then((rgPath) => {
        const child = NodeChildProcess.spawn(rgPath, ripgrepArguments(input), {
          cwd: workspaceRoot,
          stdio: ["ignore", "pipe", "pipe"],
        });

        const matches: Array<MutableMatch> = [];
        const files = new Set<string>();
        let truncated = false;
        let settled = false;
        let stderrTail = "";

        const settle = (result: () => void) => {
          if (settled) return;
          settled = true;
          clearTimeout(timeout);
          result();
        };

        const finishSuccess = () =>
          settle(() =>
            resolve({
              matches,
              fileCount: files.size,
              truncated,
            }),
          );

        const timeout = setTimeout(() => {
          truncated = true;
          child.kill("SIGKILL");
          // Deliver partial results on timeout: a slow search that found
          // something beats an error that found nothing.
          finishSuccess();
        }, Duration.toMillis(SEARCH_TIMEOUT));

        child.on("error", (cause) =>
          settle(() => reject(new WorkspaceContentSearchSpawnError({ workspaceRoot, cause }))),
        );

        child.stderr.setEncoding("utf8");
        child.stderr.on("data", (chunk: string) => {
          stderrTail = (stderrTail + chunk).slice(-2000);
        });

        const lines = NodeReadline.createInterface({ input: child.stdout });
        lines.on("line", (line) => {
          if (settled || truncated) return;
          for (const match of parseRipgrepEventLine(line)) {
            if (matches.length >= maxResults) {
              truncated = true;
              child.kill("SIGKILL");
              return;
            }
            matches.push(match);
            files.add(match.path);
          }
        });

        child.on("close", (code) => {
          // 0 = matches found, 1 = no matches; both are successful searches.
          // 2 with a parse error on stderr = bad user pattern.
          if (code === 0 || code === 1 || truncated) {
            finishSuccess();
            return;
          }
          if (code === 2 && /regex parse error|error parsing glob/i.test(stderrTail)) {
            settle(() =>
              reject(
                new WorkspaceContentSearchInvalidPatternError({
                  workspaceRoot,
                  detail: stderrTail.trim().slice(0, 500),
                }),
              ),
            );
            return;
          }
          settle(() =>
            reject(
              new WorkspaceContentSearchFailedError({
                workspaceRoot,
                exitCode: code,
                detail: stderrTail.trim().slice(0, 500),
              }),
            ),
          );
        });
      })
      .catch((cause) => reject(new WorkspaceContentSearchSpawnError({ workspaceRoot, cause })));
  });
}

const isWorkspaceContentSearchError = Schema.is(WorkspaceContentSearchError);

export const make = Effect.gen(function* () {
  const workspacePaths = yield* WorkspacePaths.WorkspacePaths;

  const search: WorkspaceContentSearch["Service"]["search"] = Effect.fn(
    "WorkspaceContentSearch.search",
  )(function* (input) {
    const workspaceRoot = yield* workspacePaths.normalizeWorkspaceRoot(input.cwd);
    const maxResults = Math.min(
      input.maxResults ?? SEARCH_DEFAULT_MAX_RESULTS,
      SEARCH_HARD_MAX_RESULTS,
    );

    return yield* Effect.tryPromise({
      try: () => runRipgrep(workspaceRoot, input, maxResults),
      catch: (cause) =>
        isWorkspaceContentSearchError(cause)
          ? cause
          : new WorkspaceContentSearchSpawnError({ workspaceRoot, cause }),
    });
  });

  return WorkspaceContentSearch.of({ search });
});

export const layer = Layer.effect(WorkspaceContentSearch, make);
