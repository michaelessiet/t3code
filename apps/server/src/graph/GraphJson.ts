/**
 * Decoding and slicing of graphify's `graph.json`.
 *
 * Everything here is pure: no filesystem, no subprocess, no Effect services.
 * `WorkspaceGraph` owns reading the file and caching the result; this module
 * only turns bytes into a queryable index and the index into bounded payloads.
 *
 * **Decoding degrades, it never throws.** graphify is a fast-moving 0.x — the
 * shipped 0.9.27 wheel emits `file_type` values that its own README does not
 * list, and node attributes appear between releases. A strict decode would take
 * the entire panel down over one unfamiliar node, so the envelope is validated
 * with `Schema` and each node/edge is then normalised individually: anything
 * unusable is counted and skipped. `GraphIndex.skipped` carries that count so
 * the UI can say "1,204 of 1,210 nodes" rather than quietly lying.
 *
 * Shapes are taken from the 0.9.27 wheel, not from the docs:
 * - `graphify/export.py:292` writes `networkx.json_graph.node_link_data(G,
 *   edges="links")`, so edges live under `links`, not `edges`.
 * - `:297` stamps `community` (int or null) onto each node, `:299`
 *   `community_name`, `:300` `norm_label`.
 * - `:304` defaults `confidence_score` when the extractor omitted it.
 * - `source_location` is a *string* — `f"L{line}"` at `extract.py:328` and
 *   friends, but bare `str(line)` at `:937`. Both are parsed.
 * - Node degree is not stored; it is counted from the links.
 *
 * @module GraphJson
 */
import {
  type GraphCommunity,
  type GraphConfidence,
  type GraphEdge,
  type GraphFileType,
  type GraphNeighborLink,
  type GraphNode,
  type GraphSnapshot,
  type GraphSubgraph,
} from "@t3tools/contracts";
import * as Schema from "effect/Schema";

/** How many hubs `GraphSnapshot.godNodes` carries. Matches graphify's report. */
const GOD_NODE_COUNT = 12;

const FILE_TYPES: ReadonlySet<string> = new Set<GraphFileType>([
  "code",
  "document",
  "paper",
  "image",
  "rationale",
  "concept",
  "doc_ref",
  "unknown",
]);

const CONFIDENCES: ReadonlySet<string> = new Set<GraphConfidence>([
  "EXTRACTED",
  "INFERRED",
  "AMBIGUOUS",
]);

/**
 * Fallback scores for an edge whose extractor omitted `confidence_score`,
 * mirroring `_CONFIDENCE_SCORE_DEFAULTS` in `graphify/export.py`.
 */
const CONFIDENCE_SCORE_DEFAULTS: Record<GraphConfidence, number> = {
  EXTRACTED: 1,
  INFERRED: 0.6,
  AMBIGUOUS: 0.3,
};

/**
 * The envelope, and only the envelope.
 *
 * Nodes and links stay `Unknown` on purpose: validating them here would make
 * one malformed entry fatal for the whole graph, which is exactly the failure
 * mode this feature must not have.
 */
const RawGraphJson = Schema.Struct({
  nodes: Schema.Array(Schema.Unknown),
  links: Schema.optionalKey(Schema.Array(Schema.Unknown)),
  /** Older graphs, and `--no-cluster` output, use `edges` instead. */
  edges: Schema.optionalKey(Schema.Array(Schema.Unknown)),
  built_at_commit: Schema.optionalKey(Schema.String),
});

/**
 * Result-returning rather than Effect-returning, so this module keeps its
 * "no Effect services" promise and stays callable from a plain expression.
 * `WorkspaceGraph` lifts it with `Effect.fromResult` at the one call site that
 * needs it.
 */
export const decodeGraphJson = Schema.decodeUnknownResult(Schema.fromJsonString(RawGraphJson));

