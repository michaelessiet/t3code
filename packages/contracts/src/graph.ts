/**
 * Knowledge-graph contracts.
 *
 * The graph feature wraps the external `graphify` tool, which turns a
 * workspace into a queryable knowledge graph. The feature is opt-in and
 * disabled by default: it depends on a Python runtime that T3 Code does not
 * ship, so every schema here is designed to describe a *possibly absent*
 * capability without the rest of the app having to care.
 *
 * Payload sizing is the main constraint. A real graph is tens of thousands of
 * nodes, so the wire format never carries the whole thing:
 *
 * - `GraphSnapshot` is a community-level aggregate (counts, labels, hubs).
 * - `GraphSubgraph` is an explicitly bounded neighbourhood, fetched on demand.
 *
 * @module graph
 */
import * as Schema from "effect/Schema";

import { NonNegativeInt, PositiveInt, ProjectId, TrimmedNonEmptyString } from "./baseSchemas.ts";

export const GRAPH_QUERY_MAX_LENGTH = 512;
export const GRAPH_SUBGRAPH_MAX_NODES = 1_500;
export const GRAPH_SUBGRAPH_MAX_DEPTH = 4;
/** Search and explain are read by agents in a context window, so keep them small. */
export const GRAPH_SEARCH_MAX_RESULTS = 100;
export const GRAPH_EXPLAIN_MAX_NEIGHBORS = 50;

/** Node identifier as produced by graphify (lowercase `[a-z0-9_]`). */
export const GraphNodeId = TrimmedNonEmptyString.check(Schema.isMaxLength(256));
export type GraphNodeId = typeof GraphNodeId.Type;

/**
 * Provenance of an edge. Surfaced verbatim in the UI and to agents: an
 * INFERRED edge is a guess and must never be presented as fact.
 */
export const GraphConfidence = Schema.Literals(["EXTRACTED", "INFERRED", "AMBIGUOUS"]);
export type GraphConfidence = typeof GraphConfidence.Type;

/**
 * graphify's `file_type` node attribute.
 *
 * `unknown` is not one of graphify's own values — it is where the decoder puts
 * anything it does not recognise. graphify is a fast-moving 0.x and adds node
 * kinds between releases (`doc_ref` appeared this way), so a closed union that
 * *fails* on a new value would take the whole panel down over one node.
 */
export const GraphFileType = Schema.Literals([
  "code",
  "document",
  "paper",
  "image",
  "rationale",
  "concept",
  "doc_ref",
  "unknown",
]);
export type GraphFileType = typeof GraphFileType.Type;

export const GraphNode = Schema.Struct({
  id: GraphNodeId,
  label: Schema.String,
  fileType: GraphFileType,
  /** Workspace-relative path, when the node came from a file. */
  sourceFile: Schema.NullOr(Schema.String),
  /** 1-based line within `sourceFile`, when graphify recorded one. */
  sourceLine: Schema.NullOr(PositiveInt),
  communityId: Schema.NullOr(NonNegativeInt),
  /** Degree in the full graph, not within the returned subgraph. */
  degree: NonNegativeInt,
});
export type GraphNode = typeof GraphNode.Type;

export const GraphEdge = Schema.Struct({
  source: GraphNodeId,
  target: GraphNodeId,
  relation: Schema.String,
  confidence: GraphConfidence,
  confidenceScore: Schema.Number,
});
export type GraphEdge = typeof GraphEdge.Type;

export const GraphCommunity = Schema.Struct({
  id: NonNegativeInt,
  label: Schema.String,
  nodeCount: NonNegativeInt,
  /** graphify's cohesion score, surfaced raw rather than as a symbol. */
  cohesion: Schema.Number,
});
export type GraphCommunity = typeof GraphCommunity.Type;

/**
 * Community-level view of the graph. Safe to send eagerly: size is bounded by
 * the community count, not the node count.
 */
