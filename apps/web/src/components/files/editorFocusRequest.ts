/**
 * Focus-intent engine for the file editor.
 *
 * Openers declare *why* focus should land in the editor (quick search, the
 * content-search panel, panel keybindings, LSP go-to-definition) by arming a
 * one-shot intent right before routing through `rightPanelStore.openFile`;
 * the editor surface consumes it once its EditorView exists. Two context
 * rules keep the handoff smart:
 *
 * - Time-boxed: an intent whose file never mounts (load failure, markdown
 *   preview) cannot steal focus from an unrelated editor mount later.
 * - Gesture-aware: while an intent is pending, a pointer press into any
 *   other text entry (the chat composer, a terminal, a rename field) cancels
 *   it — the user deliberately redirected focus mid-handoff, and the editor
 *   must not yank it back when the file finishes mounting. Keystrokes are
 *   deliberately NOT a cancel signal: during the handoff gap keys land on
 *   whatever briefly holds focus, and typing in that window is almost always
 *   meant for the editor being opened.
 */

export type EditorFocusIntentSource =
  | "quick-search"
  | "content-search"
  | "panel-command"
  | "editor-navigation";

const EDITOR_FOCUS_INTENT_TTL_MS = 3000;

interface EditorFocusIntent {
  readonly source: EditorFocusIntentSource;
  readonly armedAt: number;
}

let pendingIntent: EditorFocusIntent | null = null;

/**
 * A pointer press here means the user is claiming focus for another text
 * entry. The code editor is excluded first — CodeMirror's content element is
 * itself contenteditable, and pressing into the editor is the handoff
 * completing, not a redirect.
 */
function isFocusRedirectTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.closest("[data-code-editor]") !== null) return false;
  if (target.closest("[data-terminal-owner]") !== null) return true;
  if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) return true;
  return target.closest('[contenteditable]:not([contenteditable="false"])') !== null;
}

function onPointerDownCapture(event: Event): void {
  if (isFocusRedirectTarget(event.target)) {
    cancelEditorFocusRequest();
  }
}

// The redirect listener only exists while an intent is pending, so idle
// sessions carry no global listener. Capture phase: the arming gesture's own
// event has already passed the window by the time the opener's (bubble-phase)
// handler arms the intent, so it can never cancel itself.
let listening = false;

function startRedirectListener(): void {
  if (listening || typeof window === "undefined") return;
  listening = true;
  window.addEventListener("pointerdown", onPointerDownCapture, true);
}

function stopRedirectListener(): void {
  if (!listening || typeof window === "undefined") return;
  listening = false;
  window.removeEventListener("pointerdown", onPointerDownCapture, true);
}

export function requestEditorFocus(source: EditorFocusIntentSource): void {
  pendingIntent = { source, armedAt: Date.now() };
  startRedirectListener();
}

export function cancelEditorFocusRequest(): void {
  pendingIntent = null;
  stopRedirectListener();
}

export function consumeEditorFocusRequest(): boolean {
  const intent = pendingIntent;
  cancelEditorFocusRequest();
  return intent !== null && Date.now() - intent.armedAt <= EDITOR_FOCUS_INTENT_TTL_MS;
}
