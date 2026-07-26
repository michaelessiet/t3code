"use client";

/**
 * Typed window-event bus for file-tree actions. Lets the global keybinding
 * handler in `ChatView` reach the mounted `FileBrowserPanel` without prop
 * drilling or shared refs, mirroring `previewActionBus`.
 */
export type FileTreeAction = "focus" | "new-file" | "new-directory" | "rename" | "search";

const EVENT_NAME = "t3code:file-tree-action";

export function dispatchFileTreeAction(action: FileTreeAction): void {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent<FileTreeAction>(EVENT_NAME, { detail: action }));
}

export function subscribeFileTreeAction(listener: (action: FileTreeAction) => void): () => void {
  if (typeof window === "undefined") return () => {};
  const handler = (event: Event) => {
    const detail = (event as CustomEvent<FileTreeAction>).detail;
    if (typeof detail === "string") listener(detail);
  };
  window.addEventListener(EVENT_NAME, handler);
  return () => window.removeEventListener(EVENT_NAME, handler);
}
