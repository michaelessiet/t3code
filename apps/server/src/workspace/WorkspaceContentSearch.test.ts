import * as NodeServices from "@effect/platform-node/NodeServices";
import { it, describe, expect } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Path from "effect/Path";

import * as WorkspaceContentSearch from "./WorkspaceContentSearch.ts";
import * as WorkspacePaths from "./WorkspacePaths.ts";

const TestLayer = WorkspaceContentSearch.layer.pipe(
  Layer.provide(WorkspacePaths.layer),
  Layer.provideMerge(NodeServices.layer),
);

const makeTempDir = Effect.gen(function* () {
  const fileSystem = yield* FileSystem.FileSystem;
  return yield* fileSystem.makeTempDirectoryScoped({
    prefix: "t3code-content-search-",
  });
});

const writeTextFile = Effect.fn("writeTextFile")(function* (
  cwd: string,
  relativePath: string,
  contents: string,
) {
  const fileSystem = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;
  const absolutePath = path.join(cwd, relativePath);
  yield* fileSystem
    .makeDirectory(path.dirname(absolutePath), { recursive: true })
    .pipe(Effect.orDie);
  yield* fileSystem.writeFileString(absolutePath, contents).pipe(Effect.orDie);
});

describe("ripgrepArguments", () => {
  it("defaults to literal smart-case search", () => {
    const args = WorkspaceContentSearch.ripgrepArguments({ cwd: "/repo", query: "foo" });
    expect(args).toContain("--fixed-strings");
    expect(args).toContain("--smart-case");
    expect(args.slice(-3)).toEqual(["--", "foo", "."]);
  });

  it("maps regex, case, word, and glob options", () => {
    const args = WorkspaceContentSearch.ripgrepArguments({
      cwd: "/repo",
      query: "fo+",
      regex: true,
      caseSensitive: true,
      wholeWord: true,
      includeGlob: "src/**",
      excludeGlob: "*.test.ts",
    });
    expect(args).not.toContain("--fixed-strings");
    expect(args).toContain("--case-sensitive");
    expect(args).toContain("--word-regexp");
    expect(args).toEqual(expect.arrayContaining(["--glob", "src/**"]));
    expect(args).toEqual(expect.arrayContaining(["--glob", "!*.test.ts"]));
  });
});

describe("byteOffsetToCharOffset", () => {
  it("is identity for ASCII", () => {
    expect(WorkspaceContentSearch.byteOffsetToCharOffset("abcdef", 3)).toBe(3);
  });

  it("accounts for multi-byte UTF-8 sequences", () => {
    // "é" is 2 bytes in UTF-8, 1 UTF-16 unit.
    expect(WorkspaceContentSearch.byteOffsetToCharOffset("éabc", 2)).toBe(1);
    expect(WorkspaceContentSearch.byteOffsetToCharOffset("éabc", 3)).toBe(2);
  });

  it("clamps out-of-range offsets", () => {
    expect(WorkspaceContentSearch.byteOffsetToCharOffset("ab", 99)).toBe(2);
    expect(WorkspaceContentSearch.byteOffsetToCharOffset("ab", -1)).toBe(0);
  });
});

describe("normalizeMatchLine", () => {
  it("strips trailing newlines and keeps short lines intact", () => {
    expect(WorkspaceContentSearch.normalizeMatchLine("const a = 1;\n", 6, 7)).toEqual({
      lineText: "const a = 1;",
      lineTruncated: false,
      matchStart: 6,
      matchEnd: 7,
    });
  });

  it("windows over-long lines around the match", () => {
    const longLine = `${"x".repeat(900)}NEEDLE${"y".repeat(200)}`;
    const result = WorkspaceContentSearch.normalizeMatchLine(longLine, 900, 906);
    expect(result.lineTruncated).toBe(true);
    expect(result.lineText.length).toBeLessThanOrEqual(500);
    expect(result.lineText.slice(result.matchStart, result.matchEnd)).toBe("NEEDLE");
  });
});

