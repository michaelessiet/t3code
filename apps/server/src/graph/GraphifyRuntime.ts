/**
 * GraphifyRuntime - detect, report and (on request) install the graphify CLI.
 *
 * graphify is a Python tool; T3 Code is a Node product and does not ship it.
 * This service is the whole story of that gap: it probes for an existing
 * install, tells the settings page exactly what is missing, and installs one
 * only when the user presses a button. Nothing here runs until the
 * `knowledgeGraph.enabled` setting is on — a disabled feature must not spawn
 * subprocesses.
 *
 * The install path mirrors `providerMaintenanceRunner` (bounded subprocess,
 * capped output, serialized behind a lock) and `relayClient` (staged progress
 * reported through a callback so the RPC can stream it).
 *
 * @module GraphifyRuntime
 */
import {
  GraphCommandFailedError,
  GraphDisabledError,
  type GraphInstallEvent,
  GraphRuntimeUnavailableError,
  type GraphRuntimeSource,
  type GraphRuntimeStatus,
  type ServerSettingsError,
} from "@t3tools/contracts";
import { HostProcessPlatform } from "@t3tools/shared/hostProcess";
import * as Context from "effect/Context";
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Ref from "effect/Ref";
import * as Semaphore from "effect/Semaphore";

import { ServerConfig } from "../config.ts";
import { ProcessRunner } from "../processRunner.ts";
import { ServerSettingsService } from "../serverSettings.ts";
import {
  GRAPHIFY_PINNED_VERSION,
  type GraphifyProbe,
  graphifyProbes,
  isSupportedPython,
  parseGraphifyVersion,
  parsePythonVersion,
  pinnedRequirement,
  planGraphifyInstall,
  type PythonVersion,
  uvInstallEnvironment,
  venvExecutable,
} from "./graphifyDetection.ts";

/** Probes must not hang the settings page. */
const PROBE_TIMEOUT = Duration.seconds(10);
/** Creating a venv is fast; installing wheels is not. */
const VENV_TIMEOUT = Duration.minutes(2);
const INSTALL_TIMEOUT = Duration.minutes(10);
/** Probe output is a version string; installer output is a log tail. */
const PROBE_MAX_OUTPUT_BYTES = 64 * 1024;
const INSTALL_MAX_OUTPUT_BYTES = 256 * 1024;
/** How much installer output to carry back to the UI on failure. */
const INSTALL_DETAIL_CHARS = 2000;

/** Interpreters to try, in order, when looking for a usable Python. */
const PYTHON_CANDIDATES = ["python3", "python"] as const;

export interface ResolvedGraphify {
  /** Executable to spawn. */
  readonly command: string;
  /** Prefix arguments that turn `command` into a graphify invocation. */
  readonly args: ReadonlyArray<string>;
  readonly source: GraphRuntimeSource;
  readonly version: string;
}

interface DetectionResult {
  /** Settings values the detection was performed against. */
  readonly fingerprint: string;
  readonly resolved: ResolvedGraphify | null;
  readonly pythonAvailable: boolean;
  readonly detail: string | null;
}

export class GraphifyRuntime extends Context.Service<
  GraphifyRuntime,
  {
    /** Current runtime state, for the settings page. Cached between calls. */
    readonly status: Effect.Effect<GraphRuntimeStatus, ServerSettingsError>;
    /** Resolve an invocable graphify, or explain why there isn't one. */
    readonly resolve: Effect.Effect<
      ResolvedGraphify,
      GraphDisabledError | GraphRuntimeUnavailableError | ServerSettingsError
    >;
    /**
     * Install graphify into a T3-owned location. Always an explicit user
     * action. Progress is reported through `report` so the RPC can stream it.
     */
    readonly installWithProgress: (
      report: (event: GraphInstallEvent) => Effect.Effect<void>,
    ) => Effect.Effect<
      GraphRuntimeStatus,
      | GraphDisabledError
      | GraphRuntimeUnavailableError
      | GraphCommandFailedError
      | ServerSettingsError
    >;
    /** Drop the cached probe result; the next read re-detects. */
    readonly invalidate: Effect.Effect<void>;
  }
>()("t3/graph/GraphifyRuntime") {}

const disabledStatus: GraphRuntimeStatus = {
  state: "disabled",
  source: null,
  interpreterPath: null,
  version: null,
  pythonAvailable: false,
  detail: null,
};

/** Map a probe kind to the source reported to the client. */
function sourceForProbe(probe: GraphifyProbe): GraphRuntimeSource {
  return probe.kind === "managed" ? "managed" : "system";
}

function truncateDetail(text: string): string {
  const trimmed = text.trim();
  return trimmed.length <= INSTALL_DETAIL_CHARS
    ? trimmed
    : `…${trimmed.slice(-INSTALL_DETAIL_CHARS)}`;
}

