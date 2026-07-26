/**
 * Host-provided document save hook. Vim's `:w`/`:write` dispatches through
 * this facet so the React layer can flush its debounced auto-save
 * immediately; views without a handler (read-only surfaces) no-op.
 */
import { Facet, type Extension } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";

const saveHandler = Facet.define<(view: EditorView) => void>();

export function documentSaveExtension(onSave: (view: EditorView) => void): Extension {
  return saveHandler.of(onSave);
}

/** Returns false when the view has no save handler installed. */
export function requestDocumentSave(view: EditorView): boolean {
  const handler = view.state.facet(saveHandler).at(0);
  if (handler === undefined) return false;
  handler(view);
  return true;
}
