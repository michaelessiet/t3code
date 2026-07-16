import type { EditorState, Extension } from "@codemirror/state";
import { RangeSet, StateEffect, StateField } from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  GutterMarker,
  ViewPlugin,
  WidgetType,
  gutterLineClass,
  keymap,
} from "@codemirror/view";

/**
 * A line-number range selected for a review comment. `start` is the drag
 * anchor and `end` the drag head, so the range may be reversed; consumers
 * normalize (see `normalizeFileCommentRange`).
 */
export interface ReviewLineRange {
  readonly start: number;
  readonly end: number;
}

/** A comment annotation group anchored below `lineNumber`. */
export interface ReviewAnnotationSpec {
  readonly id: string;
  readonly lineNumber: number;
}

export interface ReviewCommentsConfig {
  /** False while a draft comment form is open, which blocks new selections. */
  readonly isSelectionEnabled: () => boolean;
  /** Fires while dragging over the gutter as the range grows or shrinks. */
  readonly onSelectionChange: (range: ReviewLineRange | null) => void;
  /** Fires when a gutter drag ends (or Escape clears the selection). */
  readonly onSelectionEnd: (range: ReviewLineRange | null) => void;
  /**
   * Stable DOM container for an annotation group; the caller renders the
   * annotation UI into it (e.g. via a React portal) while the extension
   * mounts it below the anchored line as a block widget.
   */
  readonly getAnnotationContainer: (id: string) => HTMLElement;
  /**
   * Fires after document edits move annotation anchors, with the new line
   * number of every annotation group.
   */
  readonly onAnnotationLinesChanged: (lines: ReadonlyArray<ReviewAnnotationSpec>) => void;
}

const setSelectionEffect = StateEffect.define<ReviewLineRange | null>();
const setAnnotationsEffect = StateEffect.define<ReadonlyArray<ReviewAnnotationSpec>>();

function clampLine(state: EditorState, line: number): number {
  return Math.min(Math.max(1, line), state.doc.lines);
}

interface SelectionAnchors {
  readonly anchor: number;
  readonly head: number;
}

const selectionField = StateField.define<SelectionAnchors | null>({
  create: () => null,
  update: (selection, transaction) => {
    let next =
      selection === null
        ? null
        : {
            anchor: transaction.changes.mapPos(selection.anchor),
            head: transaction.changes.mapPos(selection.head),
          };
    for (const effect of transaction.effects) {
      if (!effect.is(setSelectionEffect)) continue;
      next =
        effect.value === null
          ? null
          : {
              anchor: transaction.state.doc.line(clampLine(transaction.state, effect.value.start))
                .from,
              head: transaction.state.doc.line(clampLine(transaction.state, effect.value.end)).from,
            };
    }
    return next;
  },
});

export function currentReviewSelection(state: EditorState): ReviewLineRange | null {
  const selection = state.field(selectionField, false) ?? null;
  if (selection === null) return null;
  return {
    start: state.doc.lineAt(selection.anchor).number,
    end: state.doc.lineAt(selection.head).number,
  };
}

function reviewRangesEqual(a: ReviewLineRange | null, b: ReviewLineRange | null): boolean {
  if (a === null || b === null) return a === b;
  return a.start === b.start && a.end === b.end;
}

/** Sync the highlighted review selection from external state; no-op when unchanged. */
export function setReviewSelection(view: EditorView, range: ReviewLineRange | null): void {
  if (reviewRangesEqual(currentReviewSelection(view.state), range)) return;
  view.dispatch({ effects: setSelectionEffect.of(range) });
}

const selectedLineDecoration = Decoration.line({ class: "cm-review-selected-line" });

const selectionDecorations = EditorView.decorations.compute(["doc", selectionField], (state) => {
  const selection = state.field(selectionField);
  if (selection === null) return Decoration.none;
  const startLine = state.doc.lineAt(Math.min(selection.anchor, selection.head)).number;
  const endLine = state.doc.lineAt(Math.max(selection.anchor, selection.head)).number;
  const decorations = [];
  for (let line = startLine; line <= endLine; line += 1) {
    decorations.push(selectedLineDecoration.range(state.doc.line(line).from));
  }
  return Decoration.set(decorations);
});

const selectedGutterMarker = new (class extends GutterMarker {
  override elementClass = "cm-review-selected-gutter";
})();

const selectionGutterHighlight = gutterLineClass.compute(["doc", selectionField], (state) => {
  const selection = state.field(selectionField);
  if (selection === null) return RangeSet.empty;
  const startLine = state.doc.lineAt(Math.min(selection.anchor, selection.head)).number;
  const endLine = state.doc.lineAt(Math.max(selection.anchor, selection.head)).number;
  const markers = [];
  for (let line = startLine; line <= endLine; line += 1) {
    markers.push(selectedGutterMarker.range(state.doc.line(line).from));
  }
  return RangeSet.of(markers);
});

class ReviewAnnotationWidget extends WidgetType {
  constructor(
    readonly id: string,
    private readonly getContainer: (id: string) => HTMLElement,
  ) {
    super();
  }

  override eq(other: ReviewAnnotationWidget): boolean {
    return other.id === this.id;
  }

