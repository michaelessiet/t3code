import { Chunk } from "@codemirror/merge";
import type { Extension } from "@codemirror/state";
import { RangeSet, StateEffect, StateField, Text } from "@codemirror/state";
import type { Command } from "@codemirror/view";
import {
  Decoration,
  EditorView,
  GutterMarker,
  ViewPlugin,
  WidgetType,
  gutter,
  keymap,
} from "@codemirror/view";

/**
 * Git diff gutter: a thin indicator column that must render to the LEFT of
 * the line numbers. This works because CodeMirrorFileEditor places the
 * `extensions` prop compartment before `baseExtensions` (which contains
 * `lineNumbers()`), and CodeMirror renders gutters in extension-precedence
 * order — do not move this extension into `baseExtensions` after the line
 * numbers.
 *
 * The buffer is diffed against the HEAD baseline (live, debounced, so
 * markers track unsaved edits); the index baseline only classifies hunks as
 * staged (hollow markers) vs unstaged (solid). Clicking a marker opens an
 * inline peek of the original lines with revert/navigation controls.
 */

const RECOMPUTE_DEBOUNCE_MS = 250;
/** Beyond this many lines (doc or baseline) the gutter stays empty. */
const MAX_DIFF_LINES = 20_000;

export interface GitDiffGutterBaseline {
  readonly headOid: string;
  readonly headContents: string;
  readonly indexOid: string | null;
  readonly indexContents: string | null;
}

interface BaselineFieldValue {
  /** Combined oid key used to skip redundant baseline dispatches. */
  readonly key: string;
  readonly headText: Text;
  readonly indexText: Text | null;
}

const setBaselineEffect = StateEffect.define<BaselineFieldValue | null>();
const setMarkersEffect = StateEffect.define<{
  readonly set: RangeSet<GutterMarker>;
  readonly specs: readonly GitDiffMarkerSpec[];
}>();

/**
 * Split with CodeMirror's own line-break semantics so a CRLF blob diffed
 * against the (always LF-joined) editor document produces no phantom
 * whole-file modifications.
 */
export function gitDiffBaselineText(contents: string): Text {
  return Text.of(contents.split(/\r\n?|\n/));
}

const baselineField = StateField.define<BaselineFieldValue | null>({
  create: () => null,
  update: (value, transaction) => {
    let next = value;
    for (const effect of transaction.effects) {
      if (effect.is(setBaselineEffect)) next = effect.value;
    }
    return next;
  },
});

const markersField = StateField.define<RangeSet<GutterMarker>>({
  create: () => RangeSet.empty,
  update: (markers, transaction) => {
    // Mapping keeps markers visually tracking edits during the debounce
    // window between recomputes; the next setMarkersEffect replaces them.
    let next = markers.map(transaction.changes);
    for (const effect of transaction.effects) {
      if (effect.is(setMarkersEffect)) next = effect.value.set;
      else if (effect.is(setBaselineEffect) && effect.value === null) next = RangeSet.empty;
    }
    return next;
  },
});

/**
 * The same marker specs the gutter renders, kept so the scrollbar overview
 * ruler can re-place its marks on geometry changes without re-diffing.
 */
const markerSpecsField = StateField.define<readonly GitDiffMarkerSpec[]>({
  create: () => [],
  update: (specs, transaction) => {
    let next = specs;
    for (const effect of transaction.effects) {
      if (effect.is(setMarkersEffect)) next = effect.value.specs;
      else if (effect.is(setBaselineEffect) && effect.value === null) next = [];
    }
    return next;
  },
});

/** Sync the diff baselines from external state; no-op when oids are unchanged. */
export function setGitDiffBaseline(view: EditorView, baseline: GitDiffGutterBaseline | null): void {
  const current = view.state.field(baselineField, false) ?? null;
  const key = baseline === null ? null : `${baseline.headOid}:${baseline.indexOid ?? ""}`;
  if ((current?.key ?? null) === key) return;
  const value =
    baseline === null || key === null
      ? null
      : {
          key,
          headText: gitDiffBaselineText(baseline.headContents),
          indexText:
            baseline.indexContents === null ? null : gitDiffBaselineText(baseline.indexContents),
        };
  view.dispatch({ effects: setBaselineEffect.of(value) });
}

// --- Marker computation (pure, unit-tested) ---------------------------------

