import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { bracketMatching, indentOnInput } from "@codemirror/language";
import { highlightSelectionMatches, search, searchKeymap } from "@codemirror/search";
import {
  Annotation,
  Compartment,
  EditorSelection,
  EditorState,
  type Extension,
} from "@codemirror/state";
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
import { CodeMirror, Vim, vim } from "@replit/codemirror-vim";
import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { syncCodeEditorFocusToDesktop } from "../../../lib/desktopEditorZoom";

import { computeDocReplacement } from "./docReplacement";
import { requestDocumentSave } from "./documentSave";
import { editorFontSize } from "./fontSize";
import { languageExtensionForPath, lazyLanguageLoaderForPath } from "./languages";
import { goToLspDefinitionAtCursor, showLspHoverAtCursor } from "./lspBridge";
import { revealEditorLine, revealLineExtension } from "./revealLine";
import { editorTheme } from "./theme";
import {
  loadEditorViewState,
  saveEditorViewState,
  shouldRestoreEditorViewState,
} from "./viewStateCache";

// `gh` in vim normal mode shows LSP hover info at the cursor (the VSCode-vim
// convention). Registered once; no-ops in documents without an LSP bridge.
Vim.defineAction("lspHover", (cm) => {
  showLspHoverAtCursor(cm.cm6);
});
Vim.mapCommand("gh", "action", "lspHover", {}, { context: "normal" });

// `gd` (VSCode-vim) and `<C-]>` (classic vim tag jump) go to the LSP
// definition of the symbol at the cursor; no-ops without an LSP bridge.
Vim.defineAction("lspDefinition", (cm) => {
  goToLspDefinitionAtCursor(cm.cm6);
});
Vim.mapCommand("gd", "action", "lspDefinition", {}, { context: "normal" });
Vim.mapCommand("<C-]>", "action", "lspDefinition", {}, { context: "normal" });

// Vim's `:w`/`:write` ex command dispatches to CodeMirror.commands.save;
// route it to the host's save handler (flushes the debounced auto-save).
CodeMirror.commands.save = (cm: CodeMirror) => {
  requestDocumentSave(cm.cm6);
};

export interface CodeMirrorFileEditorProps {
  relativePath: string;
  contents: string;
  wordWrap: boolean;
  vimMode?: boolean;
  readOnly?: boolean;
  /** Line to scroll to and highlight; retriggered by `revealRequestId`. */
  revealLine?: number | null;
  revealRequestId?: number;
  /**
   * When set, cursor/scroll are remembered across unmounts under this key
   * (see viewStateCache). A fresh explicit reveal still wins over the
   * remembered position.
   */
  viewStateKey?: string;
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
  // Precedes the default keymap so Cmd/Ctrl +/- adjust font size while focused.
  editorFontSize,
  keymap.of([
    ...closeBracketsKeymap,
    ...defaultKeymap,
    ...searchKeymap,
    ...historyKeymap,
    indentWithTab,
  ]),
  editorTheme,
  revealLineExtension,
  // Report focus to the desktop shell so it yields Cmd/Ctrl +/-/0 to the
  // editor's font-size shortcuts while focused (see desktopEditorZoom.ts).
  EditorView.domEventHandlers({
    focus: () => {
      syncCodeEditorFocusToDesktop();
      return false;
    },
    blur: () => {
      syncCodeEditorFocusToDesktop();
      return false;
    },
  }),
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
  viewStateKey,
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

  const latestRef = useRef({
    contents,
    wordWrap,
    vimMode,
    readOnly,
    revealLine,
    revealRequestId,
    viewStateKey,
    extensions,
    onContentsChange,
  });
  useLayoutEffect(() => {
    latestRef.current = {
      contents,
      wordWrap,
      vimMode,
      readOnly,
      revealLine,
      revealRequestId,
      viewStateKey,
      extensions,
      onContentsChange,
    };
  });
  /** Set while restoring a remembered position: skips one mount-run reveal. */
  const suppressNextRevealRef = useRef(false);

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

    // Remembered cursor/scroll from the last unmount of this file, unless a
    // fresh explicit reveal is pending (which must win). Contents can change
    // while the file is closed, so offsets clamp to the current doc.
    const saved =
      initial.viewStateKey === undefined ? null : loadEditorViewState(initial.viewStateKey);
    const restore = shouldRestoreEditorViewState(saved, initial.revealLine, initial.revealRequestId)
      ? saved
      : null;
    const clampOffset = (offset: number) =>
      Math.max(0, Math.min(Math.trunc(offset), initial.contents.length));

    const editorView = new EditorView({
      parent,
      state: EditorState.create({
        doc: initial.contents,
        ...(restore === null
          ? {}
          : {
              selection: EditorSelection.single(
                clampOffset(restore.anchor),
                clampOffset(restore.head),
              ),
            }),
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

    // Languages outside the eager set resolve from the language-data
    // registry; the grammar chunk arrives async, so highlighting appears
    // once loaded. A load that fails or outlives the view degrades to plain.
    const loadLazyLanguage = lazyLanguageLoaderForPath(relativePath);
    let lazyLanguageStale = false;
    if (loadLazyLanguage !== null) {
      loadLazyLanguage().then(
        (language) => {
          if (lazyLanguageStale) return;
          editorView.dispatch({ effects: languageCompartment.reconfigure(language) });
        },
        () => {},
      );
    }

    // When restoring, the mount-triggered reveal run must not re-center a
    // stale reveal line; the scroll write lands via requestMeasure so it
    // follows CodeMirror's initial layout instead of being clamped to 0.
    suppressNextRevealRef.current = restore !== null;
    if (restore !== null && restore.scrollTop > 0) {
      const scrollTop = restore.scrollTop;
      editorView.requestMeasure({
        read: () => {},
        write: () => {
          editorView.scrollDOM.scrollTop = scrollTop;
        },
      });
    }

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
      // Capture position before destroy; layout-effect cleanup runs with the
      // DOM still attached, so selection and scrollTop are both still valid.
      const { viewStateKey: latestKey, revealRequestId: latestRevealRequestId } = latestRef.current;
      if (latestKey !== undefined) {
        saveEditorViewState(latestKey, {
          anchor: editorView.state.selection.main.anchor,
          head: editorView.state.selection.main.head,
          scrollTop: editorView.scrollDOM.scrollTop,
          revealRequestId: latestRevealRequestId,
        });
      }
      lazyLanguageStale = true;
      onViewReadyRef.current?.(null);
      setEditor(null);
      editorView.destroy();
      // Destroying a focused editor won't emit a blur; re-sync so the desktop
      // shell restores window zoom.
      syncCodeEditorFocusToDesktop();
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
    // One-shot: a restored position suppresses only the mount-triggered run;
    // later runs (requestId bump, QuickSearch revealLine change) reveal as
    // usual. The mount layout-effect set the ref before this passive effect.
    if (suppressNextRevealRef.current) {
      suppressNextRevealRef.current = false;
      return;
    }
    revealEditorLine(view, revealLine);
  }, [view, revealLine, revealRequestId]);

  // data-code-editor feeds the `editorFocus` keybinding when-context
  // (see lib/editorFocus.ts).
  return <div ref={containerRef} className={className} data-code-editor="" />;
}
