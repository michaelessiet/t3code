/**
 * The fixtures here are shaped after the real `graphify-out/graph.json` that
 * `graphifyy-0.9.27` writes — `links` rather than `edges`, `source_location`
 * as the string `"L42"`, `community` as a bare int — not after the contract
 * types. Decoding the contract shape back into itself would test nothing.
 */
import { describe, expect, it } from "@effect/vitest";

import {
  buildGraphIndex,
  decodeGraphJson,
  graphExplain,
  graphPath,
  graphSearch,
  graphSnapshot,
  graphSubgraph,
  resolveNodeReference,
} from "./GraphJson.ts";

const decode = (value: unknown) => {
  const result = decodeGraphJson(JSON.stringify(value));
  if (result._tag === "Failure") throw new Error(`fixture did not decode: ${result.failure}`);
  return result.success;
};

const indexOf = (value: unknown) => buildGraphIndex(decode(value));

/** A four-node graph: two communities joined by one bridge edge. */
const SAMPLE = {
  directed: false,
  multigraph: false,
  graph: {},
  nodes: [
    {
      id: "src_server_ts",
      label: "server.ts",
      file_type: "code",
      source_file: "src/server.ts",
      source_location: "L12",
      community: 0,
      community_name: "Transport",
      norm_label: "server.ts",
    },
    {
      id: "src_ws_ts",
      label: "ws.ts",
      file_type: "code",
      source_file: "src/ws.ts",
      source_location: "L1",
      community: 0,
      community_name: "Transport",
    },
    {
      id: "docs_readme_md",
      label: "README.md",
      file_type: "document",
      source_file: "docs/README.md",
      source_location: "3",
      community: 1,
      community_name: "Docs",
    },
    {
      id: "concept_auth",
      label: "authentication",
      file_type: "concept",
      source_file: null,
      source_location: null,
      community: 1,
    },
  ],
  links: [
    {
      source: "src_server_ts",
      target: "src_ws_ts",
      relation: "imports_from",
      confidence: "EXTRACTED",
      confidence_score: 1,
    },
    {
      source: "docs_readme_md",
      target: "concept_auth",
      relation: "describes",
      confidence: "INFERRED",
    },
    {
      source: "src_server_ts",
      target: "concept_auth",
      relation: "implements",
      confidence: "AMBIGUOUS",
      confidence_score: 0.25,
    },
  ],
  hyperedges: [],
  built_at_commit: "8cf40d1a7",
};

describe("decodeGraphJson", () => {
  it("reads the envelope graphify actually writes", () => {
    const index = indexOf(SAMPLE);
    expect(index.nodes.size).toBe(4);
    expect(index.edges).toHaveLength(3);
    expect(index.builtAtCommit).toBe("8cf40d1a7");
    expect(index.skipped).toEqual({ nodes: 0, edges: 0 });
  });

  it("accepts `edges` as well as `links`, which --no-cluster output uses", () => {
    const { links, ...rest } = SAMPLE;
    expect(indexOf({ ...rest, edges: links }).edges).toHaveLength(3);
  });

  it("fails only when the envelope itself is not a graph", () => {
    expect(decodeGraphJson("not json")._tag).toBe("Failure");
    expect(decodeGraphJson('{"nodes":"nope"}')._tag).toBe("Failure");
  });

  it("survives an empty graph", () => {
    const index = indexOf({ nodes: [], links: [] });
    expect(index.nodes.size).toBe(0);
    expect(index.communities).toEqual([]);
    expect(index.godNodes).toEqual([]);
  });
});

