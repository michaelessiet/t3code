/**
 * Colour and shape rules shared by the canvas and the sidebar.
 *
 * Kept out of `GraphCanvas.tsx` deliberately: that module is the only one
 * allowed to import sigma/graphology, and pulling it in just to read a colour
 * would drag the renderer into the eager bundle.
 *
 * @module graphColors
 */
import type { GraphConfidence } from "@t3tools/contracts";

/**
 * Community palette.
 *
 * Fixed hex rather than CSS custom properties because sigma paints to a
 * canvas and cannot resolve `var(--…)`. Chosen to stay legible on both the
 * light and dark backgrounds the panel can sit on.
 */
const COMMUNITY_COLORS: ReadonlyArray<string> = [
  "#60a5fa",
  "#f472b6",
  "#34d399",
  "#fbbf24",
  "#a78bfa",
  "#fb923c",
  "#22d3ee",
  "#f87171",
  "#a3e635",
  "#e879f9",
];

/** Nodes graphify could not assign to a community. */
const UNCLUSTERED_COLOR = "#94a3b8";

export function communityColor(communityId: number | null): string {
  if (communityId === null) return UNCLUSTERED_COLOR;
  const index = Math.abs(communityId) % COMMUNITY_COLORS.length;
  return COMMUNITY_COLORS[index] ?? UNCLUSTERED_COLOR;
}

/**
 * How an edge is drawn, by provenance.
 *
 * The whole point of graphify's audit trail is that an INFERRED edge is a
 * guess. Rendering it identically to an EXTRACTED one would launder the guess
 * into a fact, so the difference is encoded twice — in weight and in opacity —
 * rather than in hue alone, which colour-blind users cannot rely on.
 */
export const EDGE_STYLES: Record<
  GraphConfidence,
  { readonly color: string; readonly size: number; readonly label: string }
> = {
  EXTRACTED: { color: "#64748b", size: 1.6, label: "Extracted from the code" },
  INFERRED: { color: "#7c8aa0", size: 0.9, label: "Inferred — not stated in the code" },
  AMBIGUOUS: { color: "#8b7ba0", size: 0.6, label: "Ambiguous — low confidence" },
};

/**
 * Node radius from degree.
 *
 * Logarithmic: a god node can have hundreds of times the degree of a leaf, and
 * a linear scale would render everything else as a dot.
 */
export function nodeSize(degree: number): number {
  return 3 + Math.min(12, Math.log2(degree + 1) * 2);
}
