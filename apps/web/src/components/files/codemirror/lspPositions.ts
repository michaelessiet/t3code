import type { Text } from "@codemirror/state";
import type { LspPosition, LspRange } from "@t3tools/contracts";

/** Convert an LSP position (0-based line/char) to a CM6 doc offset, clamped. */
export function lspPositionToOffset(doc: Text, position: LspPosition): number {
  const lineNumber = Math.min(position.line + 1, doc.lines);
  const line = doc.line(lineNumber);
  return Math.min(line.from + position.character, line.to);
}

/** Convert a CM6 doc offset to an LSP position. */
export function offsetToLspPosition(doc: Text, offset: number): LspPosition {
  const clamped = Math.max(0, Math.min(offset, doc.length));
  const line = doc.lineAt(clamped);
  return { line: line.number - 1, character: clamped - line.from };
}

/** Convert an LSP range to CM6 [from, to] offsets, clamped and ordered. */
export function lspRangeToOffsets(doc: Text, range: LspRange): { from: number; to: number } {
  const from = lspPositionToOffset(doc, range.start);
  const to = Math.max(from, lspPositionToOffset(doc, range.end));
  return { from, to };
}
