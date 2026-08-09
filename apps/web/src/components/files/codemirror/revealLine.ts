import type { Extension } from "@codemirror/state";
import { StateEffect, StateField } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView } from "@codemirror/view";

const setRevealedLinesEffect = StateEffect.define<{ start: number; end: number } | null>();

const revealedLineDecoration = Decoration.line({ class: "cm-reveal-line" });

/** Highlighting an enormous range would only bury the start the user jumped to. */
const MAX_REVEALED_LINES = 500;

const revealedLineField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update: (decorations, transaction) => {
    let next = decorations.map(transaction.changes);
    for (const effect of transaction.effects) {
      if (!effect.is(setRevealedLinesEffect)) continue;
      if (effect.value === null) {
        next = Decoration.none;
        continue;
      }
      const { start, end } = effect.value;
      const lastLine = Math.min(end, start + MAX_REVEALED_LINES - 1);
      const ranges = [];
      for (let lineNumber = start; lineNumber <= lastLine; lineNumber += 1) {
        ranges.push(revealedLineDecoration.range(transaction.state.doc.line(lineNumber).from));
      }
      next = Decoration.set(ranges);
    }
    return next;
  },
  provide: (field) => EditorView.decorations.from(field),
});

export const revealLineExtension: Extension = revealedLineField;

function clampLineNumber(view: EditorView, line: number): number {
  return Math.min(Math.max(1, line), view.state.doc.lines);
}

/**
 * Scroll `line` into the vertical center of the viewport, highlight it, and
 * place the cursor on it, replacing any previous reveal highlight. `null`
 * clears the highlight. Moving the selection keeps keyboard navigation
 * anchored to the revealed line — a reveal always comes from an explicit
 * jump (search match, go-to-definition, file link), where typing or vim
 * motions should continue from the target, not from line 1.
 *
 * With `endLine`, every line of the range is highlighted and the selection
 * spans it, anchored at the start so the viewport and subsequent motions
 * stay on the line the link named first.
 */
export function revealEditorLine(
  view: EditorView,
  line: number | null,
  endLine?: number | null,
): void {
  if (line === null) {
    view.dispatch({ effects: setRevealedLinesEffect.of(null) });
    return;
  }
  const clampedLine = clampLineNumber(view, line);
  const clampedEndLine =
    endLine !== undefined && endLine !== null
      ? Math.max(clampLineNumber(view, endLine), clampedLine)
      : clampedLine;
  const lineStart = view.state.doc.line(clampedLine).from;
  view.dispatch({
    selection: {
      anchor: lineStart,
      head: clampedEndLine > clampedLine ? view.state.doc.line(clampedEndLine).to : lineStart,
    },
    effects: [
      setRevealedLinesEffect.of({ start: clampedLine, end: clampedEndLine }),
      EditorView.scrollIntoView(lineStart, { y: "center" }),
    ],
  });
}