describe("normalisation degrades instead of failing", () => {
  // The whole reason the per-entry path is hand-written: graphify is 0.x and
  // adds node kinds between releases. One unfamiliar node must not blank the
  // panel.
  it("maps an unrecognised file_type to `unknown` rather than rejecting the node", () => {
    const index = indexOf({
      nodes: [{ id: "n1", label: "n1", file_type: "quantum_flux" }],
      links: [],
    });
    expect(index.nodes.get("n1")?.fileType).toBe("unknown");
    expect(index.skipped.nodes).toBe(0);
  });

  it("keeps `doc_ref`, which the contract's first draft did not know about", () => {
    const index = indexOf({ nodes: [{ id: "n1", file_type: "doc_ref" }], links: [] });
    expect(index.nodes.get("n1")?.fileType).toBe("doc_ref");
  });

  it("counts unusable nodes and edges instead of discarding the graph", () => {
    const index = indexOf({
      nodes: [{ id: "n1" }, { label: "no id" }, "not an object", null, { id: "n2" }],
      links: [{ source: "n1", target: "n2" }, { source: "n1" }, 42],
    });
    expect(index.nodes.size).toBe(2);
    expect(index.skipped.nodes).toBe(3);
    expect(index.edges).toHaveLength(1);
    expect(index.skipped.edges).toBe(2);
  });

  it("drops dangling edges, which would otherwise crash a layout", () => {
    const index = indexOf({
      nodes: [{ id: "n1" }],
      links: [{ source: "n1", target: "ghost" }],
    });
    expect(index.edges).toEqual([]);
    expect(index.skipped.edges).toBe(1);
  });

  it("falls back to the node id when there is no label", () => {
    expect(indexOf({ nodes: [{ id: "n1" }], links: [] }).nodes.get("n1")?.label).toBe("n1");
  });
});

describe("source line parsing", () => {
  // Clicking a node opens a file at a line. A wrong line is worse than none.
  it.each([
    ["L12", 12],
    ["3", 3],
    ["  L7  ", 7],
    ["L0", null],
    ["L12-L18", null],
    ["main", null],
    ["", null],
    [null, null],
    [12, null],
  ])("parses %j as %j", (raw, expected) => {
    const index = indexOf({ nodes: [{ id: "n1", source_location: raw }], links: [] });
    expect(index.nodes.get("n1")?.sourceLine).toBe(expected);
  });
});

describe("edge confidence", () => {
  it("defaults a missing score the way graphify's exporter does", () => {
    const index = indexOf({
      nodes: [{ id: "a" }, { id: "b" }, { id: "c" }],
      links: [
        { source: "a", target: "b", confidence: "INFERRED" },
        { source: "b", target: "c", confidence: "AMBIGUOUS" },
      ],
    });
    expect(index.edges.map((edge) => edge.confidenceScore)).toEqual([0.6, 0.3]);
  });

  it("treats an unknown confidence as EXTRACTED with its default score", () => {
    const index = indexOf({
      nodes: [{ id: "a" }, { id: "b" }],
      links: [{ source: "a", target: "b", confidence: "GUESSED" }],
    });
    expect(index.edges[0]?.confidence).toBe("EXTRACTED");
    expect(index.edges[0]?.confidenceScore).toBe(1);
  });
});

describe("index derivation", () => {
  it("counts degree from the links, since graphify does not store it", () => {
    const index = indexOf(SAMPLE);
    expect(index.nodes.get("src_server_ts")?.degree).toBe(2);
    expect(index.nodes.get("src_ws_ts")?.degree).toBe(1);
  });

  it("prefers graphify's community_name and falls back to `Community <n>`", () => {
    const index = indexOf(SAMPLE);
    expect(index.communities.map((community) => community.label).sort()).toEqual([
      "Docs",
      "Transport",
    ]);

    const unlabelled = indexOf({
      nodes: [
        { id: "a", community: 4 },
        { id: "b", community: 4 },
      ],
      links: [],
    });
    expect(unlabelled.communities[0]?.label).toBe("Community 4");
  });

  it("scores cohesion the way graphify's cluster.py does", () => {
    // Community 0 has 2 members and 1 internal edge: 1 / (2*1/2) = 1.
    // Community 1 has 2 members and 1 internal edge, likewise 1.
    const index = indexOf(SAMPLE);
    expect(index.communities.every((community) => community.cohesion === 1)).toBe(true);

    // 3 members, 1 internal edge: 1 / 3.
    const sparse = indexOf({
      nodes: [
        { id: "a", community: 0 },
        { id: "b", community: 0 },
        { id: "c", community: 0 },
      ],
      links: [{ source: "a", target: "b" }],
    });
    expect(sparse.communities[0]?.cohesion).toBeCloseTo(1 / 3);
  });

  it("ranks god nodes by degree, breaking ties on id so the order is stable", () => {
    // `src_server_ts` and `concept_auth` both have degree 2, so the id
    // tie-break decides — and must decide the same way on every call, or the
    // panel's hub list reshuffles between polls.
    const index = indexOf(SAMPLE);
    expect(index.godNodes.map((node) => [node.id, node.degree])).toEqual([
      ["concept_auth", 2],
      ["src_server_ts", 2],
      ["docs_readme_md", 1],
      ["src_ws_ts", 1],
    ]);
  });

  it("ignores nodes with no community", () => {
    const index = indexOf({ nodes: [{ id: "a" }, { id: "b", community: 0 }], links: [] });
    expect(index.communities).toHaveLength(1);
    expect(index.communities[0]?.nodeCount).toBe(1);
  });
});

