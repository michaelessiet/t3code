/**
 * Handlers for the knowledge-graph MCP toolkit.
 *
 * Each one does the same three things: check the session's capability, call
 * `GraphService`, and narrow the error channel to what the tool's failure
 * schema declares. `GraphService` re-reads `knowledgeGraph.enabled` on every
 * call, so the capability check is a clearer message rather than the security
 * boundary — see the note in `tools.ts`.
 *
 * @module graph/handlers
 */
import { GraphDisabledError } from "@t3tools/contracts";
import * as Effect from "effect/Effect";

import { GraphService } from "../../../graph/GraphService.ts";
import * as McpInvocationContext from "../../McpInvocationContext.ts";
import { GraphToolkit } from "./tools.ts";

/** Gate, then hand back the service. */
const enter = Effect.fn("GraphToolkit.enter")(function* () {
  yield* McpInvocationContext.requireGraphCapability();
  return yield* GraphService;
});

/**
 * A settings read that failed is reported as "the feature is off".
 *
 * The alternative is surfacing `ServerSettingsError` — a defect and a path on
 * disk — to a language model, which cannot act on either. Off is also the
 * truthful answer: the feature defaults to disabled and T3 has just failed to
 * find evidence otherwise.
 */
const asDisabled = () => new GraphDisabledError();

const handlers = {
  graph_status: (input) =>
    enter().pipe(
      Effect.flatMap((service) => service.status(input.cwd)),
      Effect.catchTag("ServerSettingsError", asDisabled),
    ),
  graph_query: (input) =>
    enter().pipe(
      Effect.flatMap((service) => service.search(input)),
      Effect.catchTag("ServerSettingsError", asDisabled),
    ),
  graph_explain: (input) =>
    enter().pipe(
      Effect.flatMap((service) => service.explain(input)),
      Effect.catchTag("ServerSettingsError", asDisabled),
    ),
  graph_path: (input) =>
    enter().pipe(
      Effect.flatMap((service) => service.path(input)),
      Effect.catchTag("ServerSettingsError", asDisabled),
    ),
  graph_neighbors: (input) =>
    enter().pipe(
      Effect.flatMap((service) => service.subgraph(input)),
      Effect.catchTag("ServerSettingsError", asDisabled),
    ),
  graph_god_nodes: (input) =>
    enter().pipe(
      Effect.flatMap((service) => service.snapshot(input.cwd)),
      Effect.catchTag("ServerSettingsError", asDisabled),
    ),
} satisfies Parameters<typeof GraphToolkit.toLayer>[0];

export const GraphToolkitHandlersLive = GraphToolkit.toLayer(handlers);
