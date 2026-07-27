/**
 * GraphifyCli - the only module that spawns graphify.
 *
 * Argument and environment construction are pure functions exported for tests,
 * mirroring `ripgrepArguments` in `WorkspaceContentSearch.ts`. The service on
 * top of them is thin: resolve the runtime, run one command through
 * `ProcessRunner`, map a non-zero exit to `GraphCommandFailedError`.
 *
 * ## Why `GRAPHIFY_OUT` must be absolute
 *
 * This is the single line standing between the feature and writing into the
 * user's repository. `graphify/cli.py:2639` computes its output directory as:
 *
 * ```python
 * out_root = (out_dir.resolve() if out_dir else target)   # target = the scanned path
 * graphify_out = out_root / _GRAPHIFY_OUT                 # _GRAPHIFY_OUT = $GRAPHIFY_OUT
 * ```
 *
 * `pathlib` discards the left operand when the right is absolute, so
 * `Path("/repo") / "/t3/store/graphify-out"` is `/t3/store/graphify-out` — the
 * override escapes the repo *because it is absolute*, not because the code
 * checks for it. A relative `GRAPHIFY_OUT` would silently land in
 * `<repo>/<value>` instead. `graphifyEnv` therefore refuses a relative path
 * rather than trusting a caller to pass the right kind.
 *
 * The basename must also stay `graphify-out`; see `graphStoreKey.ts`.
 *
 * @module GraphifyCli
 */
import {
  GraphCommandFailedError,
  type GraphDisabledError,
  type GraphRuntimeUnavailableError,
  type ServerSettingsError,
} from "@t3tools/contracts";
import * as Context from "effect/Context";
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";

import { ProcessRunner } from "../processRunner.ts";
import { GraphifyRuntime } from "./GraphifyRuntime.ts";
import { GRAPHIFY_OUT_DIR_NAME } from "./graphStoreKey.ts";

/**
 * A full structural extraction of a large monorepo is minutes, not seconds.
 * Long, but bounded: a build that never returns is worse than one that fails.
 */
const BUILD_TIMEOUT = Duration.minutes(30);
/** graphify is chatty; keep the tail for diagnostics and drop the rest. */
const BUILD_MAX_OUTPUT_BYTES = 1024 * 1024;
/** How much output travels back to the client on failure. */
const FAILURE_DETAIL_CHARS = 4000;

export interface GraphifyBuildInvocation {
  /** Repository being scanned. graphify never writes here. */
  readonly workspaceRoot: string;
  /** Absolute `graphify-out` directory inside T3's store. */
  readonly outDir: string;
  readonly mode: "structural" | "semantic";
  /** Re-extract everything rather than only what changed. */
  readonly force: boolean;
  /**
   * Refresh an existing graph in place. Requires a previous full build, so the
   * caller decides based on whether `graph.json` is already there.
   */
  readonly incremental: boolean;
}

/**
 * Which LLM graphify uses for semantic extraction.
 *
 * `claude-cli` is the only backend in `llm.py`'s `BACKENDS` table that needs no
 * API key: `_call_backend` skips the key check for it (`llm.py:1701`) and
 * shells out to `claude -p --output-format json` instead, so extraction bills
 * whatever Claude Code subscription the user is already signed in with. Every
 * other backend would mean asking for a `GEMINI_API_KEY` before the feature
 * does anything, which is a worse first run for a product whose users have the
 * Claude CLI installed by definition.
 *
 * It is not free — it spends tokens — which is why semantic mode is a separate,
 * explicitly confirmed action and never what auto-rebuild picks. If `claude` is
 * not on PATH graphify exits non-zero with "Claude Code CLI not found on
 * $PATH", which lands in the build failure detail verbatim.
 */
export const GRAPHIFY_SEMANTIC_BACKEND = "claude-cli";

/**
 * Arguments for one graphify invocation.
 *
 * `--code-only` is what makes the structural mode free and keyless: it runs
 * graphify's local AST pass and skips semantic extraction entirely, so no API
 * key is needed and nothing leaves the machine. Semantic mode omits it and
 * names a backend instead — without one, `cli.py:2857` exits 1 with "no LLM API
 * key found" rather than falling back to anything.
 */
