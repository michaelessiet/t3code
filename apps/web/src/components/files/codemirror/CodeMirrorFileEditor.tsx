import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { bracketMatching, indentOnInput } from "@codemirror/language";
import { highlightSelectionMatches, search, searchKeymap } from "@codemirror/search";
import { Annotation, Compartment, EditorState, type Extension } from "@codemirror/state";
import {
  EditorView,
  crosshairCursor,
  drawSelection,
  dropCursor,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
  rectangularSelection,
} from "@codemirror/view";
import { vim } from "@replit/codemirror-vim";
import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { computeDocReplacement } from "./docReplacement";
import { languageExtensionForPath } from "./languages";
import { revealEditorLine, revealLineExtension } from "./revealLine";
import { editorTheme } from "./theme";

export interface CodeMirrorFileEditorProps {
  relativePath: string;
  contents: string;
  wordWrap: boolean;
  vimMode?: boolean;
  readOnly?: boolean;
  /** Line to scroll to and highlight; retriggered by `revealRequestId`. */
  revealLine?: number | null;
  revealRequestId?: number;
  /** Additional extensions (e.g. review comments); reconfigured on change. */
  extensions?: Extension;
  className?: string;
  onContentsChange?: (contents: string) => void;
  /** Receives the live EditorView (null on teardown) for imperative syncs. */
  onViewReady?: (view: EditorView | null) => void;
}

/** Marks transactions that apply `contents` prop changes (external reloads). */
const externalContentsUpdate = Annotation.define<boolean>();

interface EditorHandle {
  readonly view: EditorView;
  readonly wrap: Compartment;
  readonly readOnly: Compartment;
  readonly vim: Compartment;
  readonly extra: Compartment;
}

const baseExtensions: Extension = [
  lineNumbers(),
  highlightActiveLineGutter(),
  highlightSpecialChars(),
  history(),
  drawSelection(),
  dropCursor(),
  EditorState.allowMultipleSelections.of(true),
  indentOnInput(),
  bracketMatching(),
  closeBrackets(),
  rectangularSelection(),
  crosshairCursor(),
  highlightActiveLine(),
  highlightSelectionMatches(),
  search({ top: true }),
  keymap.of([
    ...closeBracketsKeymap,
    ...defaultKeymap,
    ...searchKeymap,
    ...historyKeymap,
    indentWithTab,
  ]),
  editorTheme,
  revealLineExtension,
];

function readOnlyExtension(readOnly: boolean): Extension {
  return [EditorState.readOnly.of(readOnly), EditorView.editable.of(!readOnly)];
}

function wordWrapExtension(wordWrap: boolean): Extension {
  return wordWrap ? EditorView.lineWrapping : [];
}

function vimModeExtension(vimMode: boolean): Extension {
  return vimMode ? vim() : [];
}

/**
 * React wrapper owning a CodeMirror EditorView for one file surface. The
 * view is created once per mount; prop updates flow in as transactions
 * (external content reloads apply as minimal changes so the cursor and
 * decorations survive) and local edits flow out through `onContentsChange`.
 */
export function CodeMirrorFileEditor({
  relativePath,
  contents,
  wordWrap,
  vimMode = false,
  readOnly = false,
  revealLine = null,
  revealRequestId,
  extensions,
  className,
  onContentsChange,
  onViewReady,
}: CodeMirrorFileEditorProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [editor, setEditor] = useState<EditorHandle | null>(null);
  const view = editor?.view ?? null;
  /** Contents last applied to or emitted from the document. */
  const syncedContentsRef = useRef<string | null>(null);

  const latestRef = useRef({ contents, wordWrap, vimMode, readOnly, extensions, onContentsChange });
  useLayoutEffect(() => {
    latestRef.current = { contents, wordWrap, vimMode, readOnly, extensions, onContentsChange };
  });

  const onViewReadyRef = useRef(onViewReady);
  useLayoutEffect(() => {
    onViewReadyRef.current = onViewReady;
  });

  useLayoutEffect(() => {
    const parent = containerRef.current;
    if (!parent) return;
    const initial = latestRef.current;
    const languageCompartment = new Compartment();
    const wrapCompartment = new Compartment();
    const readOnlyCompartment = new Compartment();
    const vimCompartment = new Compartment();
    const extraCompartment = new Compartment();

    const editorView = new EditorView({
      parent,
      state: EditorState.create({
        doc: initial.contents,
        extensions: [
          // Vim must precede the other keymaps so it can intercept keys first.
          vimCompartment.of(vimModeExtension(initial.vimMode)),
          languageCompartment.of(languageExtensionForPath(relativePath)),
          wrapCompartment.of(wordWrapExtension(initial.wordWrap)),
          readOnlyCompartment.of(readOnlyExtension(initial.readOnly)),
          extraCompartment.of(initial.extensions ?? []),
          baseExtensions,
          EditorView.updateListener.of((update) => {
            if (!update.docChanged) return;
            if (update.transactions.some((tr) => tr.annotation(externalContentsUpdate))) return;
            const nextContents = update.state.doc.toString();
            syncedContentsRef.current = nextContents;
            latestRef.current.onContentsChange?.(nextContents);
          }),
        ],
      }),
    });

    syncedContentsRef.current = initial.contents;
    setEditor({
      view: editorView,
      wrap: wrapCompartment,
      readOnly: readOnlyCompartment,
      vim: vimCompartment,
      extra: extraCompartment,
    });
    onViewReadyRef.current?.(editorView);
    return () => {
      onViewReadyRef.current?.(null);
      setEditor(null);
      editorView.destroy();
    };
  }, [relativePath]);

  useEffect(() => {
    if (view === null || contents === syncedContentsRef.current) return;
    syncedContentsRef.current = contents;
    const replacement = computeDocReplacement(view.state.doc.toString(), contents);
    if (replacement === null) return;
    view.dispatch({
      changes: replacement,
      annotations: externalContentsUpdate.of(true),
    });
  }, [contents, view]);

  useEffect(() => {
    editor?.view.dispatch({
      effects: editor.wrap.reconfigure(wordWrapExtension(wordWrap)),
    });
  }, [editor, wordWrap]);

  useEffect(() => {
    editor?.view.dispatch({
      effects: editor.readOnly.reconfigure(readOnlyExtension(readOnly)),
    });
  }, [editor, readOnly]);

  useEffect(() => {
    editor?.view.dispatch({
      effects: editor.vim.reconfigure(vimModeExtension(vimMode)),
    });
  }, [editor, vimMode]);

  useEffect(() => {
    editor?.view.dispatch({
      effects: editor.extra.reconfigure(extensions ?? []),
    });
  }, [editor, extensions]);

  useEffect(() => {
    if (view === null || revealRequestId === undefined) return;
    revealEditorLine(view, revealLine);
  }, [view, revealLine, revealRequestId]);

  return <div ref={containerRef} className={className} />;
}