export interface GraphIndex {
  readonly nodes: ReadonlyMap<string, GraphNode>;
  readonly edges: ReadonlyArray<GraphEdge>;
  /** Every edge touching a node, by node id. Undirected adjacency. */
  readonly adjacency: ReadonlyMap<string, ReadonlyArray<GraphEdge>>;
  readonly communities: ReadonlyArray<GraphCommunity>;
  readonly nodesByCommunity: ReadonlyMap<number, ReadonlyArray<GraphNode>>;
  /** Hubs, highest degree first. */
  readonly godNodes: ReadonlyArray<GraphNode>;
  /** HEAD graphify recorded at build time, when it recorded one. */
  readonly builtAtCommit: string | null;
  /** Entries dropped during normalisation, so the UI can admit to the gap. */
  readonly skipped: { readonly nodes: number; readonly edges: number };
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function asString(value: unknown): string | null {
  return typeof value === "string" && value !== "" ? value : null;
}

/**
 * `source_location` is `"L42"` in most extractors and `"42"` in one. Anything
 * else — a range, a symbol name, a future format — yields null rather than a
 * wrong line number, because the payoff of this feature is clicking a node and
 * landing in the right place.
 */
function parseSourceLine(value: unknown): number | null {
  const raw = asString(value);
  if (raw === null) return null;
  const match = /^L?(\d+)$/.exec(raw.trim());
  if (match?.[1] === undefined) return null;
  const line = Number.parseInt(match[1], 10);
  return Number.isSafeInteger(line) && line > 0 ? line : null;
}

function parseCommunityId(value: unknown): number | null {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) return null;
  return value;
}

function normalizeNode(raw: unknown): GraphNode | null {
  const record = asRecord(raw);
  if (record === null) return null;
  const id = asString(record["id"]);
  if (id === null) return null;
  const fileTypeRaw = asString(record["file_type"]);
  return {
    id,
    label: asString(record["label"]) ?? id,
    fileType:
      fileTypeRaw !== null && FILE_TYPES.has(fileTypeRaw)
        ? (fileTypeRaw as GraphFileType)
        : "unknown",
    sourceFile: asString(record["source_file"]),
    sourceLine: parseSourceLine(record["source_location"]),
    communityId: parseCommunityId(record["community"]),
    // Filled in once the links have been counted.
    degree: 0,
  };
}

function normalizeEdge(raw: unknown): GraphEdge | null {
  const record = asRecord(raw);
  if (record === null) return null;
  const source = asString(record["source"]);
  const target = asString(record["target"]);
  if (source === null || target === null) return null;
  const confidenceRaw = asString(record["confidence"]);
  const confidence: GraphConfidence =
    confidenceRaw !== null && CONFIDENCES.has(confidenceRaw)
      ? (confidenceRaw as GraphConfidence)
      : "EXTRACTED";
  const score = record["confidence_score"];
  return {
    source,
    target,
    relation: asString(record["relation"]) ?? "relates_to",
    confidence,
    confidenceScore:
      typeof score === "number" && Number.isFinite(score)
        ? score
        : CONFIDENCE_SCORE_DEFAULTS[confidence],
  };
}

/**
 * Community label, preferring the one graphify's semantic pass wrote.
 *
 * `community_name` only exists when the graph was built with labels, so a
 * structural-only build falls back to the same `Community <n>` string graphify
 * uses in that case (`export.py:299`).
 */
function communityLabel(id: number, named: string | null): string {
  return named ?? `Community ${id}`;
}

/**
 * Ratio of actual intra-community edges to the maximum possible.
 *
 * Deliberately identical to `cohesion_score` in `graphify/cluster.py:257` so
 * the number the panel shows matches the number graphify's own `GRAPH_REPORT.md`
 * shows for the same graph.
 */
function cohesionScore(memberCount: number, internalEdges: number): number {
  if (memberCount <= 1) return 1;
  const possible = (memberCount * (memberCount - 1)) / 2;
  return possible > 0 ? internalEdges / possible : 0;
}