export const GraphSnapshot = Schema.Struct({
  nodeCount: NonNegativeInt,
  edgeCount: NonNegativeInt,
  communities: Schema.Array(GraphCommunity),
  /** Highest-centrality nodes — the graph's structural hubs. */
  godNodes: Schema.Array(GraphNode),
  /** Epoch millis of the graph build this snapshot describes. */
  builtAt: Schema.Number,
  /**
   * True when the workspace changed after `builtAt`. A stale graph is worse
   * than no graph if presented as current, so this is never inferred
   * client-side.
   */
  stale: Schema.Boolean,
});
export type GraphSnapshot = typeof GraphSnapshot.Type;

export const GraphSubgraph = Schema.Struct({
  nodes: Schema.Array(GraphNode),
  edges: Schema.Array(GraphEdge),
  /** True when the neighbourhood was clipped by `limit`. */
  truncated: Schema.Boolean,
});
export type GraphSubgraph = typeof GraphSubgraph.Type;

// -- Runtime -----------------------------------------------------------------

/**
 * Where the graphify runtime comes from.
 *
 * - `system`  — resolved from PATH / an interpreter the user already has.
 * - `managed` — a venv T3 Code created under its own data directory.
 */
export const GraphRuntimeSource = Schema.Literals(["system", "managed"]);
export type GraphRuntimeSource = typeof GraphRuntimeSource.Type;

export const GraphRuntimeState = Schema.Literals([
  /** Feature switched off in settings — nothing has been probed. */
  "disabled",
  /** Enabled, but no usable interpreter or graphify install was found. */
  "missing",
  /** An install is running right now. */
  "installing",
  "ready",
  "failed",
]);
export type GraphRuntimeState = typeof GraphRuntimeState.Type;

export const GraphRuntimeStatus = Schema.Struct({
  state: GraphRuntimeState,
  source: Schema.NullOr(GraphRuntimeSource),
  /** Absolute interpreter path once resolved. */
  interpreterPath: Schema.NullOr(Schema.String),
  /** Installed graphify version, when known. */
  version: Schema.NullOr(Schema.String),
  /** True when a Python interpreter exists but graphify is not installed. */
  pythonAvailable: Schema.Boolean,
  /** Human-readable reason for `missing` / `failed`. */
  detail: Schema.NullOr(Schema.String),
});
export type GraphRuntimeStatus = typeof GraphRuntimeStatus.Type;

/**
 * Stages of a runtime install, streamed so a multi-minute `pip install` shows
 * progress instead of an unresolving spinner. Mirrors the relay-client
 * installer's staged events (`RelayClientInstallProgressEventSchema`).
 */
export const GraphInstallStage = Schema.Literals([
  "checking",
  "waiting_for_lock",
  "creating_venv",
  "installing",
  "validating",
]);
export type GraphInstallStage = typeof GraphInstallStage.Type;

export const GraphInstallEvent = Schema.Union([
  Schema.Struct({
    type: Schema.Literal("progress"),
    stage: GraphInstallStage,
    /** Trailing installer output, for the dialog's log view. */
    detail: Schema.NullOr(Schema.String),
  }),
  Schema.Struct({
    type: Schema.Literal("complete"),
    runtime: GraphRuntimeStatus,
  }),
]);
export type GraphInstallEvent = typeof GraphInstallEvent.Type;

// -- Builds ------------------------------------------------------------------

/**
 * `structural` runs graphify's AST pass only: deterministic, free, and safe to
 * re-run on every change. `semantic` additionally runs LLM extraction, which
 * costs tokens and is therefore always explicitly requested.
 */
export const GraphBuildMode = Schema.Literals(["structural", "semantic"]);
export type GraphBuildMode = typeof GraphBuildMode.Type;

export const GraphBuildState = Schema.Literals([
  "idle",
  "queued",
  "running",
  "succeeded",
  "failed",
  "cancelled",
]);
export type GraphBuildState = typeof GraphBuildState.Type;

