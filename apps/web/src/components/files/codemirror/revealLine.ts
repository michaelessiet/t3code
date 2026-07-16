import type { Extension } from "@codemirror/state";
import { StateEffect, StateField } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView } from "@codemirror/view";

const setRevealedLineEffect = StateEffect.define<number | null>();

const revealedLineDecoration = Decoration.line({ class: "cm-reveal-line" });

const revealedLineField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update: (decorations, transaction) => {
    let next = decorations.map(transaction.changes);
    for (const effect of transaction.effects) {
      if (!effect.is(setRevealedLineEffect)) continue;
      next =
        effect.value === null
          ? Decoration.none
          : Decoration.set([
              revealedLineDecoration.range(transaction.state.doc.line(effect.value).from),
            ]);
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
 * Scroll `line` into the vertical center of the viewport and highlight it,
 * replacing any previous reveal highlight. `null` clears the highlight.
 */
export function revealEditorLine(view: EditorView, line: number | null): void {
  if (line === null) {
    view.dispatch({ effects: setRevealedLineEffect.of(null) });
    return;
  }
  const clampedLine = clampLineNumber(view, line);
  view.dispatch({
    effects: [
      setRevealedLineEffect.of(clampedLine),
      EditorView.scrollIntoView(view.state.doc.line(clampedLine).from, { y: "center" }),
    ],
  });
}