export interface GitDiffMarkerSpec {
  /** 1-based document line number. */
  readonly line: number;
  /** null for a boundary-only line (pure deletion wedge, no bar). */
  readonly kind: "added" | "modified" | null;
  /** True when the whole hunk is staged (clean vs the index baseline). */
  readonly staged: boolean;
  /** Deletion wedge placement on this line, if any. */
  readonly wedge: "above" | "below" | null;
}

function chunkLineSpan(text: Text, from: number, to: number, end: number): number {
  if (from === to) return 0;
  const lastPos = Math.min(end, text.length);
  return text.lineAt(lastPos).number - text.lineAt(Math.min(from, text.length)).number + 1;
}

function chunkIsStaged(chunk: Chunk, indexChunks: readonly Chunk[] | null): boolean {
  if (indexChunks === null) return false;
  // A hunk is fully staged when no buffer-vs-index chunk touches its doc
  // range (inclusive bounds so zero-width deletion chunks still count).
  return !indexChunks.some(
    (indexChunk) => indexChunk.fromB <= chunk.toB && chunk.fromB <= indexChunk.toB,
  );
}

export function computeGitDiffMarkerSpecs(
  headText: Text,
  doc: Text,
  indexText: Text | null,
): GitDiffMarkerSpec[] {
  const chunks = Chunk.build(headText, doc);
  if (chunks.length === 0) return [];
  const indexChunks = indexText === null ? null : Chunk.build(indexText, doc);
  const specs: GitDiffMarkerSpec[] = [];
  for (const chunk of chunks) {
    const staged = chunkIsStaged(chunk, indexChunks);
    const linesA = chunkLineSpan(headText, chunk.fromA, chunk.toA, chunk.endA);
    const linesB = chunkLineSpan(doc, chunk.fromB, chunk.toB, chunk.endB);
    if (linesB === 0) {
      // Pure deletion: a wedge on the boundary line. A deletion at EOF has no
      // line below it, so the wedge hangs off the bottom of the last line.
      const atEof = chunk.fromB >= doc.length;
      const line = doc.lineAt(Math.min(chunk.fromB, doc.length)).number;
      specs.push({ line, kind: null, staged, wedge: atEof ? "below" : "above" });
      continue;
    }
    const firstLine = doc.lineAt(Math.min(chunk.fromB, doc.length)).number;
    const modifiedCount = Math.min(linesA, linesB);
    for (let offset = 0; offset < linesB; offset += 1) {
      const isLast = offset === linesB - 1;
      specs.push({
        line: firstLine + offset,
        kind: offset < modifiedCount ? "modified" : "added",
        staged,
        // Net-shrinking replacement: the trailing deletion hangs below the
        // hunk's last surviving line (VS Code semantics).
        wedge: isLast && linesA > linesB ? "below" : null,
      });
    }
  }
  return specs;
}

// --- Gutter markers ----------------------------------------------------------

class DiffBarMarker extends GutterMarker {
  constructor(kind: "added" | "modified", staged: boolean) {
    super();
    this.elementClass = `cm-gitDiffGutter-${kind}${staged ? " cm-gitDiffGutter-staged" : ""}`;
  }

  override eq(other: DiffBarMarker): boolean {
    return other.elementClass === this.elementClass;
  }
}

class DiffWedgeMarker extends GutterMarker {
  constructor(
    private readonly placement: "above" | "below",
    private readonly staged: boolean,
  ) {
    super();
    this.elementClass = "cm-gitDiffGutter-deleted";
  }

  override eq(other: DiffWedgeMarker): boolean {
    return other.placement === this.placement && other.staged === this.staged;
  }

  override toDOM(): Node {
    const wedge = document.createElement("div");
    wedge.className = `cm-gitDiffGutter-wedge cm-gitDiffGutter-wedge-${this.placement}${
      this.staged ? " cm-gitDiffGutter-staged" : ""
    }`;
    return wedge;
  }
}

const barMarkers = {
  added: new DiffBarMarker("added", false),
  addedStaged: new DiffBarMarker("added", true),
  modified: new DiffBarMarker("modified", false),
  modifiedStaged: new DiffBarMarker("modified", true),
};
const wedgeMarkers = {
  above: new DiffWedgeMarker("above", false),
  aboveStaged: new DiffWedgeMarker("above", true),
  below: new DiffWedgeMarker("below", false),
  belowStaged: new DiffWedgeMarker("below", true),
};

