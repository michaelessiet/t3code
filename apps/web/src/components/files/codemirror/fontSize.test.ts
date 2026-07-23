import { EditorState, type TransactionSpec } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import { describe, expect, it } from "vite-plus/test";

import { __testing, clampEditorFontSize, DEFAULT_EDITOR_FONT_SIZE } from "./fontSize.ts";

const { editorFontSizeField, setEditorFontSize, adjustEditorFontSize, resetEditorFontSize } =
  __testing;

/**
 * Minimal EditorView stand-in: the font-size commands only read `state` and
 * call `dispatch`, so we apply the dispatched spec back onto the state. Lets us
 * exercise the commands without a DOM (tests run in Node).
 */
function makeView() {
  let state = EditorState.create({ extensions: [editorFontSizeField] });
  const view = {
    get state() {
      return state;
    },
    dispatch(spec: TransactionSpec) {
      state = state.update(spec).state;
    },
  } as unknown as EditorView;
  return {
    view,
    size: () => view.state.field(editorFontSizeField),
  };
}

describe("editor fontSize", () => {
  it("clamps sizes into range and rounds", () => {
    expect(clampEditorFontSize(4)).toBe(8);
    expect(clampEditorFontSize(999)).toBe(32);
    expect(clampEditorFontSize(12.6)).toBe(13);
    expect(clampEditorFontSize(Number.NaN)).toBe(DEFAULT_EDITOR_FONT_SIZE);
  });

  it("starts at the default size", () => {
    expect(makeView().size()).toBe(DEFAULT_EDITOR_FONT_SIZE);
  });

  it("increases and decreases the font size", () => {
    const { view, size } = makeView();
    expect(adjustEditorFontSize(1)(view)).toBe(true);
    expect(size()).toBe(DEFAULT_EDITOR_FONT_SIZE + 1);
    adjustEditorFontSize(-1)(view);
    expect(size()).toBe(DEFAULT_EDITOR_FONT_SIZE);
  });

  it("clamps at the maximum without failing the command", () => {
    const { view, size } = makeView();
    for (let i = 0; i < 100; i += 1) adjustEditorFontSize(1)(view);
    expect(size()).toBe(32);
    expect(adjustEditorFontSize(1)(view)).toBe(true);
    expect(size()).toBe(32);
  });

  it("clamps at the minimum", () => {
    const { view, size } = makeView();
    for (let i = 0; i < 100; i += 1) adjustEditorFontSize(-1)(view);
    expect(size()).toBe(8);
  });

  it("resets to the default size", () => {
    const { view, size } = makeView();
    adjustEditorFontSize(5)(view);
    expect(size()).not.toBe(DEFAULT_EDITOR_FONT_SIZE);
    expect(resetEditorFontSize(view)).toBe(true);
    expect(size()).toBe(DEFAULT_EDITOR_FONT_SIZE);
  });

  it("clamps out-of-range values applied via the effect", () => {
    let state = EditorState.create({ extensions: [editorFontSizeField] });
    state = state.update({ effects: setEditorFontSize.of(1000) }).state;
    expect(state.field(editorFontSizeField)).toBe(32);
  });
});