export const GraphBuildStatus = Schema.Struct({
  state: GraphBuildState,
  mode: Schema.NullOr(GraphBuildMode),
  /** Coarse progress message from the build, e.g. "extracting (120 files)". */
  message: Schema.NullOr(Schema.String),
  startedAt: Schema.NullOr(Schema.Number),
  finishedAt: Schema.NullOr(Schema.Number),
  detail: Schema.NullOr(Schema.String),
});
export type GraphBuildStatus = typeof GraphBuildStatus.Type;

// -- Store -------------------------------------------------------------------

/**
 * Identity of one stored graph.
 *
 * Keyed by project and branch rather than by workspace path. A single checkout
 * keeps one path while its branch changes underneath it, so a path key would
 * serve the wrong graph after a branch switch; a worktree gets a path per
 * branch, so a path key would fragment. Git forbids the same branch in two
 * worktrees, which makes this pair unique across both layouts.
 *
 * `branch` is null for a detached HEAD; the store substitutes the short SHA.
 */
export const GraphStoreKey = Schema.Struct({
  projectId: ProjectId,
  branch: Schema.NullOr(TrimmedNonEmptyString),
});
export type GraphStoreKey = typeof GraphStoreKey.Type;

/**
 * One entry in the graph store, as recorded in its `meta.json`. Surfaced so the
 * settings page can show what is on disk and what the sweep is about to reclaim.
 */
export const GraphStoreEntry = Schema.Struct({
  key: GraphStoreKey,
  /** Checkout the graph was built from. A change forces a full rebuild. */
  workspaceRoot: Schema.String,
  /** HEAD at build time, for staleness reporting. */
  headSha: Schema.NullOr(Schema.String),
  mode: GraphBuildMode,
  /** graphify version that produced it; a mismatch invalidates the entry. */
  graphifyVersion: Schema.String,
  builtAt: Schema.Number,
  /**
   * Last time the graph was read through an RPC. Retention is keyed on this,
   * not on `builtAt`, so a graph in weekly use never expires.
   */
  lastOpenedAt: Schema.Number,
  nodeCount: NonNegativeInt,
  edgeCount: NonNegativeInt,
  /** Total bytes on disk, used by the size-budget eviction pass. */
  sizeBytes: NonNegativeInt,
});
export type GraphStoreEntry = typeof GraphStoreEntry.Type;

/** Everything the client needs to render the panel in one round trip. */
export const GraphStatus = Schema.Struct({
  enabled: Schema.Boolean,
  runtime: GraphRuntimeStatus,
  build: GraphBuildStatus,
  /**
   * Branch the status describes, so the panel can name it and notice a switch.
   * Null when the workspace is not a git checkout or HEAD is detached.
   */
  branch: Schema.NullOr(Schema.String),
  /** Null until a graph has been built for this workspace. */
  snapshot: Schema.NullOr(GraphSnapshot),
});
export type GraphStatus = typeof GraphStatus.Type;

// -- Inputs ------------------------------------------------------------------

export const GraphWorkspaceInput = Schema.Struct({
  cwd: TrimmedNonEmptyString,
});
export type GraphWorkspaceInput = typeof GraphWorkspaceInput.Type;

export const GraphBuildInput = Schema.Struct({
  cwd: TrimmedNonEmptyString,
  mode: GraphBuildMode,
  /** Re-extract everything instead of only changed files. */
  force: Schema.Boolean,
});
export type GraphBuildInput = typeof GraphBuildInput.Type;

export const GraphSubgraphInput = Schema.Struct({
  cwd: TrimmedNonEmptyString,
  /** Centre the neighbourhood on a node. Mutually exclusive with community. */
  nodeId: Schema.NullOr(GraphNodeId),
  /** Return a whole community instead of a node neighbourhood. */
  communityId: Schema.NullOr(NonNegativeInt),
  depth: PositiveInt.check(Schema.isLessThanOrEqualTo(GRAPH_SUBGRAPH_MAX_DEPTH)),
  limit: PositiveInt.check(Schema.isLessThanOrEqualTo(GRAPH_SUBGRAPH_MAX_NODES)),
});
export type GraphSubgraphInput = typeof GraphSubgraphInput.Type;

