/**
 * GraphCanvas - the only module in the app that imports sigma or graphology.
 *
 * That is a hard constraint, not a style preference. `apps/web/vite.config.ts`
 * sets no `manualChunks`, so Rolldown splits purely on dynamic-import
 * boundaries: as long as this file is reached exclusively through the
 * `React.lazy()` in `GraphPanel`, the renderer lands in its own chunk and a
 * user who never enables the knowledge graph never downloads it. Import it
 * statically from anywhere and that guarantee is silently gone.
 *
 * Layout runs in a Web Worker (`graphology-layout-forceatlas2/worker`).
 * ForceAtlas2 over a thousand nodes is hundreds of milliseconds per tick; on
 * the main thread that is a frozen window for as long as the panel is open.
 *
 * @module GraphCanvas
 */
import type { GraphEdge, GraphNode } from "@t3tools/contracts";
import Graph from "graphology";
import FA2Layout from "graphology-layout-forceatlas2/worker";
import Sigma from "sigma";
import { useEffect, useRef } from "react";

import { EDGE_STYLES, communityColor, nodeSize } from "./graphColors";

/**
 * How long the layout is allowed to run before it is parked.
 *
 * ForceAtlas2 never converges on its own. Left running it burns a core for as
 * long as the panel is open, which on a laptop is a fan spinning up because
 * someone left a tab visible. Ten seconds is well past the point where further
 * ticks stop visibly moving anything at these sizes.
 */
const LAYOUT_RUN_MS = 10_000;

interface GraphCanvasProps {
  readonly nodes: ReadonlyArray<GraphNode>;
  readonly edges: ReadonlyArray<GraphEdge>;
  /** Node the user is focused on, drawn highlighted. */
  readonly selectedNodeId: string | null;
  readonly onSelectNode: (node: GraphNode) => void;
  /** Double-click — the "take me to the code" gesture. */
  readonly onOpenNode: (node: GraphNode) => void;
}

/** Deterministic starting ring. ForceAtlas2 needs coordinates to push apart. */
function seedPosition(index: number, total: number): { x: number; y: number } {
  const angle = (2 * Math.PI * index) / Math.max(1, total);
  const radius = 10 + Math.sqrt(total);
  return { x: Math.cos(angle) * radius, y: Math.sin(angle) * radius };
}

function buildGraph(
  nodes: ReadonlyArray<GraphNode>,
  edges: ReadonlyArray<GraphEdge>,
  selectedNodeId: string | null,
): Graph {
  const graph = new Graph({ multi: false, type: "undirected" });
  nodes.forEach((node, index) => {
    const seed = seedPosition(index, nodes.length);
    graph.addNode(node.id, {
      ...seed,
      label: node.label,
      size: nodeSize(node.degree),
      color: communityColor(node.communityId),
      highlighted: node.id === selectedNodeId,
      // Carried so a click handler can answer with the whole node without a
      // second lookup into the props array.
      payload: node,
    });
  });
  for (const edge of edges) {
    // A subgraph is clipped by `limit`, so an edge can point at a node that
    // did not make the cut. Dropping it is correct — sigma throws on a
    // dangling reference and half an edge means nothing to look at.
    if (!graph.hasNode(edge.source) || !graph.hasNode(edge.target)) continue;
    if (graph.hasEdge(edge.source, edge.target)) continue;
    const style = EDGE_STYLES[edge.confidence];
    graph.addEdge(edge.source, edge.target, {
      label: edge.relation,
      color: style.color,
      size: style.size,
    });
  }
  return graph;
}

export default function GraphCanvas(props: GraphCanvasProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  // Handlers change identity on every render of the parent; holding them in a
  // ref keeps the sigma instance out of the effect's dependency list, so
  // hovering a node does not tear down and rebuild the whole renderer.
  const handlersRef = useRef({ onSelectNode: props.onSelectNode, onOpenNode: props.onOpenNode });
  handlersRef.current = { onSelectNode: props.onSelectNode, onOpenNode: props.onOpenNode };

  const { nodes, edges, selectedNodeId } = props;

  useEffect(() => {
    const container = containerRef.current;
    if (container === null || nodes.length === 0) return;

    const graph = buildGraph(nodes, edges, selectedNodeId);
    const renderer = new Sigma(graph, container, {
      renderEdgeLabels: false,
      defaultEdgeType: "line",
      labelDensity: 0.2,
      labelGridCellSize: 90,
      labelRenderedSizeThreshold: 8,
      zIndex: true,
    });

    const nodePayload = (id: string): GraphNode | null =>
      (graph.getNodeAttribute(id, "payload") as GraphNode | undefined) ?? null;

    renderer.on("clickNode", ({ node }) => {
      const payload = nodePayload(node);
      if (payload !== null) handlersRef.current.onSelectNode(payload);
    });
    renderer.on("doubleClickNode", (event) => {
      // Otherwise sigma also zooms, which is disorienting when the click just
      // swapped the panel to a file.
      event.preventSigmaDefault();
      const payload = nodePayload(event.node);
      if (payload !== null) handlersRef.current.onOpenNode(payload);
    });

    const layout = new FA2Layout(graph, {
      settings: { gravity: 1, scalingRatio: 8, slowDown: 4, barnesHutOptimize: nodes.length > 300 },
    });
    layout.start();
    const parkLayout = setTimeout(() => layout.stop(), LAYOUT_RUN_MS);

    return () => {
      clearTimeout(parkLayout);
      layout.kill();
      renderer.kill();
    };
  }, [nodes, edges, selectedNodeId]);

  return <div className="size-full" ref={containerRef} />;
}