export function buildGraphIndex(raw: typeof RawGraphJson.Type): GraphIndex {
  const nodes = new Map<string, GraphNode>();
  let skippedNodes = 0;
  for (const entry of raw.nodes) {
    const node = normalizeNode(entry);
    if (node === null) {
      skippedNodes += 1;
      continue;
    }
    nodes.set(node.id, node);
  }

  const degrees = new Map<string, number>();
  const adjacency = new Map<string, Array<GraphEdge>>();
  const edges: Array<GraphEdge> = [];
  let skippedEdges = 0;
  for (const entry of raw.links ?? raw.edges ?? []) {
    const edge = normalizeEdge(entry);
    // A dangling edge is a real condition — graphify ships
    // `prune_dangling_edges` for it — and rendering one would crash a layout.
    if (edge === null || !nodes.has(edge.source) || !nodes.has(edge.target)) {
      skippedEdges += 1;
      continue;
    }
    edges.push(edge);
    for (const endpoint of [edge.source, edge.target]) {
      degrees.set(endpoint, (degrees.get(endpoint) ?? 0) + 1);
      const bucket = adjacency.get(endpoint);
      if (bucket === undefined) adjacency.set(endpoint, [edge]);
      else bucket.push(edge);
    }
  }

  for (const [id, node] of nodes) {
    nodes.set(id, { ...node, degree: degrees.get(id) ?? 0 });
  }

  // Community names come off the nodes, so collect them in the same pass.
  const nodesByCommunity = new Map<number, Array<GraphNode>>();
  const communityNames = new Map<number, string>();
  for (const node of nodes.values()) {
    if (node.communityId === null) continue;
    const bucket = nodesByCommunity.get(node.communityId);
    if (bucket === undefined) nodesByCommunity.set(node.communityId, [node]);
    else bucket.push(node);
  }
  for (const entry of raw.nodes) {
    const record = asRecord(entry);
    if (record === null) continue;
    const id = parseCommunityId(record["community"]);
    const name = asString(record["community_name"]);
    if (id !== null && name !== null && !communityNames.has(id)) communityNames.set(id, name);
  }

  const internalEdges = new Map<number, number>();
  for (const edge of edges) {
    const source = nodes.get(edge.source)?.communityId ?? null;
    const target = nodes.get(edge.target)?.communityId ?? null;
    if (source !== null && source === target) {
      internalEdges.set(source, (internalEdges.get(source) ?? 0) + 1);
    }
  }

  const communities: Array<GraphCommunity> = [];
  for (const [id, members] of nodesByCommunity) {
    communities.push({
      id,
      label: communityLabel(id, communityNames.get(id) ?? null),
      nodeCount: members.length,
      cohesion: cohesionScore(members.length, internalEdges.get(id) ?? 0),
    });
  }
  communities.sort((a, b) => b.nodeCount - a.nodeCount || a.id - b.id);

  const godNodes = [...nodes.values()]
    .sort((a, b) => b.degree - a.degree || a.id.localeCompare(b.id))
    .slice(0, GOD_NODE_COUNT);

  return {
    nodes,
    edges,
    adjacency,
    communities,
    nodesByCommunity,
    godNodes,
    builtAtCommit: raw.built_at_commit ?? null,
    skipped: { nodes: skippedNodes, edges: skippedEdges },
  };
}

export function graphSnapshot(
  index: GraphIndex,
  options: { readonly builtAt: number; readonly stale: boolean },
): GraphSnapshot {
  return {
    nodeCount: index.nodes.size,
    edgeCount: index.edges.length,
    communities: index.communities,
    godNodes: index.godNodes,
    builtAt: options.builtAt,
    stale: options.stale,
  };
}

export interface SubgraphRequest {
  readonly nodeId: string | null;
  readonly communityId: number | null;
  readonly depth: number;
  readonly limit: number;
}

/**
 * A bounded slice of the graph.
 *
 * Never returns the whole thing: a monorepo graph is tens of megabytes and the
 * client renders progressively. `truncated` is true whenever `limit` cut the
 * traversal short, so the UI can offer "expand" rather than silently showing a
 * partial neighbourhood as if it were complete.
 */