describe("graphSnapshot", () => {
  it("is a community-level aggregate, never the node list", () => {
    const snapshot = graphSnapshot(indexOf(SAMPLE), { builtAt: 1_700_000_000_000, stale: true });
    expect(snapshot.nodeCount).toBe(4);
    expect(snapshot.edgeCount).toBe(3);
    expect(snapshot.communities).toHaveLength(2);
    expect(snapshot.stale).toBe(true);
    expect(Object.keys(snapshot)).not.toContain("nodes");
  });
});

describe("graphSubgraph", () => {
  it("walks a bounded neighbourhood from a node", () => {
    const result = graphSubgraph(indexOf(SAMPLE), {
      nodeId: "src_server_ts",
      communityId: null,
      depth: 1,
      limit: 100,
    });
    expect(result.nodes.map((node) => node.id).sort()).toEqual([
      "concept_auth",
      "src_server_ts",
      "src_ws_ts",
    ]);
    expect(result.truncated).toBe(false);
  });

  it("reaches further at greater depth", () => {
    const result = graphSubgraph(indexOf(SAMPLE), {
      nodeId: "src_ws_ts",
      communityId: null,
      depth: 3,
      limit: 100,
    });
    expect(result.nodes).toHaveLength(4);
  });

  it("only returns edges whose endpoints both survived the slice", () => {
    const result = graphSubgraph(indexOf(SAMPLE), {
      nodeId: "src_ws_ts",
      communityId: null,
      depth: 1,
      limit: 100,
    });
    const ids = new Set(result.nodes.map((node) => node.id));
    expect(result.edges.every((edge) => ids.has(edge.source) && ids.has(edge.target))).toBe(true);
  });

  it("flags truncation rather than silently returning a partial view", () => {
    const result = graphSubgraph(indexOf(SAMPLE), {
      nodeId: "src_server_ts",
      communityId: null,
      depth: 3,
      limit: 2,
    });
    expect(result.nodes).toHaveLength(2);
    expect(result.truncated).toBe(true);
  });

  it("returns a whole community when asked for one", () => {
    const result = graphSubgraph(indexOf(SAMPLE), {
      nodeId: null,
      communityId: 1,
      depth: 1,
      limit: 100,
    });
    expect(result.nodes.map((node) => node.id).sort()).toEqual(["concept_auth", "docs_readme_md"]);
  });

  it("falls back to the hubs with no anchor", () => {
    const result = graphSubgraph(indexOf(SAMPLE), {
      nodeId: null,
      communityId: null,
      depth: 1,
      limit: 100,
    });
    expect(result.nodes).toHaveLength(4);
  });

  it("is empty, not broken, for a node that is not in the graph", () => {
    const result = graphSubgraph(indexOf(SAMPLE), {
      nodeId: "does_not_exist",
      communityId: null,
      depth: 2,
      limit: 100,
    });
    expect(result.nodes).toEqual([]);
    expect(result.edges).toEqual([]);
  });
});

describe("resolveNodeReference", () => {
  const index = indexOf(SAMPLE);

  it("prefers an exact id", () => {
    expect(resolveNodeReference(index, "concept_auth")?.id).toBe("concept_auth");
  });

  it("falls back to an exact label", () => {
    expect(resolveNodeReference(index, "README.md")?.id).toBe("docs_readme_md");
  });

  it("matches a substring case-insensitively", () => {
    expect(resolveNodeReference(index, "READ")?.id).toBe("docs_readme_md");
  });

  // An agent given the wrong long-named node spends a turn on the wrong file;
  // the shortest match is the one whose name is mostly the query.
  it("takes the shortest label when several match", () => {
    expect(resolveNodeReference(index, "s")?.id).toBe("src_ws_ts");
  });

  it("returns null rather than a plausible wrong node", () => {
    expect(resolveNodeReference(index, "nothing_like_this")).toBeNull();
    expect(resolveNodeReference(index, "  ")).toBeNull();
  });
});

