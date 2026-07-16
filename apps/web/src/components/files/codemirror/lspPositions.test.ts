import { Text } from "@codemirror/state";
import { describe, expect, it } from "vite-plus/test";

import { lspPositionToOffset, lspRangeToOffsets, offsetToLspPosition } from "./lspPositions";

const doc = Text.of(["const a = 1;", "const bb = 2;", ""]);

describe("lspPositionToOffset", () => {
  it("maps line/character to offsets", () => {
    expect(lspPositionToOffset(doc, { line: 0, character: 6 })).toBe(6);
    expect(lspPositionToOffset(doc, { line: 1, character: 6 })).toBe(19);
  });

  it("clamps beyond-end positions", () => {
    expect(lspPositionToOffset(doc, { line: 0, character: 999 })).toBe(12);
    expect(lspPositionToOffset(doc, { line: 99, character: 0 })).toBe(doc.length);
  });
});

describe("offsetToLspPosition", () => {
  it("round-trips positions", () => {
    expect(offsetToLspPosition(doc, 19)).toEqual({ line: 1, character: 6 });
    expect(offsetToLspPosition(doc, 0)).toEqual({ line: 0, character: 0 });
  });

  it("clamps out-of-range offsets", () => {
    expect(offsetToLspPosition(doc, 9999).line).toBe(2);
  });
});

describe("lspRangeToOffsets", () => {
  it("orders inverted ranges", () => {
    const range = {
      start: { line: 1, character: 5 },
      end: { line: 0, character: 2 },
    };
    const offsets = lspRangeToOffsets(doc, range);
    expect(offsets.to).toBeGreaterThanOrEqual(offsets.from);
  });
});
