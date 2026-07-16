import { describe, expect, it } from "vite-plus/test";

import { computeDocReplacement } from "./docReplacement";

function applyReplacement(current: string, next: string): string {
  const replacement = computeDocReplacement(current, next);
  if (replacement === null) return current;
  return current.slice(0, replacement.from) + replacement.insert + current.slice(replacement.to);
}

describe("computeDocReplacement", () => {
  it("returns null for identical documents", () => {
    expect(computeDocReplacement("same", "same")).toBeNull();
    expect(computeDocReplacement("", "")).toBeNull();
  });

  it("replaces only the changed middle span", () => {
    expect(computeDocReplacement("const a = 1;", "const b = 1;")).toEqual({
      from: 6,
      to: 7,
      insert: "b",
    });
  });

  it("handles pure insertions and deletions", () => {
    expect(computeDocReplacement("ab", "axb")).toEqual({ from: 1, to: 1, insert: "x" });
    expect(computeDocReplacement("axb", "ab")).toEqual({ from: 1, to: 2, insert: "" });
    expect(computeDocReplacement("", "new file")).toEqual({ from: 0, to: 0, insert: "new file" });
    expect(computeDocReplacement("old", "")).toEqual({ from: 0, to: 3, insert: "" });
  });

  it("does not overlap prefix and suffix on repeated content", () => {
    const cases: Array<[string, string]> = [
      ["aaa", "aa"],
      ["aa", "aaa"],
      ["abab", "ab"],
      ["line\nline\n", "line\n"],
      ["xyx", "xx"],
    ];
    for (const [current, next] of cases) {
      expect(applyReplacement(current, next)).toBe(next);
    }
  });

  it("round-trips arbitrary edits", () => {
    const current = "function hello() {\n  return 1;\n}\n";
    const next = "function hello(name) {\n  console.log(name);\n  return 2;\n}\n";
    expect(applyReplacement(current, next)).toBe(next);
    expect(applyReplacement(next, current)).toBe(current);
  });
});