describe("graphSearch", () => {
  const index = indexOf(SAMPLE);

  it("finds by id and by label", () => {
    expect(graphSearch(index, { question: "server", limit: 10 }).matches.map((n) => n.id)).toEqual([
      "src_server_ts",
    ]);
    expect(
      graphSearch(index, { question: "authentication", limit: 10 }).matches.map((n) => n.id),
    ).toEqual(["concept_auth"]);
  });

  // Whole-name hits outrank fragments of long names, and degree breaks the tie.
  it("ranks tighter matches first, then hubs", () => {
    const result = graphSearch(index, { question: "s", limit: 10 });
    expect(result.matches.map((node) => node.id)).toEqual([
      "src_ws_ts",
      "src_server_ts",
      "docs_readme_md",
    ]);
  });

  // Without this an agent reports the first page as the whole answer.
  it("reports the pre-limit total so a partial answer is visible as one", () => {
    const result = graphSearch(index, { question: "s", limit: 2 });
    expect(result.matches).toHaveLength(2);
    expect(result.totalMatches).toBe(3);
  });

  it("is empty for a miss and for a blank question", () => {
    expect(graphSearch(index, { question: "zzz", limit: 10 }).totalMatches).toBe(0);
    expect(graphSearch(index, { question: "   ", limit: 10 }).totalMatches).toBe(0);
  });
});

describe("graphExplain", () => {
  const index = indexOf(SAMPLE);
  const server = resolveNodeReference(index, "src_server_ts");
  if (server === null) throw new Error("fixture node missing");

  // Clipping must drop the guesses, not the facts, so confidence sorts first.
  it("lists neighbours strongest link first", () => {
    const result = graphExplain(index, server, 10);
    expect(result.neighbors.map((link) => link.node.id)).toEqual(["src_ws_ts", "concept_auth"]);
    expect(result.neighbors[0]?.confidence).toBe("EXTRACTED");
    expect(result.truncated).toBe(false);
  });

  it("flags clipping instead of quietly dropping links", () => {
    const result = graphExplain(index, server, 1);
    expect(result.neighbors.map((link) => link.node.id)).toEqual(["src_ws_ts"]);
    expect(result.truncated).toBe(true);
  });

  it("resolves the node's community", () => {
    expect(graphExplain(index, server, 10).community?.label).toBe("Transport");
  });

  it("has no neighbours for an isolated node", () => {
    const isolated = indexOf({
      ...SAMPLE,
      nodes: [...SAMPLE.nodes, { id: "orphan", label: "orphan", file_type: "concept" }],
    });
    const node = resolveNodeReference(isolated, "orphan");
    if (node === null) throw new Error("fixture node missing");
    const result = graphExplain(isolated, node, 10);
    expect(result.neighbors).toEqual([]);
    expect(result.truncated).toBe(false);
    expect(result.community).toBeNull();
  });
});

describe("graphPath", () => {
  const index = indexOf(SAMPLE);
  const node = (reference: string) => {
    const found = resolveNodeReference(index, reference);
    if (found === null) throw new Error(`fixture node missing: ${reference}`);
    return found;
  };

  it("walks the shortest chain across communities", () => {
    const result = graphPath(index, node("src_ws_ts"), node("docs_readme_md"));
    expect(result.nodes.map((entry) => entry.id)).toEqual([
      "src_ws_ts",
      "src_server_ts",
      "concept_auth",
      "docs_readme_md",
    ]);
    // One fewer edge than nodes, and each one joins consecutive nodes.
    expect(result.edges).toHaveLength(3);
    expect(result.edges.map((edge) => edge.relation)).toEqual([
      "imports_from",
      "implements",
      "describes",
    ]);
  });

  it("is a single node with no edges when asked for a node's path to itself", () => {
    const result = graphPath(index, node("concept_auth"), node("concept_auth"));
    expect(result.nodes.map((entry) => entry.id)).toEqual(["concept_auth"]);
    expect(result.edges).toEqual([]);
  });

  // "Nothing connects these" is the answer, not a failure.
  it("returns empty for two nodes in disconnected parts of the graph", () => {
    const split = indexOf({
      ...SAMPLE,
      nodes: [...SAMPLE.nodes, { id: "orphan", label: "orphan", file_type: "concept" }],
    });
    const from = resolveNodeReference(split, "src_ws_ts");
    const to = resolveNodeReference(split, "orphan");
    if (from === null || to === null) throw new Error("fixture node missing");
    const result = graphPath(split, from, to);
    expect(result.nodes).toEqual([]);
    expect(result.edges).toEqual([]);
  });
});