  override toDOM(): HTMLElement {
    const container = this.getContainer(this.id);
    container.classList.add("cm-review-annotation");
    return container;
  }

  override ignoreEvent(): boolean {
    // The annotation UI owns its events (textarea, buttons); the editor must
    // not treat interactions inside the widget as document interactions.
    return true;
  }
}

function buildAnnotationDecorations(
  state: EditorState,
  specs: ReadonlyArray<ReviewAnnotationSpec>,
  getContainer: (id: string) => HTMLElement,
): DecorationSet {
  const decorations = specs
    .map((spec) => {
      const line = state.doc.line(clampLine(state, spec.lineNumber));
      return Decoration.widget({
        widget: new ReviewAnnotationWidget(spec.id, getContainer),
        block: true,
        side: 1,
      }).range(line.to);
    })
    .sort((a, b) => a.from - b.from);
  return Decoration.set(decorations);
}

function annotationField(config: ReviewCommentsConfig) {
  return StateField.define<DecorationSet>({
    create: () => Decoration.none,
    update: (decorations, transaction) => {
      let next = decorations.map(transaction.changes);
      for (const effect of transaction.effects) {
        if (!effect.is(setAnnotationsEffect)) continue;
        next = buildAnnotationDecorations(
          transaction.state,
          effect.value,
          config.getAnnotationContainer,
        );
      }
      return next;
    },
    provide: (field) => EditorView.decorations.from(field),
  });
}

/** Sync the annotation block widgets from external state. */
export function setReviewAnnotations(
  view: EditorView,
  specs: ReadonlyArray<ReviewAnnotationSpec>,
): void {
  view.dispatch({ effects: setAnnotationsEffect.of(specs) });
}

function currentAnnotationLines(
  state: EditorState,
  field: StateField<DecorationSet>,
): ReviewAnnotationSpec[] {
  const lines: ReviewAnnotationSpec[] = [];
  const cursor = state.field(field).iter();
  while (cursor.value !== null) {
    const widget = cursor.value.spec.widget;
    if (widget instanceof ReviewAnnotationWidget) {
      lines.push({ id: widget.id, lineNumber: state.doc.lineAt(cursor.from).number });
    }
    cursor.next();
  }
  return lines;
}

function gutterSelectionPlugin(config: ReviewCommentsConfig) {
  return ViewPlugin.define((view) => {
    let dragAnchorLine: number | null = null;
    let dragHeadLine: number | null = null;

    const lineAtEvent = (event: MouseEvent): number => {
      const pos = view.posAtCoords({ x: event.clientX, y: event.clientY }, false);
      return view.state.doc.lineAt(pos).number;
    };

    const applyDragRange = (headLine: number, done: boolean) => {
      if (dragAnchorLine === null) return;
      const range: ReviewLineRange = { start: dragAnchorLine, end: headLine };
      if (done || dragHeadLine !== headLine) {
        dragHeadLine = headLine;
        setReviewSelection(view, range);
        config.onSelectionChange(range);
      }
      if (done) config.onSelectionEnd(range);
    };

    const endDrag = () => {
      dragAnchorLine = null;
      dragHeadLine = null;
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };

    const handleMouseMove = (event: MouseEvent) => {
      applyDragRange(lineAtEvent(event), false);
    };

    const handleMouseUp = (event: MouseEvent) => {
      applyDragRange(lineAtEvent(event), true);
      endDrag();
    };

    const handleMouseDown = (event: MouseEvent) => {
      if (event.button !== 0 || !config.isSelectionEnabled()) return;
      if (!(event.target instanceof Element) || event.target.closest(".cm-gutters") === null) {
        return;
      }
      event.preventDefault();
      dragAnchorLine = lineAtEvent(event);
      applyDragRange(dragAnchorLine, false);
      window.addEventListener("mousemove", handleMouseMove);
      window.addEventListener("mouseup", handleMouseUp);
    };

    view.dom.addEventListener("mousedown", handleMouseDown);
    return {
      destroy() {
        view.dom.removeEventListener("mousedown", handleMouseDown);
        endDrag();
      },
    };
  });
}

function reviewEscapeKeymap(config: ReviewCommentsConfig) {
  return keymap.of([
    {
      key: "Escape",
      run: (view) => {
        if (!config.isSelectionEnabled()) return false;
        if (currentReviewSelection(view.state) === null) return false;
        setReviewSelection(view, null);
        config.onSelectionEnd(null);
        return true;
      },
    },
  ]);
}

/**
 * Review-comment support for the editable file surface: drag over the line
 * gutter to select a range (highlighted in the content and gutter), then a
 * comment annotation widget renders below the range's last line. Annotation
 * anchors follow document edits and are reported back via
 * `onAnnotationLinesChanged` so the owning state can remap comment ranges.
 */
export function reviewCommentsExtension(config: ReviewCommentsConfig): Extension {
  const annotations = annotationField(config);
  return [
    selectionField,
    selectionDecorations,
    selectionGutterHighlight,
    annotations,
    gutterSelectionPlugin(config),
    reviewEscapeKeymap(config),
    EditorView.updateListener.of((update) => {
      if (!update.docChanged) return;
      const lines = currentAnnotationLines(update.state, annotations);
      if (lines.length > 0) config.onAnnotationLinesChanged(lines);
    }),
  ];
}
