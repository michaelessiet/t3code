/**
 * The knowledge-graph MCP toolkit — what agents see.
 *
 * Every tool is a read: `Tool.Readonly`, `Tool.Idempotent`, and explicitly not
 * destructive. Nothing here can build a graph, because a build spawns a Python
 * process over the whole workspace and that is a decision for the person at the
 * keyboard, not for an agent that noticed the graph was stale.
 *
 * **Registration is static; the gate is at invoke time.** There is one MCP HTTP
 * server for the whole app and `McpServer.toolkit` registers into it at layer
 * construction, so these tools are advertised to every session regardless of the
 * setting. The plan asked for them to be advertised only when the feature is on;
 * that is not expressible without a per-session tool list, so the honest version
 * is a capability check in the handler plus a description that says what happens
 * when the graph is off. An agent that calls one anyway gets a specific error
 * telling it to stop, which is the outcome hiding the tool was meant to buy.
 *
 * ## Descriptions are routing rules, not summaries
 *
 * An agent's default for any question about a codebase is grep, and it is a good
 * default: always current, costs nothing to maintain. A description that only
 * says what a tool returns never displaces it, because the agent has no way to
 * recognise the questions where a graph wins. So each one below leads with those
 * question shapes, names the cases that should stay with grep, and says what to
 * do when the call fails — a miss should cost one call, not a retry loop.
 *
 * For the same reason nothing here tells the agent to call `graph_status` first.
 * A mandatory preflight put every graph answer two round-trips behind grep's
 * one, which is a sound reason never to start; the read tools each fail with a
 * specific error instead, so the preflight buys nothing it does not already get.
 *
 * @module graph/tools
 */
import {
  GraphCapabilityUnavailableError,
  GraphDisabledError,
  GraphExplanation,
  GraphNodeNotFoundError,
  GraphNodeQueryInput,
  GraphNotBuiltError,
  GraphPathInput,
  GraphPathResult,
  GraphQueryInput,
  GraphSearchResult,
  GraphSnapshot,
  GraphStatus,
  GraphStorePathError,
  GraphSubgraph,
  GraphSubgraphInput,
  GraphWorkspaceInput,
  GraphWorkspaceUnknownError,
} from "@t3tools/contracts";
import * as Schema from "effect/Schema";
import { Tool, Toolkit } from "effect/unstable/ai";

import { GraphService } from "../../../graph/GraphService.ts";
import * as McpInvocationContext from "../../McpInvocationContext.ts";

const dependencies = [McpInvocationContext.McpInvocationContext, GraphService];

/**
 * Every way a graph read can fail, as one union.
 *
 * `ServerSettingsError` is deliberately absent: it carries a `Defect` field and
 * a settings path, neither of which means anything to an agent. Handlers map it
 * to `GraphDisabledError` — if T3 cannot read its own settings it cannot claim
 * the feature is on.
 */
const GraphToolError = Schema.Union([
  GraphCapabilityUnavailableError,
  GraphDisabledError,
  GraphWorkspaceUnknownError,
  GraphNotBuiltError,
  GraphNodeNotFoundError,
  GraphStorePathError,
]);

const readTool = <T extends Tool.Any>(tool: T, title: string): T =>
  tool
    .annotate(Tool.Title, title)
    .annotate(Tool.Readonly, true)
    .annotate(Tool.Destructive, false)
    .annotate(Tool.Idempotent, true) as T;

export const GraphStatusTool = readTool(
  Tool.make("graph_status", {
    description:
      "Report whether a knowledge graph exists for a workspace and how current it is: the branch it was built from, the graphify runtime state, any in-flight build, and a community-level summary with node and edge counts. You do not need to call this before the other graph tools — they each fail with a specific error when there is no graph. Use it when the trustworthiness of the graph is itself the question, above all whether it has gone stale against the working tree.",
    parameters: GraphWorkspaceInput,
    success: GraphStatus,
    failure: GraphToolError,
    dependencies,
  }),
  "Get knowledge graph status",
);