function buildMarkerSet(
  doc: Text,
  specs: ReadonlyArray<GitDiffMarkerSpec>,
): RangeSet<GutterMarker> {
  const ranges = [];
  for (const spec of specs) {
    const pos = doc.line(Math.min(spec.line, doc.lines)).from;
    if (spec.kind === "added") {
      ranges.push((spec.staged ? barMarkers.addedStaged : barMarkers.added).range(pos));
    } else if (spec.kind === "modified") {
      ranges.push((spec.staged ? barMarkers.modifiedStaged : barMarkers.modified).range(pos));
    }
    if (spec.wedge === "above") {
      ranges.push((spec.staged ? wedgeMarkers.aboveStaged : wedgeMarkers.above).range(pos));
    } else if (spec.wedge === "below") {
      ranges.push((spec.staged ? wedgeMarkers.belowStaged : wedgeMarkers.below).range(pos));
    }
  }
  return RangeSet.of(ranges, true);
}

// --- Chunk lookup helpers -----------------------------------------------------

interface ChunkContext {
  readonly chunks: readonly Chunk[];
  readonly headText: Text;
}

function currentChunks(view: EditorView): ChunkContext | null {
  const baseline = view.state.field(baselineField, false) ?? null;
  if (baseline === null) return null;
  const doc = view.state.doc;
  if (doc.lines > MAX_DIFF_LINES || baseline.headText.lines > MAX_DIFF_LINES) return null;
  return { chunks: Chunk.build(baseline.headText, doc), headText: baseline.headText };
}

/** First/last doc line numbers a chunk visually owns (incl. deletion boundary). */
function chunkLineRange(
  doc: Text,
  chunk: Chunk,
): { readonly first: number; readonly last: number } {
  const first = doc.lineAt(Math.min(chunk.fromB, doc.length)).number;
  if (chunk.fromB === chunk.toB) return { first, last: first };
  const last = doc.lineAt(Math.min(chunk.endB, doc.length)).number;
  return { first, last };
}

function chunkIndexContaining(doc: Text, chunks: readonly Chunk[], pos: number): number {
  const lineNumber = doc.lineAt(Math.min(pos, doc.length)).number;
  for (let index = 0; index < chunks.length; index += 1) {
    const chunk = chunks[index];
    if (chunk === undefined) continue;
    const range = chunkLineRange(doc, chunk);
    if (lineNumber >= range.first && lineNumber <= range.last) return index;
  }
  return -1;
}

/** Where the peek widget attaches: end of the hunk's last (or boundary) line. */
function chunkAnchorPos(doc: Text, chunk: Chunk): number {
  const pos =
    chunk.fromB === chunk.toB
      ? Math.min(chunk.fromB, doc.length)
      : Math.min(chunk.endB, doc.length);
  return doc.lineAt(pos).to;
}

// --- Hunk peek widget ---------------------------------------------------------

interface PeekData {
  readonly chunkIndex: number;
  readonly chunkCount: number;
  /** Original (HEAD) hunk text; empty string for pure additions. */
  readonly originalText: string;
}

interface PeekState {
  readonly pos: number;
  readonly data: PeekData;
}

const openPeekEffect = StateEffect.define<PeekState>({
  map: (value, mapping) => ({ ...value, pos: mapping.mapPos(value.pos) }),
});
const closePeekEffect = StateEffect.define<null>();

const peekField = StateField.define<PeekState | null>({
  create: () => null,
  update: (value, transaction) => {
    // Edits and baseline swaps invalidate the captured hunk ranges: close.
    let next = transaction.docChanged ? null : value;
    for (const effect of transaction.effects) {
      if (effect.is(openPeekEffect)) next = effect.value;
      else if (effect.is(closePeekEffect)) next = null;
      else if (effect.is(setBaselineEffect)) next = null;
    }
    return next;
  },
  provide: (field) =>
    EditorView.decorations.from(field, (value) =>
      value === null
        ? Decoration.none
        : Decoration.set([
            Decoration.widget({
              widget: new GitDiffPeekWidget(value.data),
              block: true,
              side: 1,
            }).range(value.pos),
          ]),
    ),
});

