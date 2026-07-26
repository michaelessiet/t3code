// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from "vite-plus/test";

import {
  cancelEditorFocusRequest,
  consumeEditorFocusRequest,
  requestEditorFocus,
} from "./editorFocusRequest";

function pointerDownOn(element: Element): void {
  element.dispatchEvent(new Event("pointerdown", { bubbles: true, composed: true }));
}

afterEach(() => {
  cancelEditorFocusRequest();
  document.body.innerHTML = "";
  vi.useRealTimers();
});

describe("editor focus intents", () => {
  it("is one-shot: consuming an armed intent reports it once", () => {
    requestEditorFocus("quick-search");
    expect(consumeEditorFocusRequest()).toBe(true);
    expect(consumeEditorFocusRequest()).toBe(false);
  });

  it("reports nothing when no intent was armed", () => {
    expect(consumeEditorFocusRequest()).toBe(false);
  });

  it("expires an intent whose surface never mounted", () => {
    vi.useFakeTimers();
    requestEditorFocus("editor-navigation");
    vi.advanceTimersByTime(3001);
    expect(consumeEditorFocusRequest()).toBe(false);
  });

  it("cancels when the user presses into another text entry", () => {
    document.body.innerHTML = `<div contenteditable="true"><p id="composer">draft</p></div>`;
    requestEditorFocus("editor-navigation");
    pointerDownOn(document.getElementById("composer")!);
    expect(consumeEditorFocusRequest()).toBe(false);
  });

  it("cancels when the user presses into a terminal", () => {
    document.body.innerHTML = `<div data-terminal-owner="drawer"><div id="term"></div></div>`;
    requestEditorFocus("panel-command");
    pointerDownOn(document.getElementById("term")!);
    expect(consumeEditorFocusRequest()).toBe(false);
  });

  it("cancels when the user presses into an input field", () => {
    document.body.innerHTML = `<input id="rename" />`;
    requestEditorFocus("content-search");
    pointerDownOn(document.getElementById("rename")!);
    expect(consumeEditorFocusRequest()).toBe(false);
  });

  it("survives pointer presses on non-text-entry surfaces", () => {
    document.body.innerHTML = `<button id="tab">file.ts</button>`;
    requestEditorFocus("quick-search");
    pointerDownOn(document.getElementById("tab")!);
    expect(consumeEditorFocusRequest()).toBe(true);
  });

  it("does not treat the code editor's own contenteditable as a redirect", () => {
    document.body.innerHTML = `<div data-code-editor=""><div contenteditable="true" id="cm"></div></div>`;
    requestEditorFocus("editor-navigation");
    pointerDownOn(document.getElementById("cm")!);
    expect(consumeEditorFocusRequest()).toBe(true);
  });

  it("stops watching for redirects once consumed", () => {
    document.body.innerHTML = `<input id="rename" />`;
    requestEditorFocus("quick-search");
    expect(consumeEditorFocusRequest()).toBe(true);
    // A later press must not affect the next, unrelated intent.
    requestEditorFocus("panel-command");
    expect(consumeEditorFocusRequest()).toBe(true);
  });
});
