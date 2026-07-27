/**
 * The knowledge-graph note appended to a provider's system prompt.
 *
 * Tool descriptions alone did not get agents to reach for the graph, and the
 * reason is not that they were unpersuasive. A description is static: it cannot
 * say whether a graph exists for *this* workspace, so declining to spend a call
 * finding out is the correct move for a model that has grep available and knows
 * it always works. That is an information gap, not an attitude problem, and it
 * is the only thing this text is here to close.
 *
 * So the note is emitted **only when a graph actually exists** for the workspace
 * being opened. In the common case — feature off, or on but never built — it
 * contributes nothing at all, which is what keeps a disabled feature free.
 *
 * It deliberately stops at availability. `T3_CODE_BROWSER_TOOL_INSTRUCTIONS` in
 * `CodexDeveloperInstructions.ts` is forceful because the preview browser has no
 * substitute: an agent that falls back to Playwright is driving the wrong
 * browser. The graph is the opposite case — grep is a genuinely good answer to
 * most questions, and a note that told the model to prefer the graph would make
 * it worse at the majority of its work in order to help with a minority. Stating
 * the facts and leaving the routing to the model is not timidity; it is the only
 * version that cannot backfire.
 *
 * @module KnowledgeGraphInstructions
 */

/** What the note needs to know about a built graph. */
export interface KnowledgeGraphNoteInput {
  /** Branch the graph was built from; null for a detached HEAD. */
  readonly branch: string | null;
  readonly nodeCount: number;
  readonly edgeCount: number;
  /** Epoch millis the graph was built. */
  readonly builtAt: number;
  /** Epoch millis now, so the function stays pure and testable. */
  readonly now: number;
}

const HOUR_MS = 60 * 60 * 1000;
const DAY_MS = 24 * HOUR_MS;

/**
 * Age in words, coarse on purpose.
 *
 * The model needs to know whether to trust the graph, and that judgement turns
 * on "today" versus "weeks ago", never on the exact hour. Rounding also keeps
 * the string stable across a session, so it does not perturb the prompt cache
 * on every request the way a live minute count would.
 */
function describeAge(ageMs: number): string {
  if (ageMs < HOUR_MS) return "less than an hour ago";
  if (ageMs < DAY_MS) {
    const hours = Math.max(1, Math.floor(ageMs / HOUR_MS));
    return hours === 1 ? "about an hour ago" : `about ${hours} hours ago`;
  }
  const days = Math.floor(ageMs / DAY_MS);
  if (days === 1) return "yesterday";
  if (days < 30) return `${days} days ago`;
  const months = Math.floor(days / 30);
  return months === 1 ? "about a month ago" : `about ${months} months ago`;
}

/**
 * Build the note, or null when there is nothing worth saying.
 *
 * Callers pass the store entry for the workspace; absence of an entry means
 * absence of a note, so the null case is handled by not calling this at all.
 */
export function knowledgeGraphNote(input: KnowledgeGraphNoteInput): string {
  const ageMs = Math.max(0, input.now - input.builtAt);
  const builtFrom = input.branch === null ? "a detached HEAD" : `the \`${input.branch}\` branch`;
  // Past a week the graph has probably drifted far enough from the working tree
  // that a confident citation from it would be a liability, so say so rather
  // than letting the age line carry the whole warning.
  const staleness =
    ageMs >= 7 * DAY_MS
      ? " It has not been rebuilt in a while, so treat anything it reports as needing confirmation against the current files."
      : "";

  return `

## T3 Code knowledge graph

A knowledge graph of this workspace already exists: ${input.nodeCount.toLocaleString("en-US")} nodes and ${input.edgeCount.toLocaleString("en-US")} edges extracted from ${builtFrom}, built ${describeAge(ageMs)}. The \`t3-code\` MCP server's \`graph_*\` tools read it, and reading it costs no file access.

It holds relationships rather than text: what depends on what, what a change would affect, and how one part of the code reaches another through files that mention neither end. Those are questions ordinary search answers slowly or not at all.

Ordinary search and file reads remain the better tool for finding a literal string, locating a file, and reading an implementation, and unlike the graph they always reflect the current working tree.${staleness}
`;
}