function openPeekForChunk(view: EditorView, context: ChunkContext, index: number): boolean {
  const chunk = context.chunks[index];
  if (chunk === undefined) return false;
  const doc = view.state.doc;
  const originalText = context.headText.sliceString(
    Math.min(chunk.fromA, context.headText.length),
    Math.min(chunk.endA, context.headText.length),
  );
  view.dispatch({
    effects: [
      openPeekEffect.of({
        pos: chunkAnchorPos(doc, chunk),
        data: { chunkIndex: index, chunkCount: context.chunks.length, originalText },
      }),
      EditorView.scrollIntoView(Math.min(chunk.fromB, doc.length), { y: "center" }),
    ],
  });
  return true;
}

function openAdjacentPeek(view: EditorView, fromIndex: number, direction: 1 | -1): boolean {
  const context = currentChunks(view);
  if (context === null || context.chunks.length === 0) return false;
  const count = context.chunks.length;
  const index = (((fromIndex + direction) % count) + count) % count;
  return openPeekForChunk(view, context, index);
}

/** Revert the hunk containing `pos` to its HEAD contents. */
export function revertGitDiffHunkAt(view: EditorView, pos: number): boolean {
  const context = currentChunks(view);
  if (context === null || context.chunks.length === 0) return false;
  const doc = view.state.doc;
  const index = chunkIndexContaining(doc, context.chunks, pos);
  const chunk = index === -1 ? undefined : context.chunks[index];
  if (chunk === undefined) return false;
  // Clamp symmetrically: chunk ends include the trailing line break and may
  // point one past a document that lacks one; clipping both sides keeps the
  // replacement newline-balanced.
  const insert = context.headText.sliceString(
    Math.min(chunk.fromA, context.headText.length),
    Math.min(chunk.toA, context.headText.length),
  );
  view.dispatch({
    changes: { from: chunk.fromB, to: Math.min(chunk.toB, doc.length), insert },
    effects: closePeekEffect.of(null),
    userEvent: "revert.hunk",
  });
  return true;
}

class GitDiffPeekWidget extends WidgetType {
  constructor(private readonly data: PeekData) {
    super();
  }

  override eq(other: GitDiffPeekWidget): boolean {
    return (
      other.data.chunkIndex === this.data.chunkIndex &&
      other.data.chunkCount === this.data.chunkCount &&
      other.data.originalText === this.data.originalText
    );
  }

  override ignoreEvent(): boolean {
    return true;
  }

  override toDOM(view: EditorView): HTMLElement {
    const container = document.createElement("div");
    container.className = "cm-gitDiffPeek";

    const header = document.createElement("div");
    header.className = "cm-gitDiffPeek-header";
    const title = document.createElement("span");
    title.className = "cm-gitDiffPeek-title";
    title.textContent = `Hunk ${this.data.chunkIndex + 1} of ${this.data.chunkCount}`;
    header.appendChild(title);

    const actions = document.createElement("div");
    actions.className = "cm-gitDiffPeek-actions";
    const button = (label: string, ariaLabel: string, onClick: () => void) => {
      const element = document.createElement("button");
      element.type = "button";
      element.className = "cm-gitDiffPeek-button";
      element.textContent = label;
      element.setAttribute("aria-label", ariaLabel);
      element.title = ariaLabel;
      element.addEventListener("mousedown", (event) => event.preventDefault());
      element.addEventListener("click", (event) => {
        event.preventDefault();
        onClick();
      });
      return element;
    };
    actions.appendChild(
      button("‹", "Previous change", () => openAdjacentPeek(view, this.data.chunkIndex, -1)),
    );
    actions.appendChild(
      button("›", "Next change", () => openAdjacentPeek(view, this.data.chunkIndex, 1)),
    );
    actions.appendChild(
      button("Revert", "Revert this change", () => {
        const peek = view.state.field(peekField, false) ?? null;
        if (peek !== null) revertGitDiffHunkAt(view, peek.pos);
      }),
    );
    actions.appendChild(
      button("✕", "Close", () => view.dispatch({ effects: closePeekEffect.of(null) })),
    );
    header.appendChild(actions);
    container.appendChild(header);

    if (this.data.originalText.length > 0) {
      const body = document.createElement("div");
      body.className = "cm-gitDiffPeek-body";
      for (const lineText of this.data.originalText.split("\n")) {
        const line = document.createElement("div");
        line.className = "cm-gitDiffPeek-line";
        line.textContent = lineText;
        body.appendChild(line);
      }
      container.appendChild(body);
    } else {
      const empty = document.createElement("div");
      empty.className = "cm-gitDiffPeek-empty";
      empty.textContent = "Added lines — no previous content";
      container.appendChild(empty);
    }
    return container;
  }
}

