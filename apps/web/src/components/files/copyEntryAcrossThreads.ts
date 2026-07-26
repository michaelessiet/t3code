/**
 * Cross-thread / cross-workspace copy orchestration.
 *
 * Two strategies, chosen by {@link planCopy}:
 * - Same environment: a single `projects.copyEntry` RPC copies the file or
 *   directory server-side (recursive, binary-safe).
 * - Different environments: the source and destination live on different
 *   connections, so the client reads the source file and writes it to the
 *   destination. This path handles single text files only — directories and
 *   binary files can't be streamed through the text read/write RPCs yet.
 */
import {
  isAtomCommandInterrupted,
  squashAtomCommandFailure,
} from "@t3tools/client-runtime/state/runtime";
import type { EnvironmentId } from "@t3tools/contracts";
import { useCallback } from "react";

import { projectEnvironment } from "~/state/projects";
import { useAtomCommand } from "~/state/use-atom-command";

export interface CopySource {
  readonly environmentId: EnvironmentId;
  readonly cwd: string;
  readonly relativePath: string;
  readonly kind: "file" | "directory";
}

export interface CopyDestination {
  readonly environmentId: EnvironmentId;
  readonly cwd: string;
  readonly relativePath: string;
}

export type CopyPlan =
  | { readonly strategy: "same-environment" }
  | { readonly strategy: "cross-environment-file" }
  | { readonly strategy: "unsupported"; readonly reason: string };

/**
 * Decide how a copy should be performed, or why it can't be. Pure so it can be
 * unit-tested without the atom runtime.
 */
export function planCopy(
  source: Pick<CopySource, "environmentId" | "kind">,
  destinationEnvironmentId: EnvironmentId,
): CopyPlan {
  if (source.environmentId === destinationEnvironmentId) {
    return { strategy: "same-environment" };
  }
  if (source.kind === "directory") {
    return {
      strategy: "unsupported",
      reason: "Copying folders to a thread on another environment isn't supported yet.",
    };
  }
  return { strategy: "cross-environment-file" };
}

export type CopyOutcome =
  | { readonly status: "success"; readonly relativePath: string }
  | { readonly status: "interrupted" }
  | { readonly status: "error"; readonly message: string };

function commandErrorMessage(result: unknown): string {
  const error = squashAtomCommandFailure(result as never);
  return error instanceof Error ? error.message : "An error occurred.";
}

/**
 * Returns an imperative `copy(source, destination)` that performs a cross-thread
 * copy using whichever strategy {@link planCopy} selects.
 */
export function useCopyEntryAcrossThreads(): (
  source: CopySource,
  destination: CopyDestination,
) => Promise<CopyOutcome> {
  const copyEntry = useAtomCommand(projectEnvironment.copyEntry, { reportFailure: false });
  const readFileOnce = useAtomCommand(projectEnvironment.readFileOnce, { reportFailure: false });
  const writeFile = useAtomCommand(projectEnvironment.writeFile, { reportFailure: false });

  return useCallback(
    async (source, destination) => {
      const plan = planCopy(source, destination.environmentId);
      if (plan.strategy === "unsupported") {
        return { status: "error", message: plan.reason };
      }

      if (plan.strategy === "same-environment") {
        const result = await copyEntry({
          environmentId: source.environmentId,
          input: {
            fromCwd: source.cwd,
            fromRelativePath: source.relativePath,
            toCwd: destination.cwd,
            toRelativePath: destination.relativePath,
          },
        });
        if (result._tag === "Success") {
          return { status: "success", relativePath: result.value.relativePath };
        }
        if (isAtomCommandInterrupted(result)) return { status: "interrupted" };
        return { status: "error", message: commandErrorMessage(result) };
      }

      // cross-environment single file: read from source, write to destination.
      const read = await readFileOnce({
        environmentId: source.environmentId,
        input: { cwd: source.cwd, relativePath: source.relativePath },
      });
      if (read._tag !== "Success") {
        if (isAtomCommandInterrupted(read)) return { status: "interrupted" };
        return { status: "error", message: commandErrorMessage(read) };
      }
      if (read.value.truncated) {
        return {
          status: "error",
          message: "File is too large to copy to a thread on another environment.",
        };
      }

      const write = await writeFile({
        environmentId: destination.environmentId,
        input: {
          cwd: destination.cwd,
          relativePath: destination.relativePath,
          contents: read.value.contents,
        },
      });
      if (write._tag === "Success") {
        return { status: "success", relativePath: write.value.relativePath };
      }
      if (isAtomCommandInterrupted(write)) return { status: "interrupted" };
      return { status: "error", message: commandErrorMessage(write) };
    },
    [copyEntry, readFileOnce, writeFile],
  );
}
