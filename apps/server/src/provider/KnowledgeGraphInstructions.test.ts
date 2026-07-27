import { describe, expect, it } from "@effect/vitest";

import { knowledgeGraphNote } from "./KnowledgeGraphInstructions.ts";

const HOUR = 60 * 60 * 1000;
const DAY = 24 * HOUR;

const note = (overrides?: Partial<Parameters<typeof knowledgeGraphNote>[0]>) =>
  knowledgeGraphNote({
    branch: "main",
    nodeCount: 49_692,
    edgeCount: 105_249,
    builtAt: 1_000_000,
    now: 1_000_000 + HOUR,
    ...overrides,
  });

describe("knowledgeGraphNote", () => {
  it("names the tools, the branch and the size, so the model can judge for itself", () => {
    const text = note();
    expect(text).toContain("graph_*");
    expect(text).toContain("`main` branch");
    expect(text).toContain("49,692 nodes");
    expect(text).toContain("105,249 edges");
  });

  // The whole point of the softer wording: grep is the right answer to most
  // questions about a codebase, and a note that displaced it would make the
  // agent worse at the majority of its work to help with a minority.
  it("does not tell the model to prefer the graph over ordinary search", () => {
    const text = note().toLowerCase();
    expect(text).not.toContain("prefer");
    expect(text).not.toContain("do not switch");
    expect(text).not.toContain("first call");
    expect(text).toContain("ordinary search and file reads remain the better tool");
  });

  it("describes a detached HEAD without inventing a branch name", () => {
    const text = note({ branch: null });
    expect(text).toContain("a detached HEAD");
    expect(text).not.toContain("branch`");
  });

  it.each([
    [30 * 60 * 1000, "less than an hour ago"],
    [HOUR, "about an hour ago"],
    [5 * HOUR, "about 5 hours ago"],
    [DAY, "yesterday"],
    [3 * DAY, "3 days ago"],
    [45 * DAY, "about a month ago"],
  ])("renders an age of %ims as %s", (ageMs, expected) => {
    expect(note({ now: 1_000_000 + ageMs })).toContain(expected);
  });

  // A live minute count would change the system prompt on every request and
  // invalidate the prompt cache for no benefit.
  it("is byte-identical across two nearby requests, so the prompt cache holds", () => {
    const first = note({ now: 1_000_000 + 3 * HOUR });
    const second = note({ now: 1_000_000 + 3 * HOUR + 90_000 });
    expect(second).toBe(first);
  });

  it("warns about drift only once the graph is genuinely old", () => {
    expect(note({ now: 1_000_000 + 6 * DAY })).not.toContain("not been rebuilt");
    expect(note({ now: 1_000_000 + 8 * DAY })).toContain("not been rebuilt in a while");
  });

  // Clock skew between the build and the read must not produce "in -3 days".
  it("clamps a build timestamp in the future rather than going negative", () => {
    expect(note({ now: 500_000 })).toContain("less than an hour ago");
  });
});
