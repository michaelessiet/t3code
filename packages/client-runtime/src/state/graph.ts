import {
  type GraphBuildMode,
  type GraphInstallEvent,
  type GraphRuntimeStatus,
  WS_METHODS,
} from "@t3tools/contracts";
import * as Effect from "effect/Effect";
import * as Stream from "effect/Stream";
import type * as Crypto from "effect/Crypto";
import { Atom } from "effect/unstable/reactivity";

import { request, runStream } from "../rpc/client.ts";
import { createEnvironmentCommand, createEnvironmentRpcQueryAtomFamily } from "./runtime.ts";
import type { EnvironmentRegistry } from "../connection/registry.ts";

/**
 * Knowledge-graph atoms.
 *
 * The runtime status is a query: it is cheap, returns `disabled` without
 * probing when the feature is off, and the settings page re-reads it after an
 * install. It carries no `refreshIntervalMs` because the toolchain only
 * changes when the user changes it.
 */
export function createGraphEnvironmentAtoms<R, E>(
  runtime: Atom.AtomRuntime<EnvironmentRegistry | Crypto.Crypto | R, E>,
) {
  return {
    runtimeStatus: createEnvironmentRpcQueryAtomFamily(runtime, {
      label: "environment-data:graph:runtime-status",
      tag: WS_METHODS.graphRuntimeStatus,
      staleTimeMs: 5_000,
      idleTtlMs: 60_000,
    }),
    /**
     * Everything the panel needs in one read: runtime state, build state,
     * branch, and the aggregate snapshot when a graph exists.
     *
     * Polled rather than subscribed. A build takes minutes and reports coarse
     * stages, so a few seconds of latency on the progress line costs nothing,
     * and a poll cannot leak a subscription per closed panel. The server stamps
     * `lastOpenedAt` on every one of these, which is also what keeps a graph in
     * active use out of the retention sweep.
     */
    status: createEnvironmentRpcQueryAtomFamily(runtime, {
      label: "environment-data:graph:status",
      tag: WS_METHODS.graphStatus,
      staleTimeMs: 2_000,
      idleTtlMs: 60_000,
      refreshIntervalMs: 5_000,
    }),
    /** Bounded neighbourhood around one node or community, for expansion. */
    subgraph: createEnvironmentRpcQueryAtomFamily(runtime, {
      label: "environment-data:graph:subgraph",
      tag: WS_METHODS.graphSubgraph,
      staleTimeMs: 30_000,
      idleTtlMs: 120_000,
    }),
    /**
     * Queues a build. Returns as soon as it is queued, not when it finishes —
     * progress arrives through the status poll above.
     */
    build: createEnvironmentCommand(runtime, {
      label: "environment-data:graph:build",
      execute: (input: {
        readonly cwd: string;
        readonly mode: GraphBuildMode;
        readonly force: boolean;
      }) => request(WS_METHODS.graphBuild, input),
    }),
    /**
     * Installs graphify, forwarding each stage to `onEvent` as it arrives.
     *
     * Modelled as a unary command over a streaming RPC on purpose: the generic
     * stream-command helper only settles on the final value, and an installer
     * that reports nothing for several minutes is exactly the dead spinner this
     * RPC streams to avoid. The command resolves to the final runtime status.
     */
    installRuntime: createEnvironmentCommand(runtime, {
      label: "environment-data:graph:install-runtime",
      execute: (input: {
        readonly interpreterPath: string | null;
        readonly onEvent: (event: GraphInstallEvent) => void;
      }) =>
        runStream(WS_METHODS.graphInstallRuntime, {
          interpreterPath: input.interpreterPath,
        }).pipe(
          Stream.runForEach((event) => Effect.sync(() => input.onEvent(event))),
          Effect.andThen(request(WS_METHODS.graphRuntimeStatus, {})),
        ),
    }),
  };
}

export type GraphRuntimeStatusValue = GraphRuntimeStatus;
