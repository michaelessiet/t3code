import * as Data from "effect/Data";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import * as Stream from "effect/Stream";
import { WS_METHODS, type EnvironmentId } from "@t3tools/contracts";
import { EnvironmentRegistry } from "@t3tools/client-runtime/connection";
import { request, runStream } from "@t3tools/client-runtime/rpc";

import {
  finishClaudeBinaryInstall,
  reportClaudeBinaryInstallProgress,
  requestClaudeBinaryInstallConfirmation,
} from "./claudeBinaryInstallDialog";

export class ClaudeBinaryInstallFlowError extends Data.TaggedError("ClaudeBinaryInstallFlowError")<{
  readonly message: string;
  readonly cause?: unknown;
}> {}

const claudeBinaryRpcError = (message: string) => (cause: unknown) =>
  new ClaudeBinaryInstallFlowError({
    message,
    cause,
  });

/**
 * Ensure the environment can spawn Claude Code: check the server-side binary
 * status and, when missing, confirm with the user and stream the managed
 * download. Mirrors `ensureRelayClientAvailable` for the relay client.
 */
export function ensureClaudeBinaryAvailable(
  environmentId: EnvironmentId,
): Effect.Effect<void, ClaudeBinaryInstallFlowError, EnvironmentRegistry> {
  return Effect.gen(function* () {
    const registry = yield* EnvironmentRegistry;
    const status = yield* registry
      .run(environmentId, request(WS_METHODS.claudeGetBinaryStatus, {}))
      .pipe(Effect.mapError(claudeBinaryRpcError("Could not check Claude Code availability.")));
    if (status.status === "available") return;
    if (status.status === "unsupported") {
      return yield* new ClaudeBinaryInstallFlowError({
        message: `T3 Code cannot install Claude Code automatically on ${status.platform}-${status.arch}.`,
      });
    }

    const confirmed = yield* Effect.tryPromise({
      try: () =>
        requestClaudeBinaryInstallConfirmation({
          version: status.version,
          binarySizeBytes: status.binarySizeBytes,
        }),
      catch: claudeBinaryRpcError("Could not confirm Claude Code installation."),
    });
    if (!confirmed) {
      return yield* new ClaudeBinaryInstallFlowError({
        message: "Claude Code installation was cancelled.",
      });
    }

    const installed = yield* registry
      .runStream(
        environmentId,
        runStream(WS_METHODS.claudeInstallBinary, {}).pipe(
          Stream.tap((event) => Effect.sync(() => reportClaudeBinaryInstallProgress(event))),
        ),
      )
      .pipe(
        Stream.runLast,
        Effect.mapError(claudeBinaryRpcError("Could not install Claude Code.")),
        Effect.ensuring(Effect.sync(finishClaudeBinaryInstall)),
      );
    if (Option.isNone(installed) || installed.value.type !== "complete") {
      return yield* new ClaudeBinaryInstallFlowError({
        message: "The Claude Code install completed without a final status.",
      });
    }
    const installedStatus = installed.value.status;
    if (installedStatus.status !== "available") {
      return yield* new ClaudeBinaryInstallFlowError({
        message:
          installedStatus.status === "unsupported"
            ? `T3 Code cannot install Claude Code automatically on ${installedStatus.platform}-${installedStatus.arch}.`
            : "Claude Code is still unavailable after installation.",
      });
    }
  });
}
