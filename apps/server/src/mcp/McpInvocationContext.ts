import {
  type EnvironmentId,
  GraphCapabilityUnavailableError,
  PreviewAutomationUnavailableError,
  type ProviderInstanceId,
  type ThreadId,
} from "@t3tools/contracts";
import * as Context from "effect/Context";
import * as Effect from "effect/Effect";

export type McpCapability = "preview" | "graph";

export interface McpInvocationScope {
  readonly environmentId: EnvironmentId;
  readonly threadId: ThreadId;
  readonly providerSessionId: string;
  readonly providerInstanceId: ProviderInstanceId;
  readonly capabilities: ReadonlySet<McpCapability>;
  readonly issuedAt: number;
  readonly expiresAt: number;
}

export class McpInvocationContext extends Context.Service<
  McpInvocationContext,
  McpInvocationScope
>()("t3/mcp/McpInvocationContext") {}

export const requireMcpCapability = Effect.fn("mcp.requireCapability")(function* (
  capability: "preview",
) {
  const invocation = yield* McpInvocationContext;
  if (!invocation.capabilities.has(capability)) {
    return yield* new PreviewAutomationUnavailableError({
      capability,
      environmentId: invocation.environmentId,
      threadId: invocation.threadId,
      providerSessionId: invocation.providerSessionId,
      providerInstanceId: invocation.providerInstanceId,
    });
  }
  return invocation;
});

/**
 * The same check for the graph toolkit, with its own error.
 *
 * It does not share `requireMcpCapability` because that one raises
 * `PreviewAutomationUnavailableError`, whose tag is what
 * `PreviewAutomationBroker` switches on and what preview clients already
 * handle. Widening it to cover graph would send agents a preview-shaped
 * failure for a graph call; duplicating one `if` is the cheaper trade.
 */
export const requireGraphCapability = Effect.fn("mcp.requireGraphCapability")(function* () {
  const invocation = yield* McpInvocationContext;
  if (!invocation.capabilities.has("graph")) {
    return yield* new GraphCapabilityUnavailableError();
  }
  return invocation;
});
