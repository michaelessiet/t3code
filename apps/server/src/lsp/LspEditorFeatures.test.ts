import { describe, expect, it } from "vite-plus/test";
import { fullDocumentRange, mapCodeAction, mapSemanticTokens } from "./LspEditorFeatures.ts";
import { documentUri } from "./LspMappings.ts";

const range = { start: { line: 0, character: 0 }, end: { line: 0, character: 3 } };
const edit = { range, newText: "new" };
it("replaces the old full buffer using UTF-16 positions and every line-ending form", () => {
  expect(fullDocumentRange("").end).toEqual({ line: 0, character: 0 });
  expect(fullDocumentRange("😀").end).toEqual({ line: 0, character: 2 });
  expect(fullDocumentRange("a\r\nb\rc\n😀").end).toEqual({ line: 3, character: 2 });
  expect(fullDocumentRange("a\n").end).toEqual({ line: 1, character: 0 });
});
describe("safe code action previews", () => {
  it("allows bundled TypeScript edit-only fixes without executing bookkeeping commands", () => {
    const action = {
      title: "Convert const to let",
      edit: { changes: { [documentUri("/work", "a.ts")]: [edit] } },
      command: {
        title: "",
        command: "_typescript.applyCodeActionCommand",
        arguments: [{ fixName: "fixConvertConstToLet" }, {}, documentUri("/work", "a.ts")],
      },
    };
    expect(mapCodeAction("/work", action).disabledReason).toBeUndefined();
    expect(mapCodeAction("/work", action).files).toHaveLength(1);
    expect(
      mapCodeAction("/work", {
        ...action,
        command: {
          ...action.command,
          arguments: [
            { fixName: "install", commands: [{ type: "install package" }] },
            {},
            documentUri("/work", "a.ts"),
          ],
        },
      }).disabledReason,
    ).toContain("command");
    expect(
      mapCodeAction("/work", {
        title: "Extract",
        data: { id: 1 },
        command: { title: "", command: "_typescript.didApplyRefactoring" },
      }).disabledReason,
    ).toBeUndefined();
  });
  it("preserves editable fixes, preferences, and resolve data", () => {
    const action = {
      title: "Rename",
      kind: "quickfix",
      isPreferred: true,
      data: { id: 1 },
      edit: { changes: { [documentUri("/work", "a.ts")]: [edit] } },
    };
    const mapped = mapCodeAction("/work", action);
    expect(mapped.disabledReason).toBeUndefined();
    expect(mapped.preferred).toBe(true);
    expect(mapped.files).toEqual([{ relativePath: "a.ts", edits: [edit] }]);
    expect(JSON.parse(mapped.resolveData)).toEqual(action);
  });
  it("does not partially apply resource operations or outside-workspace edits", () => {
    for (const action of [
      { title: "Outside", edit: { changes: { [documentUri("/other", "a.ts")]: [edit] } } },
      {
        title: "Delete",
        edit: { documentChanges: [{ kind: "delete" as const, uri: documentUri("/work", "a.ts") }] },
      },
    ]) {
      expect(mapCodeAction("/work", action).disabledReason).toBeDefined();
      expect(mapCodeAction("/work", action).files).toEqual([]);
    }
  });
  it("does not execute command-only actions or ignore disabled reasons", () => {
    expect(
      mapCodeAction("/work", { title: "Run", command: "arbitrary.command" }).disabledReason,
    ).toContain("command");
    expect(
      mapCodeAction("/work", { title: "Disabled", disabled: { reason: "not applicable" } })
        .disabledReason,
    ).toBe("not applicable");
  });
});
describe("semantic tokens", () => {
  const legend = {
    tokenTypes: ["function", "variable"],
    tokenModifiers: ["declaration", "readonly"],
  };
  it("decodes UTF-16 deltas, line resets and modifier bits", () => {
    expect(
      mapSemanticTokens({ data: [0, 4, 3, 0, 1, 0, 5, 2, 1, 2, 2, 1, 4, 1, 0] }, legend),
    ).toEqual([
      {
        range: { start: { line: 0, character: 4 }, end: { line: 0, character: 7 } },
        kind: "function",
        modifiers: ["declaration"],
      },
      {
        range: { start: { line: 0, character: 9 }, end: { line: 0, character: 11 } },
        kind: "variable",
        modifiers: ["readonly"],
      },
      {
        range: { start: { line: 2, character: 1 }, end: { line: 2, character: 5 } },
        kind: "variable",
        modifiers: [],
      },
    ]);
  });
  it("rejects malformed frames and safely skips unknown types", () => {
    expect(() => mapSemanticTokens({ data: [0, 1] }, legend)).toThrow();
    expect(() => mapSemanticTokens({ data: [-1, 0, 1, 0, 0] }, legend)).toThrow();
    expect(mapSemanticTokens({ data: [0, 0, 1, 99, 0] }, legend)).toEqual([]);
    expect(mapSemanticTokens(null, legend)).toEqual([]);
  });
});
