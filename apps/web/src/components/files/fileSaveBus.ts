/**
 * Registry connecting the global `file.save` command (⌘S) to the mounted
 * editable file surface. Only one file surface exists at a time; it registers
 * its save handler on mount and unregisters on unmount. Dispatch returns
 * whether a surface handled the save so the caller can decide to
 * preventDefault (blocking the browser's save-page dialog).
 */

export interface ActiveFileSaveHandler {
  /** Relative path of the file this handler saves. */
  readonly relativePath: string;
  /** Persist pending edits now; resolves once the write settles. */
  readonly flush: () => Promise<void>;
  /** True while edits remain unsaved (a settled flush that failed stays dirty). */
  readonly isDirty: () => boolean;
}

let activeHandler: ActiveFileSaveHandler | null = null;

export function registerActiveFileSave(handler: ActiveFileSaveHandler): () => void {
  activeHandler = handler;
  return () => {
    if (activeHandler === handler) activeHandler = null;
  };
}

export function getActiveFileSave(): ActiveFileSaveHandler | null {
  return activeHandler;
}

export function dispatchActiveFileSave(): Promise<void> | null {
  return activeHandler === null ? null : activeHandler.flush();
}