/** A free-text lookup over node ids and labels. */
export const GraphQueryInput = Schema.Struct({
  cwd: TrimmedNonEmptyString,
  question: TrimmedNonEmptyString.check(Schema.isMaxLength(GRAPH_QUERY_MAX_LENGTH)),
  limit: PositiveInt.check(Schema.isLessThanOrEqualTo(GRAPH_SEARCH_MAX_RESULTS)),
});
export type GraphQueryInput = typeof GraphQueryInput.Type;

/**
 * Names one node. `node` is matched as an exact id first, then an exact label,
 * then a case-insensitive substring — agents rarely have the exact id, and
 * making them run a search before every explain would double every round trip.
 */
export const GraphNodeQueryInput = Schema.Struct({
  cwd: TrimmedNonEmptyString,
  node: TrimmedNonEmptyString.check(Schema.isMaxLength(GRAPH_QUERY_MAX_LENGTH)),
});
export type GraphNodeQueryInput = typeof GraphNodeQueryInput.Type;

export const GraphPathInput = Schema.Struct({
  cwd: TrimmedNonEmptyString,
  from: TrimmedNonEmptyString.check(Schema.isMaxLength(GRAPH_QUERY_MAX_LENGTH)),
  to: TrimmedNonEmptyString.check(Schema.isMaxLength(GRAPH_QUERY_MAX_LENGTH)),
});
export type GraphPathInput = typeof GraphPathInput.Type;

export const GraphInstallRuntimeInput = Schema.Struct({
  /**
   * Interpreter to build the managed venv from. When null the server picks the
   * best Python it can find. Installing is always an explicit user action.
   */
  interpreterPath: Schema.NullOr(TrimmedNonEmptyString),
});
export type GraphInstallRuntimeInput = typeof GraphInstallRuntimeInput.Type;

/**
 * Nodes matching a free-text query, best match first.
 *
 * `totalMatches` is the count before `limit` was applied, so a caller can tell
 * "these are all of them" from "these are the first few of many" — an agent
 * that cannot tell the difference will report a partial answer as complete.
 */
export const GraphSearchResult = Schema.Struct({
  matches: Schema.Array(GraphNode),
  totalMatches: NonNegativeInt,
  /** Mirrors `GraphSnapshot.stale` at answer time. */
  stale: Schema.Boolean,
});
export type GraphSearchResult = typeof GraphSearchResult.Type;

/** One edge, from the point of view of the node being explained. */
export const GraphNeighborLink = Schema.Struct({
  node: GraphNode,
  relation: Schema.String,
  confidence: GraphConfidence,
  confidenceScore: Schema.Number,
});
export type GraphNeighborLink = typeof GraphNeighborLink.Type;

/**
 * Everything the graph knows about one node.
 *
 * Neighbours carry their confidence rather than being flattened into a list of
 * names: an INFERRED link is a guess graphify made, and an agent that cites it
 * as fact is exactly the failure mode this feature has to avoid.
 */
export const GraphExplanation = Schema.Struct({
  node: GraphNode,
  community: Schema.NullOr(GraphCommunity),
  neighbors: Schema.Array(GraphNeighborLink),
  /** True when `neighbors` was clipped — hubs have thousands. */
  truncated: Schema.Boolean,
  stale: Schema.Boolean,
});
export type GraphExplanation = typeof GraphExplanation.Type;

/**
 * The shortest chain of relationships between two nodes.
 *
 * `nodes` is empty when the two are not connected, which is itself an answer
 * worth returning rather than an error.
 */
export const GraphPathResult = Schema.Struct({
  nodes: Schema.Array(GraphNode),
  edges: Schema.Array(GraphEdge),
  stale: Schema.Boolean,
});
export type GraphPathResult = typeof GraphPathResult.Type;

// -- Errors ------------------------------------------------------------------

export class GraphDisabledError extends Schema.TaggedErrorClass<GraphDisabledError>()(
  "GraphDisabledError",
  {},
) {
  override get message(): string {
    return "The knowledge graph feature is disabled. Enable it in settings.";
  }
}