export const make = Effect.gen(function* () {
  const config = yield* ServerConfig;
  const settingsService = yield* ServerSettingsService;
  const processRunner = yield* ProcessRunner;
  const platform = yield* HostProcessPlatform;

  const cacheRef = yield* Ref.make<DetectionResult | null>(null);
  // A second install must never race a first: both would write the same venv.
  const installLock = yield* Semaphore.make(1);

  /**
   * Run a short command and return its output, or null when it could not be
   * run at all. Probing a command that does not exist is an expected outcome,
   * not an error, so spawn failures collapse to null alongside non-zero exits.
   */
  const probeCommand = Effect.fn("GraphifyRuntime.probeCommand")(function* (
    command: string,
    args: ReadonlyArray<string>,
  ) {
    const result = yield* processRunner
      .run({
        command,
        args,
        timeout: PROBE_TIMEOUT,
        timeoutBehavior: "timedOutResult",
        maxOutputBytes: PROBE_MAX_OUTPUT_BYTES,
        outputMode: "truncate",
      })
      .pipe(Effect.catchCause(() => Effect.succeed(null)));
    if (result === null || result.timedOut || result.code !== 0) return null;
    return `${result.stdout}\n${result.stderr}`;
  });

  /** First interpreter that reports a version, with the version it reported. */
  const detectPython = Effect.fn("GraphifyRuntime.detectPython")(function* () {
    for (const command of PYTHON_CANDIDATES) {
      const output = yield* probeCommand(command, ["--version"]);
      if (output === null) continue;
      const version = parsePythonVersion(output);
      if (version === null) continue;
      return { command, version } satisfies {
        readonly command: string;
        readonly version: PythonVersion;
      };
    }
    return null;
  });

  const uvAvailable = Effect.fn("GraphifyRuntime.uvAvailable")(function* () {
    return (yield* probeCommand("uv", ["--version"])) !== null;
  });

  /**
   * Where uv puts console scripts, straight from uv rather than guessed.
   *
   * `~/.local/bin` is only the default; `UV_TOOL_BIN_DIR` and `XDG_BIN_HOME`
   * both move it, so asking is the only way to be right. Null when uv is not
   * installed, which is not an error — it just means one fewer probe.
   */
  const uvToolBinDir = Effect.fn("GraphifyRuntime.uvToolBinDir")(function* () {
    const output = yield* probeCommand("uv", ["tool", "dir", "--bin"]);
    const trimmed = output?.trim() ?? "";
    return trimmed === "" ? null : (trimmed.split("\n")[0]?.trim() ?? null);
  });

  const runDetection = Effect.fn("GraphifyRuntime.runDetection")(function* (graphifyPath: string) {
    const probes = graphifyProbes({
      graphifyPath,
      graphRuntimeDir: config.graphRuntimeDir,
      uvToolBinDir: yield* uvToolBinDir(),
      platform,
    });

    for (const probe of probes) {
      const output = yield* probeCommand(probe.command, [...probe.args, "--version"]);
      if (output === null) continue;
      const version = parseGraphifyVersion(output);
      if (version === null) continue;
      return {
        fingerprint: graphifyPath,
        resolved: {
          command: probe.command,
          args: probe.args,
          source: sourceForProbe(probe),
          version,
        },
        pythonAvailable: true,
        detail: null,
      } satisfies DetectionResult;
    }

    // No graphify. Work out whether the user needs Python too, so the settings
    // page can distinguish "install graphify" from "install Python first".
    const python = yield* detectPython();
    const pythonAvailable = python !== null && isSupportedPython(python.version);
    const detail =
      graphifyPath.trim() !== ""
        ? `'${graphifyPath.trim()}' did not respond to --version.`
        : pythonAvailable
          ? "graphify is not installed."
          : python === null
            ? "No Python interpreter was found."
            : `Found Python ${python.version.major}.${python.version.minor}, which is too old for graphify.`;

    return {
      fingerprint: graphifyPath,
      resolved: null,
      pythonAvailable,
      detail,
    } satisfies DetectionResult;
  });

  /** Cached detection. Re-probes when the configured path changes. */
  const detect = Effect.fn("GraphifyRuntime.detect")(function* (graphifyPath: string) {
    const cached = yield* Ref.get(cacheRef);
    if (cached !== null && cached.fingerprint === graphifyPath) return cached;
    const fresh = yield* runDetection(graphifyPath);
    yield* Ref.set(cacheRef, fresh);
    return fresh;
  });

  const statusFrom = (detection: DetectionResult): GraphRuntimeStatus =>
    detection.resolved === null
      ? {
          state: "missing",
          source: null,
          interpreterPath: null,
          version: null,
          pythonAvailable: detection.pythonAvailable,
          detail: detection.detail,
        }
      : {
          state: "ready",
          source: detection.resolved.source,
          interpreterPath: detection.resolved.command,
          version: detection.resolved.version,
          pythonAvailable: true,
          detail:
            detection.resolved.version === GRAPHIFY_PINNED_VERSION
              ? null
              : `Installed version ${detection.resolved.version} differs from the pinned ${GRAPHIFY_PINNED_VERSION}. Stored graphs built by another version are rebuilt on first use.`,
        };

  const status: GraphifyRuntime["Service"]["status"] = Effect.gen(function* () {
    const settings = yield* settingsService.getSettings;
    if (!settings.knowledgeGraph.enabled) return disabledStatus;
    return statusFrom(yield* detect(settings.knowledgeGraph.graphifyPath));
  });

  const resolve: GraphifyRuntime["Service"]["resolve"] = Effect.gen(function* () {
    const settings = yield* settingsService.getSettings;
    if (!settings.knowledgeGraph.enabled) return yield* new GraphDisabledError();
    const detection = yield* detect(settings.knowledgeGraph.graphifyPath);
    if (detection.resolved === null) {
      return yield* new GraphRuntimeUnavailableError({
        detail: detection.detail ?? "graphify is not installed.",
      });
    }
    return detection.resolved;
  });

  const invalidate = Ref.set(cacheRef, null);

  /**
   * Run one step of an install. Unlike `probeCommand`, failures here are real:
   * the user asked for this and needs to see why it did not work.
   */
  const runInstallStep = Effect.fn("GraphifyRuntime.runInstallStep")(function* (input: {
    readonly command: string;
    readonly args: ReadonlyArray<string>;
    readonly timeout: Duration.Duration;
    /** Layered onto the inherited environment; ProcessRunner sets extendEnv. */
    readonly env?: Record<string, string>;
  }) {
    const result = yield* processRunner
      .run({
        command: input.command,
        args: input.args,
        timeout: input.timeout,
        timeoutBehavior: "timedOutResult",
        maxOutputBytes: INSTALL_MAX_OUTPUT_BYTES,
        outputMode: "truncate",
        ...(input.env === undefined ? {} : { env: input.env }),
      })
      .pipe(
        Effect.mapError(
          (cause) =>
            new GraphCommandFailedError({
              detail: `${input.command} could not be run: ${cause.message}`,
              exitCode: null,
            }),
        ),
      );

    if (result.timedOut) {
      return yield* new GraphCommandFailedError({
        detail: `${input.command} timed out after ${Duration.toMillis(input.timeout) / 1000}s.`,
        exitCode: null,
      });
    }
    if (result.code !== 0) {
      return yield* new GraphCommandFailedError({
        detail: truncateDetail(`${result.stdout}\n${result.stderr}`),
        exitCode: result.code,
      });
    }
    return truncateDetail(`${result.stdout}\n${result.stderr}`);
  });

  const installWithProgress: GraphifyRuntime["Service"]["installWithProgress"] = (report) =>
    Effect.gen(function* () {
      const settings = yield* settingsService.getSettings;
      if (!settings.knowledgeGraph.enabled) return yield* new GraphDisabledError();

      yield* report({ type: "progress", stage: "waiting_for_lock", detail: null });

      return yield* installLock.withPermits(1)(
        Effect.gen(function* () {
          yield* report({ type: "progress", stage: "checking", detail: null });

          const [uv, python] = yield* Effect.all([uvAvailable(), detectPython()], {
            concurrency: "unbounded",
          });
          const plan = planGraphifyInstall({ uvAvailable: uv, python });

          if (plan.kind === "impossible") {
            return yield* new GraphRuntimeUnavailableError({ detail: plan.detail });
          }

          if (plan.kind === "uv") {
            yield* report({ type: "progress", stage: "installing", detail: null });
            const output = yield* runInstallStep({
              command: "uv",
              args: ["tool", "install", "--force", pinnedRequirement()],
              timeout: INSTALL_TIMEOUT,
              env: uvInstallEnvironment(config.graphRuntimeDir, platform),
            });
            yield* report({ type: "progress", stage: "installing", detail: output });
          } else {
            yield* report({ type: "progress", stage: "creating_venv", detail: null });
            yield* runInstallStep({
              command: plan.python,
              args: ["-m", "venv", config.graphRuntimeDir],
              timeout: VENV_TIMEOUT,
            });

            yield* report({ type: "progress", stage: "installing", detail: null });
            const output = yield* runInstallStep({
              command: venvExecutable(config.graphRuntimeDir, "python", platform),
              args: ["-m", "pip", "install", "--disable-pip-version-check", pinnedRequirement()],
              timeout: INSTALL_TIMEOUT,
            });
            yield* report({ type: "progress", stage: "installing", detail: output });
          }

          yield* report({ type: "progress", stage: "validating", detail: null });
          yield* invalidate;
          const detection = yield* detect(settings.knowledgeGraph.graphifyPath);
          if (detection.resolved === null) {
            // Deliberately not `detection.detail` here: that says "graphify is
            // not installed", which contradicts the sentence before it and
            // sends the user looking for the wrong problem. The install
            // reported success, so what failed is the lookup — say where T3
            // looked instead.
            return yield* new GraphRuntimeUnavailableError({
              detail:
                "graphify was installed but could not be run afterwards. T3 Code looked in " +
                `${venvExecutable(config.graphRuntimeDir, "graphify", platform)}, on PATH, ` +
                "and in uv's tool directory. Set an explicit graphify path in settings if it " +
                "landed somewhere else.",
            });
          }

          const finalStatus = statusFrom(detection);
          yield* report({ type: "complete", runtime: finalStatus });
          return finalStatus;
        }),
      );
    });

  return GraphifyRuntime.of({ status, resolve, installWithProgress, invalidate });
});

export const layer = Layer.effect(GraphifyRuntime, make);
