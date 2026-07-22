import { describe, expect, it } from "vite-plus/test";

import {
  buildSearchRegExp,
  computeReplacements,
  escapeRegExpLiteral,
  groupMatchesByFile,
  isEffectivelyCaseSensitive,
  matchLineSegments,
  splitSearchResultPath,
} from "./SearchPanel.logic";

describe("escapeRegExpLiteral", () => {
  it("escapes every regex metacharacter", () => {
    const literal = "a.b*c+d?e(f)g[h]i{j}k|l^m$n\\o";
    const pattern = new RegExp(escapeRegExpLiteral(literal));
    expect(pattern.test(literal)).toBe(true);
    expect(pattern.test("aXb*c+d?e(f)g[h]i{j}k|l^m$n\\o")).toBe(false);
  });
});

describe("isEffectivelyCaseSensitive", () => {
  it("uses smart case by default", () => {
    expect(isEffectivelyCaseSensitive({ query: "foo" })).toBe(false);
    expect(isEffectivelyCaseSensitive({ query: "Foo" })).toBe(true);
  });

  it("honors an explicit caseSensitive flag", () => {
    expect(isEffectivelyCaseSensitive({ query: "foo", caseSensitive: true })).toBe(true);
  });
});

describe("buildSearchRegExp", () => {
  it("returns null for empty queries", () => {
    expect(buildSearchRegExp({ query: "" })).toBeNull();
  });

  it("returns null for invalid regex-mode queries", () => {
    expect(buildSearchRegExp({ query: "foo(", regex: true })).toBeNull();
  });

  it("treats literal queries literally", () => {
    const pattern = buildSearchRegExp({ query: "a.b" });
    expect(pattern?.test("a.b")).toBe(true);
    expect(buildSearchRegExp({ query: "a.b" })?.test("axb")).toBe(false);
  });

  it("matches case-insensitively for lowercase queries", () => {
    expect(buildSearchRegExp({ query: "foo" })?.test("FOO")).toBe(true);
    expect(buildSearchRegExp({ query: "foo", caseSensitive: true })?.test("FOO")).toBe(false);
  });

  it("restricts whole-word queries to word boundaries", () => {
    const pattern = buildSearchRegExp({ query: "cat", wholeWord: true });
    expect(pattern?.test("the cat sat")).toBe(true);
    expect(buildSearchRegExp({ query: "cat", wholeWord: true })?.test("concatenate")).toBe(false);
  });
});

describe("computeReplacements", () => {
  it("returns null for invalid patterns", () => {
    expect(computeReplacements("foo", { query: "(", regex: true }, "bar")).toBeNull();
    expect(computeReplacements("foo", { query: "" }, "bar")).toBeNull();
  });

  it("replaces all literal matches and counts them", () => {
    expect(computeReplacements("foo bar foo", { query: "foo" }, "baz")).toEqual({
      contents: "baz bar baz",
      replacedCount: 2,
    });
  });

  it("returns unchanged contents when nothing matches", () => {
    expect(computeReplacements("foo", { query: "missing" }, "bar")).toEqual({
      contents: "foo",
      replacedCount: 0,
    });
  });

  it("keeps dollar signs literal in literal mode", () => {
    expect(computeReplacements("price", { query: "price" }, "$&100")).toEqual({
      contents: "$&100",
      replacedCount: 1,
    });
  });

  it("replaces smart-case-insensitive matches across lines", () => {
    expect(computeReplacements("Foo\nfoo\nFOO", { query: "foo" }, "bar")).toEqual({
      contents: "bar\nbar\nbar",
      replacedCount: 3,
    });
  });

  it("respects whole-word matching", () => {
    expect(computeReplacements("cat concat cat", { query: "cat", wholeWord: true }, "dog")).toEqual(
      {
        contents: "dog concat dog",
        replacedCount: 2,
      },
    );
  });

  it("expands capture groups in regex mode", () => {
    expect(
      computeReplacements(
        'import a from "./a";\nimport b from "./b";',
        { query: 'from "\\./(\\w+)"', regex: true },
        'from "~/lib/$1"',
      ),
    ).toEqual({
      contents: 'import a from "~/lib/a";\nimport b from "~/lib/b";',
      replacedCount: 2,
    });
  });

  it("expands $& in regex mode", () => {
    expect(computeReplacements("foo", { query: "foo", regex: true }, "[$&]")).toEqual({
      contents: "[foo]",
      replacedCount: 1,
    });
  });

  it("wraps regex alternations before applying whole-word boundaries", () => {
    expect(
      computeReplacements(
        "cat dog catdog",
        { query: "cat|dog", regex: true, wholeWord: true },
        "x",
      ),
    ).toEqual({
      contents: "x x catdog",
      replacedCount: 2,
    });
  });

  it("terminates on zero-length regex matches", () => {
    expect(computeReplacements("ab", { query: "x*", regex: true }, "-")).toEqual({
      contents: "-a-b-",
      replacedCount: 3,
    });
  });
});

describe("groupMatchesByFile", () => {
  const match = (path: string, line: number) => ({
    path,
    line,
    lineText: "text",
    lineTruncated: false,
    matchStart: 0,
    matchEnd: 4,
  });

  it("groups matches by path preserving order", () => {
    expect(groupMatchesByFile([match("a.ts", 1), match("b.ts", 2), match("a.ts", 3)])).toEqual([
      { path: "a.ts", matches: [match("a.ts", 1), match("a.ts", 3)] },
      { path: "b.ts", matches: [match("b.ts", 2)] },
    ]);
  });

  it("returns no groups for no matches", () => {
    expect(groupMatchesByFile([])).toEqual([]);
  });
});

describe("matchLineSegments", () => {
  it("splits the line around the match", () => {
    expect(matchLineSegments({ lineText: "const foo = 1;", matchStart: 6, matchEnd: 9 })).toEqual({
      before: "const ",
      matched: "foo",
      after: " = 1;",
      beforeClipped: false,
    });
  });

  it("drops leading whitespace from the prefix", () => {
    expect(matchLineSegments({ lineText: "        foo()", matchStart: 8, matchEnd: 11 })).toEqual({
      before: "",
      matched: "foo",
      after: "()",
      beforeClipped: false,
    });
  });

  it("clips long prefixes so the match stays visible", () => {
    const prefix = "x".repeat(80);
    const segments = matchLineSegments({
      lineText: `${prefix}match`,
      matchStart: 80,
      matchEnd: 85,
    });
    expect(segments.before).toBe("x".repeat(32));
    expect(segments.matched).toBe("match");
    expect(segments.beforeClipped).toBe(true);
  });

  it("clamps offsets beyond a truncated line", () => {
    expect(matchLineSegments({ lineText: "short", matchStart: 3, matchEnd: 99 })).toEqual({
      before: "sho",
      matched: "rt",
      after: "",
      beforeClipped: false,
    });
    expect(matchLineSegments({ lineText: "short", matchStart: 9, matchEnd: 12 })).toEqual({
      before: "short",
      matched: "",
      after: "",
      beforeClipped: false,
    });
  });
});

describe("splitSearchResultPath", () => {
  it("splits directory and file name", () => {
    expect(splitSearchResultPath("src/lib/utils.ts")).toEqual({
      name: "utils.ts",
      directory: "src/lib",
    });
  });

  it("handles root-level paths", () => {
    expect(splitSearchResultPath("README.md")).toEqual({ name: "README.md", directory: "" });
  });
});
