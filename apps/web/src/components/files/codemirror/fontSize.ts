import { StateEffect, StateField, type Extension } from "@codemirror/state";
import { EditorView, ViewPlugin, keymap, type Command, type ViewUpdate } from "@codemirror/view";

/**
 * Editor font size, adjustable with Cmd/Ctrl +/- while the editor is focused.
 *
 * The size lives in editor state (source of truth) and is projected onto the
 * `--cm-font-size` custom property on the editor DOM; `theme.ts` reads that
 * variable for `.cm-editor`'s `font-size`. Line height is a unitless ratio, so
 * everything (content and gutters) scales together. The chosen size persists to
 * localStorage so it survives file switches (each file remounts the editor) and
 * reloads.
 */

export const DEFAULT_EDITOR_FONT_SIZE = 12;
const MIN_EDITOR_FONT_SIZE = 8;
const MAX_EDITOR_FONT_SIZE = 32;
const FONT_SIZE_STORAGE_KEY = "t3code:editor-font-size";

export function clampEditorFontSize(size: number): number {
  if (!Number.isFinite(size)) return DEFAULT_EDITOR_FONT_SIZE;
  return Math.min(MAX_EDITOR_FONT_SIZE, Math.max(MIN_EDITOR_FONT_SIZE, Math.round(size)));
}

function readStoredFontSize(): number {
  try {
    const raw = window.localStorage.getItem(FONT_SIZE_STORAGE_KEY);
    if (raw === null) return DEFAULT_EDITOR_FONT_SIZE;
    return clampEditorFontSize(Number.parseInt(raw, 10));
  } catch {
    return DEFAULT_EDITOR_FONT_SIZE;
  }
}

function storeFontSize(size: number): void {
  try {
    window.localStorage.setItem(FONT_SIZE_STORAGE_KEY, String(size));
  } catch {
    // Ignore persistence failures (private mode / disabled storage).
  }
}

const setEditorFontSize = StateEffect.define<number>();

const editorFontSizeField = StateField.define<number>({
  create: () => readStoredFontSize(),
  update(value, tr) {
    let next = value;
    for (const effect of tr.effects) {
      if (effect.is(setEditorFontSize)) next = clampEditorFontSize(effect.value);
    }
    return next;
  },
});

/** Mirrors the state field onto the DOM custom property and persists changes. */
const editorFontSizeSync = ViewPlugin.fromClass(
  class {
    constructor(view: EditorView) {
      this.apply(view, view.state.field(editorFontSizeField));
    }

    update(update: ViewUpdate) {
      const previous = update.startState.field(editorFontSizeField);
      const next = update.state.field(editorFontSizeField);
      if (previous !== next) {
        this.apply(update.view, next);
        storeFontSize(next);
      }
    }

    private apply(view: EditorView, size: number) {
      view.dom.style.setProperty("--cm-font-size", `${size}px`);
    }
  },
);

function adjustEditorFontSize(delta: number): Command {
  return (view) => {
    const current = view.state.field(editorFontSizeField, false);
    if (current === undefined) return false;
    const next = clampEditorFontSize(current + delta);
    if (next === current) return true;
    view.dispatch({ effects: setEditorFontSize.of(next) });
    return true;
  };
}

const resetEditorFontSize: Command = (view) => {
  const current = view.state.field(editorFontSizeField, false);
  if (current === undefined) return false;
  if (current === DEFAULT_EDITOR_FONT_SIZE) return true;
  view.dispatch({ effects: setEditorFontSize.of(DEFAULT_EDITOR_FONT_SIZE) });
  return true;
};

// `Mod` is Cmd on macOS, Ctrl elsewhere. Both `Mod-=` and `Mod-+` map to zoom
// in so it works whether or not Shift is held (US keyboards put `+` on Shift-`=`);
// `Mod-0` resets to the default. `preventDefault` stops the browser's own page
// zoom while the editor is focused.
const editorFontSizeKeymap = keymap.of([
  { key: "Mod-=", run: adjustEditorFontSize(1), preventDefault: true },
  { key: "Mod-+", run: adjustEditorFontSize(1), preventDefault: true },
  { key: "Mod--", run: adjustEditorFontSize(-1), preventDefault: true },
  { key: "Mod-0", run: resetEditorFontSize, preventDefault: true },
]);

export const editorFontSize: Extension = [
  editorFontSizeField,
  editorFontSizeSync,
  editorFontSizeKeymap,
];

export const __testing = {
  editorFontSizeField,
  setEditorFontSize,
  adjustEditorFontSize,
  resetEditorFontSize,
  FONT_SIZE_STORAGE_KEY,
};
