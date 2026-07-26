import { isEditorFocused } from "./editorFocus";

/**
 * Bridges code-editor focus to the Electron desktop shell so it can hand
 * Cmd/Ctrl +/-/0 to the editor's font-size shortcuts (see
 * `codemirror/fontSize.ts`) instead of window zoom. The shell disables its
 * window-zoom menu items while the editor is focused; a disabled accelerator
 * falls through to the renderer, where CodeMirror handles it.
 *
 * No-op on web builds, where `desktopBridge` (and this method) are absent.
 */

let lastNotified: boolean | null = null;
let scheduled = false;

function flush(): void {
  scheduled = false;
  const notify = window.desktopBridge?.setCodeEditorFocused;
  if (typeof notify !== "function") return;
  const focused = isEditorFocused();
  if (focused === lastNotified) return;
  lastNotified = focused;
  notify(focused);
}

/**
 * Reports the editor focus state to the desktop shell, deferred to a microtask
 * so focus moving directly between two editors (blur then focus) settles to the
 * correct value before it is reported. Safe to call on every focus/blur.
 */
export function syncCodeEditorFocusToDesktop(): void {
  if (typeof window === "undefined") return;
  if (typeof window.desktopBridge?.setCodeEditorFocused !== "function") return;
  if (scheduled) return;
  scheduled = true;
  queueMicrotask(flush);
}