export function graphifyArguments(input: GraphifyBuildInvocation): ReadonlyArray<string> {
  if (input.incremental) {
    // `update` re-extracts code files only and has no --code-only flag: it is
    // already AST-only by construction (`cli.py:1816` → `watch._rebuild_code`).
    return ["update", input.workspaceRoot, ...(input.force ? ["--force"] : [])];
  }
  return [
    "extract",
    input.workspaceRoot,
    ...(input.mode === "structural" ? ["--code-only"] : ["--backend", GRAPHIFY_SEMANTIC_BACKEND]),
    ...(input.force ? ["--force"] : []),
  ];
}

/**
 * Environment for one graphify invocation.
 *
 * `ProcessRunner` spawns with `extendEnv: true`, so this layers onto the
 * inherited environment rather than replacing it.
 */
export function graphifyEnv(input: { readonly outDir: string }): NodeJS.ProcessEnv {
  if (!input.outDir.startsWith("/") && !/^[a-zA-Z]:[\\/]/.test(input.outDir)) {
    // Not an assertion for style's sake: a relative value here means graphify
    // writes `<repo>/<value>` and the store guarantee is silently broken.
    throw new Error(`GRAPHIFY_OUT must be an absolute path, got '${input.outDir}'`);
  }
  if (!input.outDir.endsWith(GRAPHIFY_OUT_DIR_NAME)) {
    // `detect.py` excludes the output from its own scan by matching the
    // *basename*. A differently-named leaf would make graphify index its own
    // output, and would exclude every same-named directory in the repo.
    throw new Error(`GRAPHIFY_OUT must end in '${GRAPHIFY_OUT_DIR_NAME}', got '${input.outDir}'`);
  }
  return {
    GRAPHIFY_OUT: input.outDir,
    // graphify keeps dated copies of a semantic or curated graph before an
    // overwrite (`export.py:34`). T3 owns this directory and rebuilds are
    // cheap, so the backups would only grow the store the sweep has to reclaim.
    GRAPHIFY_NO_BACKUP: "1",
    // Suppress the "set GEMINI_API_KEY" tips: they are advice for someone at a
    // terminal, and this output is a log tail in a UI.
    GRAPHIFY_NO_TIPS: "1",
  };
}

export interface GraphifyRunResult {
  readonly stdout: string;
  readonly stderr: string;
}

export class GraphifyCli extends Context.Service<
  GraphifyCli,
  {
    readonly build: (
      invocation: GraphifyBuildInvocation,
    ) => Effect.Effect<
      GraphifyRunResult,
      | GraphDisabledError
      | GraphRuntimeUnavailableError
      | GraphCommandFailedError
      | ServerSettingsError
    >;
  }
>()("t3/graph/GraphifyCli") {}

function truncateTail(text: string): string {
  const trimmed = text.trim();
  return trimmed.length <= FAILURE_DETAIL_CHARS
    ? trimmed
    : `…${trimmed.slice(-FAILURE_DETAIL_CHARS)}`;
}

export const make = Effect.gen(function* () {
  const runtime = yield* GraphifyRuntime;
  const processRunner = yield* ProcessRunner;

  const build: GraphifyCli["Service"]["build"] = Effect.fn("GraphifyCli.build")(
    function* (invocation) {
      const resolved = yield* runtime.resolve;
      const args = [...resolved.args, ...graphifyArguments(invocation)];

      const result = yield* processRunner
        .run({
          command: resolved.command,
          args,
          // Run from the repository so relative paths in graphify's own output
          // read the way the user expects. Nothing is written here: the output
          // directory is absolute and outside the repo.
          cwd: invocation.workspaceRoot,
          env: graphifyEnv({ outDir: invocation.outDir }),
          timeout: BUILD_TIMEOUT,
          timeoutBehavior: "timedOutResult",
          maxOutputBytes: BUILD_MAX_OUTPUT_BYTES,
          outputMode: "truncate",
        })
        .pipe(
          Effect.mapError(
            (cause) =>
              new GraphCommandFailedError({
                detail: `graphify could not be run: ${cause.message}`,
                exitCode: null,
              }),
          ),
        );

      if (result.timedOut) {
        return yield* new GraphCommandFailedError({
          detail: `graphify timed out after ${Duration.toMinutes(BUILD_TIMEOUT)} minutes.`,
          exitCode: null,
        });
      }
      if (result.code !== 0) {
        return yield* new GraphCommandFailedError({
          detail: truncateTail(`${result.stdout}\n${result.stderr}`),
          exitCode: result.code,
        });
      }
      return { stdout: result.stdout, stderr: result.stderr };
    },
  );

  return GraphifyCli.of({ build });
});

export const layer = Layer.effect(GraphifyCli, make);