export function graphSubgraph(index: GraphIndex, request: SubgraphRequest): GraphSubgraph {
  const selected = new Map<string, GraphNode>();
  let truncated = false;

  const admit = (node: GraphNode): boolean => {
    if (selected.has(node.id)) return true;
    if (selected.size >= request.limit) {
      truncated = true;
      return false;
    }
    selected.set(node.id, node);
    return true;
  };

  if (request.communityId !== null) {
    // Highest-degree members first, so a clipped community still shows the
    // part of it worth looking at.
    const members = [...(index.nodesByCommunity.get(request.communityId) ?? [])].sort(
      (a, b) => b.degree - a.degree || a.id.localeCompare(b.id),
    );
    for (const node of members) if (!admit(node)) break;
  } else if (request.nodeId !== null) {
    const origin = index.nodes.get(request.nodeId);
    if (origin !== undefined) {
      admit(origin);
      let frontier: ReadonlyArray<string> = [origin.id];
      for (let depth = 0; depth < request.depth && frontier.length > 0; depth += 1) {
        const next: Array<string> = [];
        for (const id of frontier) {
          for (const edge of index.adjacency.get(id) ?? []) {
            const neighbourId = edge.source === id ? edge.target : edge.source;
            if (selected.has(neighbourId)) continue;
            const neighbour = index.nodes.get(neighbourId);
            if (neighbour === undefined) continue;
            if (!admit(neighbour)) return sliceEdges(index, selected, true);
            next.push(neighbourId);
          }
        }
        frontier = next;
      }
    }
  } else {
    // No anchor: show the hubs, which is the most useful default first view.
    for (const node of index.godNodes) if (!admit(node)) break;
  }

  return sliceEdges(index, selected, truncated);
}

/**
 * Resolves the loose node reference an agent is likely to have.
 *
 * Exact id, then exact label, then a case-insensitive substring on either —
 * shortest match first, because `user` should find `user` rather than
 * `user_preferences_migration_v2`. Returns null rather than guessing between
 * two equally-good matches of the same length; the caller turns that into
 * "say which one".
 */
export function resolveNodeReference(index: GraphIndex, reference: string): GraphNode | null {
  const exact = index.nodes.get(reference);
  if (exact !== undefined) return exact;

  const needle = reference.trim().toLowerCase();
  if (needle === "") return null;

  let best: GraphNode | null = null;
  for (const node of index.nodes.values()) {
    if (node.label === reference) return node;
    const haystack = `${node.id} ${node.label.toLowerCase()}`;
    if (!haystack.includes(needle)) continue;
    if (best === null || node.label.length < best.label.length) best = node;
  }
  return best;
}

export interface SearchRequest {
  readonly question: string;
  readonly limit: number;
}

/**
 * Free-text lookup over ids and labels.
 *
 * Ranked by how much of the match is the query — a whole-name hit beats a
 * fragment of a long name — and then by degree, so the most connected of
 * several equally-good matches comes first. No fuzzy matching: an agent
 * spending a turn on a plausible-looking wrong node is worse than one that
 * gets told there is no match.
 */
export function graphSearch(
  index: GraphIndex,
  request: SearchRequest,
): { readonly matches: ReadonlyArray<GraphNode>; readonly totalMatches: number } {
  const needle = request.question.trim().toLowerCase();
  if (needle === "") return { matches: [], totalMatches: 0 };

  const scored: Array<{ readonly node: GraphNode; readonly score: number }> = [];
  for (const node of index.nodes.values()) {
    const id = node.id.toLowerCase();
    const label = node.label.toLowerCase();
    if (!id.includes(needle) && !label.includes(needle)) continue;
    const shortest = Math.min(id.length, label.length) || 1;
    scored.push({ node, score: needle.length / shortest });
  }

  scored.sort(
    (a, b) =>
      b.score - a.score || b.node.degree - a.node.degree || a.node.id.localeCompare(b.node.id),
  );
  return {
    matches: scored.slice(0, request.limit).map((entry) => entry.node),
    totalMatches: scored.length,
  };
}

export interface ExplainResult {
  readonly node: GraphNode;
  readonly community: GraphCommunity | null;
  readonly neighbors: ReadonlyArray<GraphNeighborLink>;
  readonly truncated: boolean;
}

/**
 * One node and what it touches, strongest link first.
 *
 * "Strongest" is the confidence score graphify recorded, so an EXTRACTED edge
 * outranks an INFERRED one before either is clipped by the limit — if only
 * some of a hub's links fit, they should be the ones the code actually states.
 */