describe("parseRipgrepEventLine", () => {
  it("converts match events into workspace matches", () => {
    const event = JSON.stringify({
      type: "match",
      data: {
        path: { text: "./src/index.ts" },
        line_number: 3,
        lines: { text: "const answer = 42;\n" },
        submatches: [{ match: { text: "answer" }, start: 6, end: 12 }],
      },
    });
    expect(WorkspaceContentSearch.parseRipgrepEventLine(event)).toEqual([
      {
        path: "src/index.ts",
        line: 3,
        lineText: "const answer = 42;",
        lineTruncated: false,
        matchStart: 6,
        matchEnd: 12,
      },
    ]);
  });

  it("ignores non-match events and malformed lines", () => {
    expect(
      WorkspaceContentSearch.parseRipgrepEventLine(JSON.stringify({ type: "begin", data: {} })),
    ).toEqual([]);
    expect(WorkspaceContentSearch.parseRipgrepEventLine("not json")).toEqual([]);
  });
});

it.layer(TestLayer, { excludeTestServices: true })("WorkspaceContentSearchLive", (it) => {
  describe("search", () => {
    it.effect("finds literal matches across files", () =>
      Effect.gen(function* () {
        const contentSearch = yield* WorkspaceContentSearch.WorkspaceContentSearch;
        const cwd = yield* makeTempDir;
        yield* writeTextFile(cwd, "src/a.ts", "export const alpha = 'needle';\n");
        yield* writeTextFile(cwd, "src/b.ts", "// no match here\n");
        yield* writeTextFile(cwd, "docs/notes.md", "another needle appears\n");

        const result = yield* contentSearch.search({ cwd, query: "needle" });

        expect(result.truncated).toBe(false);
        expect(result.fileCount).toBe(2);
        expect(result.matches.map((match) => match.path).sort()).toEqual([
          "docs/notes.md",
          "src/a.ts",
        ]);
        const first = result.matches.find((match) => match.path === "src/a.ts")!;
        expect(first.lineText.slice(first.matchStart, first.matchEnd)).toBe("needle");
      }),
    );

    it.effect("supports regex searches", () =>
      Effect.gen(function* () {
        const contentSearch = yield* WorkspaceContentSearch.WorkspaceContentSearch;
        const cwd = yield* makeTempDir;
        yield* writeTextFile(cwd, "src/a.ts", "const value1 = 1;\nconst value22 = 2;\n");

        const result = yield* contentSearch.search({
          cwd,
          query: String.raw`value\d+`,
          regex: true,
        });

        expect(result.matches).toHaveLength(2);
      }),
    );

    it.effect("honors include globs", () =>
      Effect.gen(function* () {
        const contentSearch = yield* WorkspaceContentSearch.WorkspaceContentSearch;
        const cwd = yield* makeTempDir;
        yield* writeTextFile(cwd, "src/a.ts", "needle\n");
        yield* writeTextFile(cwd, "docs/a.md", "needle\n");

        const result = yield* contentSearch.search({
          cwd,
          query: "needle",
          includeGlob: "src/**",
        });

        expect(result.matches.map((match) => match.path)).toEqual(["src/a.ts"]);
      }),
    );

    it.effect("caps results and reports truncation", () =>
      Effect.gen(function* () {
        const contentSearch = yield* WorkspaceContentSearch.WorkspaceContentSearch;
        const cwd = yield* makeTempDir;
        const lines = Array.from({ length: 40 }, (_, index) => `needle ${index}`).join("\n");
        yield* writeTextFile(cwd, "src/a.txt", `${lines}\n`);

        const result = yield* contentSearch.search({ cwd, query: "needle", maxResults: 10 });

        expect(result.matches).toHaveLength(10);
        expect(result.truncated).toBe(true);
      }),
    );

    it.effect("rejects invalid regex patterns", () =>
      Effect.gen(function* () {
        const contentSearch = yield* WorkspaceContentSearch.WorkspaceContentSearch;
        const cwd = yield* makeTempDir;
        yield* writeTextFile(cwd, "src/a.ts", "content\n");

        const error = yield* contentSearch
          .search({ cwd, query: "([unclosed", regex: true })
          .pipe(Effect.flip);

        expect(error._tag).toBe("WorkspaceContentSearchInvalidPatternError");
      }),
    );

    it.effect("returns empty results when nothing matches", () =>
      Effect.gen(function* () {
        const contentSearch = yield* WorkspaceContentSearch.WorkspaceContentSearch;
        const cwd = yield* makeTempDir;
        yield* writeTextFile(cwd, "src/a.ts", "content\n");

        const result = yield* contentSearch.search({ cwd, query: "zzz-not-present" });

        expect(result).toEqual({ matches: [], fileCount: 0, truncated: false });
      }),
    );
  });
});
