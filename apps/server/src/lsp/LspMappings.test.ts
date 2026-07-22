import { describe, expect, it } from "vite-plus/test";

import {
  documentUri,
  mapCompletion,
  mapDiagnostics,
  mapHover,
  mapLocations,
  mapWorkspaceEdit,
  uriToLocationPath,
} from "./LspMappings.ts";

const ROOT = "/repo";

describe("uriToLocationPath", () => {
  it("maps workspace uris to relative paths", () => {
    expect(uriToLocationPath(ROOT, "file:///repo/src/index.ts")).toEqual({
      relativePath: "src/index.ts",
    });
  });

  it("maps outside uris to absolute paths", () => {
    expect(uriToLocationPath(ROOT, "file:///usr/lib/node/globals.d.ts")).toEqual({
      absolutePath: "/usr/lib/node/globals.d.ts",
    });
  });

  it("round-trips documentUri", () => {
    expect(uriToLocationPath(ROOT, documentUri(ROOT, "src/a b.ts"))).toEqual({
      relativePath: "src/a b.ts",
    });
  });
});

describe("mapCompletion", () => {
  it("maps lists with text edits and honors the item cap flag", () => {
    const result = mapCompletion({
      isIncomplete: false,
      items: [
        {
          label: "useMemo",
          kind: 3,
          detail: "React hook",
          documentation: { kind: "markdown", value: "Memoizes." },
          textEdit: {
            range: { start: { line: 0, character: 0 }, end: { line: 0, character: 3 } },
            newText: "useMemo",
          },
          sortText: "11",
        },
      ],
    });
    expect(result.isIncomplete).toBe(false);
    expect(result.items).toHaveLength(1);
    const item = result.items[0]!;
    expect(item).toMatchObject({
      label: "useMemo",
      kind: 3,
      detail: "React hook",
      documentation: "Memoizes.",
      insertText: "useMemo",
      sortText: "11",
      range: { start: { line: 0, character: 0 }, end: { line: 0, character: 3 } },
    });
    // The opaque resolve payload round-trips the original protocol item.
    expect(JSON.parse(item.resolveData!)).toMatchObject({ label: "useMemo", kind: 3 });
  });

  it("returns empty for null results", () => {
    expect(mapCompletion(null)).toEqual({ items: [], isIncomplete: false });
  });
});

describe("mapHover", () => {
  it("joins marked strings into markdown", () => {
    expect(
      mapHover({
        contents: [{ language: "typescript", value: "const a: number" }, "docs here"],
      }),
    ).toEqual({ contents: "```typescript\nconst a: number\n```\n\ndocs here" });
  });

  it("returns null for empty hovers", () => {
    expect(mapHover(null)).toBe(null);
    expect(mapHover({ contents: "" })).toBe(null);
  });
});

describe("mapLocations", () => {
  it("normalizes location links using the selection range", () => {
    const locations = mapLocations(ROOT, [
      {
        targetUri: "file:///repo/src/def.ts",
        targetRange: { start: { line: 0, character: 0 }, end: { line: 10, character: 0 } },
        targetSelectionRange: { start: { line: 2, character: 4 }, end: { line: 2, character: 9 } },
      },
    ]);
    expect(locations).toEqual([
      {
        relativePath: "src/def.ts",
        range: { start: { line: 2, character: 4 }, end: { line: 2, character: 9 } },
      },
    ]);
  });
});

describe("mapWorkspaceEdit", () => {
  it("merges changes and documentChanges per file, skipping outside-workspace edits", () => {
    const edit = mapWorkspaceEdit(ROOT, {
      changes: {
        "file:///repo/src/a.ts": [
          {
            range: { start: { line: 1, character: 0 }, end: { line: 1, character: 3 } },
            newText: "foo",
          },
        ],
        "file:///elsewhere/b.ts": [
          {
            range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } },
            newText: "x",
          },
        ],
      },
      documentChanges: [
        {
          textDocument: { uri: "file:///repo/src/a.ts", version: 3 },
          edits: [
            {
              range: { start: { line: 5, character: 0 }, end: { line: 5, character: 2 } },
              newText: "bar",
            },
          ],
        },
      ],
    });
    expect(edit).toEqual([
      {
        relativePath: "src/a.ts",
        edits: [
          {
            range: { start: { line: 1, character: 0 }, end: { line: 1, character: 3 } },
            newText: "foo",
          },
          {
            range: { start: { line: 5, character: 0 }, end: { line: 5, character: 2 } },
            newText: "bar",
          },
        ],
      },
    ]);
  });
});

describe("mapDiagnostics", () => {
  it("defaults severity to error and stringifies codes", () => {
    expect(
      mapDiagnostics([
        {
          range: { start: { line: 0, character: 0 }, end: { line: 0, character: 5 } },
          message: "boom",
          code: 2304,
          source: "ts",
        },
      ]),
    ).toEqual([
      {
        range: { start: { line: 0, character: 0 }, end: { line: 0, character: 5 } },
        severity: 1,
        message: "boom",
        source: "ts",
        code: "2304",
      },
    ]);
  });
});