export class GraphRuntimeUnavailableError extends Schema.TaggedErrorClass<GraphRuntimeUnavailableError>()(
  "GraphRuntimeUnavailableError",
  {
    detail: Schema.String,
  },
) {
  override get message(): string {
    return `The graphify runtime is unavailable: ${this.detail}`;
  }
}

export class GraphNotBuiltError extends Schema.TaggedErrorClass<GraphNotBuiltError>()(
  "GraphNotBuiltError",
  {
    cwd: Schema.String,
  },
) {
  override get message(): string {
    return `No knowledge graph has been built for '${this.cwd}'.`;
  }
}

/**
 * An agent's MCP credential does not carry the `graph` capability.
 *
 * Distinct from `GraphDisabledError` even though today the two have the same
 * cause: capabilities are stamped onto a session when it is issued, so a
 * session that started before the feature was switched on keeps its old
 * grant. "Your credential does not allow this" and "the feature is off" call
 * for different fixes, and collapsing them would send an agent to the wrong one.
 */
export class GraphCapabilityUnavailableError extends Schema.TaggedErrorClass<GraphCapabilityUnavailableError>()(
  "GraphCapabilityUnavailableError",
  {},
) {
  override get message(): string {
    return "This agent session was not granted the knowledge-graph capability. Enable the knowledge graph in T3 Code settings and start a new session.";
  }
}

/**
 * A node reference matched nothing.
 *
 * Carries the reference verbatim so the caller — usually an agent — can see
 * what it asked for. Returning an empty result instead would read as "these
 * things are unrelated", which is a different and wrong claim.
 */
export class GraphNodeNotFoundError extends Schema.TaggedErrorClass<GraphNodeNotFoundError>()(
  "GraphNodeNotFoundError",
  {
    reference: Schema.String,
  },
) {
  override get message(): string {
    return `No node in the knowledge graph matches '${this.reference}'.`;
  }
}

/**
 * The workspace is not a project T3 Code knows about.
 *
 * Graphs are keyed by `(projectId, branch)`, and `projectId` comes from an
 * exact match on `projection_projects.workspace_root`. A thread running in a
 * worktree, a subdirectory, or a folder opened outside the project list has no
 * project to key on, and inventing one would build a graph the sweep could
 * never attribute or reclaim. Distinct from `GraphNotBuiltError`, which means
 * "we know where it would go, and it is not there yet".
 */
export class GraphWorkspaceUnknownError extends Schema.TaggedErrorClass<GraphWorkspaceUnknownError>()(
  "GraphWorkspaceUnknownError",
  {
    cwd: Schema.String,
  },
) {
  override get message(): string {
    return `'${this.cwd}' is not a T3 Code project, so no knowledge graph can be stored for it.`;
  }
}

export class GraphCommandFailedError extends Schema.TaggedErrorClass<GraphCommandFailedError>()(
  "GraphCommandFailedError",
  {
    detail: Schema.String,
    exitCode: Schema.NullOr(Schema.Number),
  },
) {
  override get message(): string {
    return `graphify failed: ${this.detail}`;
  }
}

/**
 * Refusal from the graph store's path guard. Distinct from every other error
 * here because it means a computed path failed a containment or shape check —
 * i.e. a bug, not a user-facing condition. Never silently swallowed.
 */
export class GraphStorePathError extends Schema.TaggedErrorClass<GraphStorePathError>()(
  "GraphStorePathError",
  {
    path: Schema.String,
    reason: Schema.String,
  },
) {
  override get message(): string {
    return `Refused to operate on graph store path '${this.path}': ${this.reason}`;
  }
}

export const GraphError = Schema.Union([
  GraphDisabledError,
  GraphRuntimeUnavailableError,
  GraphNotBuiltError,
  GraphNodeNotFoundError,
  GraphCommandFailedError,
  GraphStorePathError,
]);
export type GraphError = typeof GraphError.Type;
