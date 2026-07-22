import { type LspDiagnostic, WS_METHODS } from "@t3tools/contracts";
import * as Stream from "effect/Stream";
import type * as Crypto from "effect/Crypto";
import { Atom } from "effect/unstable/reactivity";

import {
  createEnvironmentRpcCommand,
  createEnvironmentRpcQueryAtomFamily,
  createEnvironmentRpcSubscriptionAtomFamily,
} from "./runtime.ts";
import type { EnvironmentRegistry } from "../connection/registry.ts";

/** Per-file diagnostic sets for one workspace, folded from the LSP stream. */
export type LspDiagnosticsSnapshot = ReadonlyMap<string, ReadonlyArray<LspDiagnostic>>;

/**
 * Language-intelligence atoms. Request commands are deliberately
 * scheduler-less: completion/hover latency is user-visible and the server
 * already serializes per language server; queueing behind unrelated commands
 * would only add lag.
 */
export function createLspEnvironmentAtoms<R, E>(
  runtime: Atom.AtomRuntime<EnvironmentRegistry | Crypto.Crypto | R, E>,
) {
  return {
    didOpen: createEnvironmentRpcCommand(runtime, {
      label: "environment-data:lsp:did-open",
      tag: WS_METHODS.lspDidOpen,
    }),
    didChange: createEnvironmentRpcCommand(runtime, {
      label: "environment-data:lsp:did-change",
      tag: WS_METHODS.lspDidChange,
    }),
    didClose: createEnvironmentRpcCommand(runtime, {
      label: "environment-data:lsp:did-close",
      tag: WS_METHODS.lspDidClose,
    }),
    completion: createEnvironmentRpcCommand(runtime, {
      label: "environment-data:lsp:completion",
      tag: WS_METHODS.lspCompletion,
    }),
    resolveCompletion: createEnvironmentRpcCommand(runtime, {
      label: "environment-data:lsp:resolve-completion",
      tag: WS_METHODS.lspResolveCompletion,
    }),
    signatureHelp: createEnvironmentRpcCommand(runtime, {
      label: "environment-data:lsp:signature-help",
      tag: WS_METHODS.lspSignatureHelp,
    }),
    hover: createEnvironmentRpcCommand(runtime, {
      label: "environment-data:lsp:hover",
      tag: WS_METHODS.lspHover,
    }),
    definition: createEnvironmentRpcCommand(runtime, {
      label: "environment-data:lsp:definition",
      tag: WS_METHODS.lspDefinition,
    }),
    references: createEnvironmentRpcCommand(runtime, {
      label: "environment-data:lsp:references",
      tag: WS_METHODS.lspReferences,
    }),
    rename: createEnvironmentRpcCommand(runtime, {
      label: "environment-data:lsp:rename",
      tag: WS_METHODS.lspRename,
    }),
    format: createEnvironmentRpcCommand(runtime, {
      label: "environment-data:lsp:format",
      tag: WS_METHODS.lspFormat,
    }),
    serverStatus: createEnvironmentRpcQueryAtomFamily(runtime, {
      label: "environment-data:lsp:server-status",
      tag: WS_METHODS.lspServerStatus,
      staleTimeMs: 5_000,
      idleTtlMs: 60_000,
    }),
    diagnostics: createEnvironmentRpcSubscriptionAtomFamily(runtime, {
      label: "environment-data:lsp:diagnostics",
      tag: WS_METHODS.subscribeLspDiagnostics,
      transform: (stream) =>
        stream.pipe(
          Stream.mapAccum(
            () => new Map<string, ReadonlyArray<LspDiagnostic>>(),
            (current, event) => {
              const next = new Map(current);
              if (event.diagnostics.length === 0) {
                next.delete(event.relativePath);
              } else {
                next.set(event.relativePath, event.diagnostics);
              }
              return [next, [next as LspDiagnosticsSnapshot]] as const;
            },
          ),
        ),
    }),
  };
}