export function graphExplain(
  index: GraphIndex,
  node: GraphNode,
  maxNeighbors: number,
): ExplainResult {
  const links: Array<GraphNeighborLink> = [];
  const seen = new Set<string>();
  for (const edge of index.adjacency.get(node.id) ?? []) {
    const otherId = edge.source === node.id ? edge.target : edge.source;
    // A self-edge has nothing to say about structure, and a repeated relation
    // between the same pair is the same fact twice.
    if (otherId === node.id) continue;
    const key = `${otherId} ${edge.relation}`;
    if (seen.has(key)) continue;
    const other = index.nodes.get(otherId);
    if (other === undefined) continue;
    seen.add(key);
    links.push({
      node: other,
      relation: edge.relation,
      confidence: edge.confidence,
      confidenceScore: edge.confidenceScore,
    });
  }

  links.sort(
    (a, b) =>
      b.confidenceScore - a.confidenceScore ||
      b.node.degree - a.node.degree ||
      a.node.id.localeCompare(b.node.id),
  );

  const community =
    node.communityId === null
      ? null
      : (index.communities.find((entry) => entry.id === node.communityId) ?? null);

  return {
    node,
    community,
    neighbors: links.slice(0, maxNeighbors),
    truncated: links.length > maxNeighbors,
  };
}

/**
 * Shortest chain of relationships between two nodes, or empty when there is
 * none.
 *
 * Breadth-first on the undirected adjacency, which is the right search for an
 * unweighted graph and is what makes "how are these two related" answerable in
 * one call. Disconnection is a legitimate answer, not a failure — "nothing
 * connects these" is often the fact the caller needed.
 */
export function graphPath(
  index: GraphIndex,
  from: GraphNode,
  to: GraphNode,
): { readonly nodes: ReadonlyArray<GraphNode>; readonly edges: ReadonlyArray<GraphEdge> } {
  if (from.id === to.id) return { nodes: [from], edges: [] };

  /** Node id → the edge that first reached it, and where from. */
  const cameFrom = new Map<string, { readonly via: GraphEdge; readonly previous: string }>();
  const visited = new Set<string>([from.id]);
  let frontier: ReadonlyArray<string> = [from.id];

  while (frontier.length > 0) {
    const next: Array<string> = [];
    for (const id of frontier) {
      for (const edge of index.adjacency.get(id) ?? []) {
        const neighbourId = edge.source === id ? edge.target : edge.source;
        if (visited.has(neighbourId) || !index.nodes.has(neighbourId)) continue;
        visited.add(neighbourId);
        cameFrom.set(neighbourId, { via: edge, previous: id });
        if (neighbourId === to.id) return reconstructPath(index, cameFrom, from.id, to.id);
        next.push(neighbourId);
      }
    }
    frontier = next;
  }

  return { nodes: [], edges: [] };
}

function reconstructPath(
  index: GraphIndex,
  cameFrom: ReadonlyMap<string, { readonly via: GraphEdge; readonly previous: string }>,
  fromId: string,
  toId: string,
): { readonly nodes: ReadonlyArray<GraphNode>; readonly edges: ReadonlyArray<GraphEdge> } {
  const nodes: Array<GraphNode> = [];
  const edges: Array<GraphEdge> = [];
  let cursor = toId;
  while (cursor !== fromId) {
    const step = cameFrom.get(cursor);
    if (step === undefined) return { nodes: [], edges: [] };
    const node = index.nodes.get(cursor);
    if (node !== undefined) nodes.push(node);
    edges.push(step.via);
    cursor = step.previous;
  }
  const origin = index.nodes.get(fromId);
  if (origin !== undefined) nodes.push(origin);
  return { nodes: nodes.toReversed(), edges: edges.toReversed() };
}

/** Every edge whose endpoints both survived selection. */
function sliceEdges(
  index: GraphIndex,
  selected: ReadonlyMap<string, GraphNode>,
  truncated: boolean,
): GraphSubgraph {
  const edges: Array<GraphEdge> = [];
  const seen = new Set<string>();
  for (const id of selected.keys()) {
    for (const edge of index.adjacency.get(id) ?? []) {
      if (!selected.has(edge.source) || !selected.has(edge.target)) continue;
      const key = `${edge.source}\u0000${edge.target}\u0000${edge.relation}`;
      if (seen.has(key)) continue;
      seen.add(key);
      edges.push(edge);
    }
  }
  return { nodes: [...selected.values()], edges, truncated };
}