// --- Scrollbar overview ruler ---------------------------------------------------

/** Contiguous same-kind marker lines, drawn as one mark on the ruler. */
export interface GitDiffOverviewRun {
  readonly kind: "added" | "modified" | "deleted";
  readonly staged: boolean;
  readonly firstLine: number;
  readonly lastLine: number;
}

/**
 * Collapse per-line specs into hunk-sized runs. A spec that carries both a bar
 * and a deletion wedge stays one run of its bar kind: the ruler shows one mark
 * per change, not one per line.
 */
export function buildGitDiffOverviewRuns(
  specs: readonly GitDiffMarkerSpec[],
): GitDiffOverviewRun[] {
  const runs: GitDiffOverviewRun[] = [];
  for (const spec of specs) {
    const kind = spec.kind ?? "deleted";
    const previous = runs[runs.length - 1];
    if (
      previous !== undefined &&
      previous.kind === kind &&
      previous.staged === spec.staged &&
      spec.line <= previous.lastLine + 1
    ) {
      runs[runs.length - 1] = { ...previous, lastLine: Math.max(previous.lastLine, spec.line) };
      continue;
    }
    runs.push({ kind, staged: spec.staged, firstLine: spec.line, lastLine: spec.line });
  }
  return runs;
}

interface OverviewMark {
  readonly className: string;
  readonly topPercent: number;
  readonly heightPercent: number;
}

function measureOverviewMarks(view: EditorView): OverviewMark[] {
  const specs = view.state.field(markerSpecsField, false) ?? [];
  if (specs.length === 0) return [];
  // The ruler spans the scroll track, so document pixels map onto it by
  // scrollHeight — which also stays correct for docs shorter than the
  // viewport, where scrollHeight is the client height.
  const total = view.scrollDOM.scrollHeight;
  if (total <= 0) return [];
  const doc = view.state.doc;
  const marks: OverviewMark[] = [];
  for (const run of buildGitDiffOverviewRuns(specs)) {
    const firstFrom = doc.line(Math.min(run.firstLine, doc.lines)).from;
    const lastFrom = doc.line(Math.min(run.lastLine, doc.lines)).from;
    const top = view.lineBlockAt(firstFrom).top;
    const bottom = view.lineBlockAt(lastFrom).bottom;
    marks.push({
      className: `cm-gitDiffOverview-mark cm-gitDiffOverview-${run.kind}${
        run.staged ? " cm-gitDiffOverview-staged" : ""
      }`,
      topPercent: Math.max(0, Math.min(100, (top / total) * 100)),
      heightPercent: Math.max(0, Math.min(100, ((bottom - top) / total) * 100)),
    });
  }
  return marks;
}

/**
 * VS Code-style overview ruler: change marks painted over the scrollbar track.
 * Non-interactive by design — the strip sits on top of the native scrollbar, so
 * capturing pointer events here would break dragging the thumb.
 */
const overviewRulerPlugin = ViewPlugin.define((view) => {
  const dom = document.createElement("div");
  dom.className = "cm-gitDiffOverview";
  dom.setAttribute("aria-hidden", "true");
  view.dom.appendChild(dom);

  let pending = false;
  const render = (marks: readonly OverviewMark[]) => {
    dom.textContent = "";
    for (const mark of marks) {
      const element = document.createElement("div");
      element.className = mark.className;
      element.style.top = `${mark.topPercent}%`;
      element.style.height = `${mark.heightPercent}%`;
      dom.appendChild(element);
    }
  };
  const schedule = () => {
    if (pending) return;
    pending = true;
    view.requestMeasure({
      read: (measured) => {
        pending = false;
        return measureOverviewMarks(measured);
      },
      write: render,
    });
  };

  schedule();
  return {
    update: (update) => {
      const specsChanged = update.transactions.some((transaction) =>
        transaction.effects.some(
          (effect) => effect.is(setMarkersEffect) || effect.is(setBaselineEffect),
        ),
      );
      if (specsChanged || update.docChanged || update.geometryChanged) schedule();
    },
    destroy: () => {
      dom.remove();
    },
  };
});

// --- Recompute plugin ---------------------------------------------------------