export const GraphQueryTool = readTool(
  Tool.make("graph_query", {
    description:
      "Answer 'what connects to what' questions about this codebase without reading files. Use it when the question is about relationships, dependencies, blast radius, or how two parts interact — what uses X, what would break if Y changed, where a feature actually lives. Matches node ids and labels as a substring, best match first, and returns the exact node ids that graph_explain and graph_path take, plus totalMatches so you can tell a complete answer from a truncated one. Do not use it to find a literal string, locate a file by name, or read an implementation: grep and file reads are faster there and are always current, whereas this graph is only as fresh as its last build. Fails with GraphNotBuiltError when no graph exists and GraphDisabledError when the feature is off; in both cases fall back to ordinary file search rather than retrying.",
    parameters: GraphQueryInput,
    success: GraphSearchResult,
    failure: GraphToolError,
    dependencies,
  }),
  "Search the knowledge graph",
);

export const GraphExplainTool = readTool(
  Tool.make("graph_explain", {
    description:
      "Describe one node: its source file and line, its community, and what it is connected to, strongest link first. Use it after graph_query to turn a match into its actual dependencies, or to jump straight to where something is defined. Each neighbour carries a confidence — EXTRACTED means graphify read it out of the code; INFERRED and AMBIGUOUS are guesses, so check them against the source before reporting them as fact. Read the file itself when you need the implementation rather than the relationships. Fails with GraphNotBuiltError when no graph exists; fall back to file reads rather than retrying.",
    parameters: GraphNodeQueryInput,
    success: GraphExplanation,
    failure: GraphToolError,
    dependencies,
  }),
  "Explain a knowledge graph node",
);

export const GraphPathTool = readTool(
  Tool.make("graph_path", {
    description:
      "Find the shortest chain of relationships between two nodes — how one part of the codebase reaches another. Use it for 'how does A end up reaching B' and for tracing an unexpected coupling; this is the question grep cannot answer at all, because the chain runs through files that mention neither endpoint. Takes node ids from graph_query. Returns empty nodes when nothing connects them, which is an answer, not a failure. Fails with GraphNotBuiltError when no graph exists.",
    parameters: GraphPathInput,
    success: GraphPathResult,
    failure: GraphToolError,
    dependencies,
  }),
  "Find a path between graph nodes",
);

export const GraphNeighborsTool = readTool(
  Tool.make("graph_neighbors", {
    description:
      "Return a bounded neighbourhood around a node, or a whole community, as nodes plus edges. Use it when you need the shape of a region — a subsystem's internal structure, or everything within a couple of hops of a planned change — rather than one node's links. Depth 1-2 and a limit you can afford to read; the graph is tens of thousands of nodes, so this never returns all of it and sets truncated when it clips. Prefer graph_explain for a single node, and ordinary directory listing for 'what files are in here', which is a question about layout rather than dependencies. Fails with GraphNotBuiltError when no graph exists.",
    parameters: GraphSubgraphInput,
    success: GraphSubgraph,
    failure: GraphToolError,
    dependencies,
  }),
  "Expand a knowledge graph neighbourhood",
);

export const GraphGodNodesTool = readTool(
  Tool.make("graph_god_nodes", {
    description:
      "Return the workspace's structural hubs — the most-connected nodes — together with the community breakdown. Use it to orient before starting work in an unfamiliar codebase, in place of grepping around for an entry point: these are the things most of the code depends on, so they are where a change is most likely to have consequences. It has little to offer once you already know which part of the code you are in. Same payload as graph_status's snapshot, without the runtime and build detail. Fails with GraphNotBuiltError when no graph exists.",
    parameters: GraphWorkspaceInput,
    success: GraphSnapshot,
    failure: GraphToolError,
    dependencies,
  }),
  "List knowledge graph hubs",
);

export const GraphToolkit = Toolkit.make(
  GraphStatusTool,
  GraphQueryTool,
  GraphExplainTool,
  GraphPathTool,
  GraphNeighborsTool,
  GraphGodNodesTool,
);