function recomputeMarkers(view: EditorView): void {
  const baseline = view.state.field(baselineField, false) ?? null;
  const doc = view.state.doc;
  let specs: GitDiffMarkerSpec[] = [];
  if (
    baseline !== null &&
    doc.lines <= MAX_DIFF_LINES &&
    baseline.headText.lines <= MAX_DIFF_LINES
  ) {
    specs = computeGitDiffMarkerSpecs(baseline.headText, doc, baseline.indexText);
  }
  const markers = buildMarkerSet(doc, specs);
  const current = view.state.field(markersField, false) ?? RangeSet.empty;
  const currentSpecs = view.state.field(markerSpecsField, false) ?? [];
  if (markers.size === 0 && current.size === 0 && currentSpecs.length === 0) return;
  view.dispatch({ effects: setMarkersEffect.of({ set: markers, specs }) });
}

const recomputePlugin = ViewPlugin.define((view) => {
  let timer: number | null = null;
  const schedule = (delay: number) => {
    if (timer !== null) window.clearTimeout(timer);
    // Never dispatch inside an update cycle: even the immediate path defers.
    timer = window.setTimeout(() => {
      timer = null;
      recomputeMarkers(view);
    }, delay);
  };
  return {
    update: (update) => {
      const baselineChanged = update.transactions.some((transaction) =>
        transaction.effects.some((effect) => effect.is(setBaselineEffect)),
      );
      const reverted = update.transactions.some((transaction) =>
        transaction.isUserEvent("revert.hunk"),
      );
      if (baselineChanged || reverted) schedule(0);
      else if (update.docChanged) schedule(RECOMPUTE_DEBOUNCE_MS);
    },
    destroy: () => {
      if (timer !== null) window.clearTimeout(timer);
    },
  };
});

// --- Gutter, commands, keymap ---------------------------------------------------

function handleGutterClick(view: EditorView, pos: number): boolean {
  const context = currentChunks(view);
  if (context === null || context.chunks.length === 0) return false;
  const index = chunkIndexContaining(view.state.doc, context.chunks, pos);
  if (index === -1) return false;
  const peek = view.state.field(peekField, false) ?? null;
  if (peek !== null && peek.data.chunkIndex === index) {
    view.dispatch({ effects: closePeekEffect.of(null) });
    return true;
  }
  return openPeekForChunk(view, context, index);
}

function gotoHunk(view: EditorView, direction: 1 | -1): boolean {
  const context = currentChunks(view);
  if (context === null || context.chunks.length === 0) return false;
  const doc = view.state.doc;
  const head = view.state.selection.main.head;
  const starts = context.chunks.map((chunk) => Math.min(chunk.fromB, doc.length));
  let target = -1;
  if (direction === 1) {
    target = starts.findIndex((start) => start > head);
    if (target === -1) target = 0;
  } else {
    for (let index = starts.length - 1; index >= 0; index -= 1) {
      const start = starts[index];
      if (start !== undefined && start < head) {
        target = index;
        break;
      }
    }
    if (target === -1) target = starts.length - 1;
  }
  const pos = starts[target];
  if (pos === undefined) return false;
  view.dispatch({
    selection: { anchor: doc.lineAt(pos).from },
    effects: EditorView.scrollIntoView(pos, { y: "center" }),
    userEvent: "select",
  });
  return true;
}

export const gotoNextGitDiffHunk: Command = (view) => gotoHunk(view, 1);
export const gotoPreviousGitDiffHunk: Command = (view) => gotoHunk(view, -1);

const closePeekCommand: Command = (view) => {
  if ((view.state.field(peekField, false) ?? null) === null) return false;
  view.dispatch({ effects: closePeekEffect.of(null) });
  return true;
};

const diffGutter = gutter({
  class: "cm-gitDiffGutter",
  markers: (view) => view.state.field(markersField),
  domEventHandlers: {
    mousedown: (view, line, event) => {
      const handled = handleGutterClick(view, line.from);
      // Keep a handled marker click from also starting a review-comment
      // line selection, whose listener sits on the editor root.
      if (handled) event.stopPropagation();
      return handled;
    },
  },
});

export function gitDiffGutter(): Extension {
  return [
    baselineField,
    markersField,
    markerSpecsField,
    peekField,
    recomputePlugin,
    overviewRulerPlugin,
    diffGutter,
    keymap.of([
      { key: "Alt-F5", run: gotoNextGitDiffHunk },
      { key: "Shift-Alt-F5", run: gotoPreviousGitDiffHunk },
      { key: "Escape", run: closePeekCommand },
    ]),
  ];
}
