/**
 * CodeMirror 6 extensions bridging the editor to the server's LSP proxy.
 *
 * The bridge is host-agnostic: all transport happens through the injected
 * `LspBridgeHost`, so these extensions stay pure CM6 and unit-testable. The
 * React layer supplies a host wired to `lspEnvironment` commands and pushes
 * diagnostics snapshots in via `applyLspDiagnostics`.
 *
 * Correctness notes learned the hard way:
 * - Completion `from` must cover only the word segment after the last `.`;
 *   including the receiver makes CM's client-side filter drop every server
 *   suggestion (no member completions).
 * - Document sync must be flushed (and awaited) before position-based
 *   requests, or the server answers against stale text.
 * - Auto-import edits usually arrive only via `completionItem/resolve`, so
 *   acceptance resolves lazily and applies `additionalTextEdits` atomically
 *   with the main edit.
 */
import {
  autocompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
} from "@codemirror/autocomplete";
import { setDiagnostics, type Diagnostic } from "@codemirror/lint";
import { StateEffect, StateField, type Extension, type Text } from "@codemirror/state";
import { EditorView, hoverTooltip, keymap, showTooltip, type Tooltip } from "@codemirror/view";
import type {
  LspCompletionItem,
  LspCompletionResult,
  LspDiagnostic,
  LspHoverResult,
  LspLocation,
  LspLocationsResult,
  LspPosition,
  LspSignatureHelpResult,
} from "@t3tools/contracts";

import { lspRangeToOffsets, offsetToLspPosition } from "./lspPositions.ts";
import { appendHighlightedCode, renderTooltipMarkdown } from "./tooltipMarkdown.ts";

export interface LspBridgeHost {
  /** Push the full document; resolves once the server acknowledged it. */
  readonly didChange: (contents: string) => Promise<void>;
  readonly completion: (position: LspPosition) => Promise<LspCompletionResult | null>;
  readonly resolveCompletion: (resolveData: string) => Promise<LspCompletionItem | null>;
  readonly signatureHelp: (position: LspPosition) => Promise<LspSignatureHelpResult | null>;
  readonly hover: (position: LspPosition) => Promise<LspHoverResult | null>;
  readonly definition: (position: LspPosition) => Promise<LspLocationsResult | null>;
  /** Navigate to a location outside the current document. */
  readonly openLocation: (location: LspLocation) => void;
}

const DOC_SYNC_DEBOUNCE_MS = 200;

/**
 * Characters that should open (or re-query) completion even though no word
 * precedes the cursor: member access, import-path strings, decorators, JSX.
 */
const COMPLETION_TRIGGER_CHARS = new Set([".", '"', "'", "/", "@", "<"]);

interface DocSyncController {
  readonly extension: Extension;
  /** Send any pending document text now and wait for the server ack. */
  readonly flush: () => Promise<void>;
}

function createDocSyncController(host: LspBridgeHost): DocSyncController {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let pendingContents: string | null = null;
  let inflight: Promise<void> = Promise.resolve();

  const send = () => {
    if (pendingContents === null) return inflight;
    const contents = pendingContents;
    pendingContents = null;
    inflight = host.didChange(contents).catch(() => undefined);
    return inflight;
  };

  return {
    extension: EditorView.updateListener.of((update) => {
      if (!update.docChanged) return;
      pendingContents = update.state.doc.toString();
      if (timer !== null) clearTimeout(timer);
      timer = setTimeout(() => {
        timer = null;
        void send();
      }, DOC_SYNC_DEBOUNCE_MS);
    }),
    flush: () => {
      if (timer !== null) {
        clearTimeout(timer);
        timer = null;
      }
      return send();
    },
  };
}

/** LSP CompletionItemKind → CM6 completion `type` (icon class). */
export function completionKindToType(kind: number | undefined): string | undefined {
  switch (kind) {
    case 2:
    case 3:
      return "function";
    case 4:
    case 11:
    case 12:
    case 20:
    case 21:
      return "constant";
    case 5:
    case 10:
      return "property";
    case 6:
      return "variable";
    case 7:
    case 8:
    case 22:
    case 23:
      return "class";
    case 9:
      return "namespace";
    case 13:
      return "enum";
    case 14:
      return "keyword";
    case 25:
      return "type";
    default:
      return undefined;
  }
}

/**
 * Decide where the completion should anchor. Exported for tests.
 *
 * Returns null when completion should not run: no word before the cursor,
 * not explicitly requested, and the preceding character is not a trigger.
 */
export function completionAnchor(
  lineTextBeforeCursor: string,
  cursorPos: number,
  explicit: boolean,
): { from: number } | null {
  const wordMatch = /[\w$]+$/.exec(lineTextBeforeCursor);
  if (wordMatch !== null) {
    return { from: cursorPos - wordMatch[0].length };
  }
  const lastChar = lineTextBeforeCursor.at(-1);
  if (explicit || (lastChar !== undefined && COMPLETION_TRIGGER_CHARS.has(lastChar))) {
    return { from: cursorPos };
  }
  return null;
}

function applyResolvedEdits(
  view: EditorView,
  from: number,
  to: number,
  insert: string,
  item: LspCompletionItem,
): void {
  const doc = view.state.doc;
  const additional = (item.additionalTextEdits ?? []).map((edit) => {
    const offsets = lspRangeToOffsets(doc, edit.range);
    return { from: offsets.from, to: offsets.to, insert: edit.newText };
  });
  view.dispatch({
    changes: [{ from, to, insert }, ...additional],
    selection: { anchor: from + insert.length },
    userEvent: "input.complete",
  });
}

function toCompletion(host: LspBridgeHost, doc: Text, item: LspCompletionItem): Completion {
  const insert = item.insertText ?? item.label;
  const type = completionKindToType(item.kind);
  return {
    label: item.label,
    ...(item.detail !== undefined ? { detail: item.detail } : {}),
    ...(item.documentation !== undefined ? { info: item.documentation } : {}),
    ...(type !== undefined ? { type } : {}),
    apply: (view, _completion, from, to) => {
      const range = item.range !== undefined ? lspRangeToOffsets(view.state.doc, item.range) : null;
      const applyFrom = range?.from ?? from;
      const applyTo = Math.max(range?.to ?? to, to);
      const hasImportEdits = (item.additionalTextEdits ?? []).length > 0;
      if (hasImportEdits || item.resolveData === undefined) {
        applyResolvedEdits(view, applyFrom, applyTo, insert, item);
        return;
      }
      // Auto-import edits typically only materialize on resolve. Apply the
      // main edit immediately for responsiveness, then splice the resolved
      // import edits in (they never overlap the completion range).
      applyResolvedEdits(view, applyFrom, applyTo, insert, item);
      void host.resolveCompletion(item.resolveData).then((resolved) => {
        const edits = resolved?.additionalTextEdits ?? [];
        if (edits.length === 0) return;
        const currentDoc = view.state.doc;
        view.dispatch({
          changes: edits.map((edit) => {
            const offsets = lspRangeToOffsets(currentDoc, edit.range);
            return { from: offsets.from, to: offsets.to, insert: edit.newText };
          }),
          userEvent: "input.complete",
        });
      });
    },
  };
}

/** Autocomplete source backed by the LSP host. */
export function lspCompletionSource(host: LspBridgeHost, sync: DocSyncController) {
  return async (context: CompletionContext): Promise<CompletionResult | null> => {
    const line = context.state.doc.lineAt(context.pos);
    const anchor = completionAnchor(
      line.text.slice(0, context.pos - line.from),
      context.pos,
      context.explicit,
    );
    if (anchor === null) return null;
    await sync.flush();
    const result = await host.completion(offsetToLspPosition(context.state.doc, context.pos));
    if (result === null || result.items.length === 0) return null;
    return {
      from: anchor.from,
      options: result.items.map((item) => toCompletion(host, context.state.doc, item)),
      validFor: /^[\w$]*$/,
    };
  };
}

// ── Signature help ──────────────────────────────────────────────

const setSignatureTooltip = StateEffect.define<Tooltip | null>();

const signatureTooltipField = StateField.define<Tooltip | null>({
  create: () => null,
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setSignatureTooltip)) return effect.value;
    }
    // Any selection jump outside the tooltip's line dismisses it.
    if (value !== null && tr.selection !== undefined) {
      const line = tr.state.doc.lineAt(tr.state.selection.main.head);
      const tooltipLine = tr.state.doc.lineAt(Math.min(value.pos, tr.state.doc.length));
      if (line.number !== tooltipLine.number) return null;
    }
    return value;
  },
  provide: (field) => showTooltip.from(field),
});

function signatureDom(result: NonNullable<LspSignatureHelpResult>): HTMLElement {
  const dom = document.createElement("div");
  dom.className = "cm-lsp-signature";
  const signature =
    result.signatures[Math.min(result.activeSignature, result.signatures.length - 1)];
  if (signature === undefined) return dom;

  // Signature label rendered as highlighted TypeScript, with the active
  // parameter range emphasized inside the highlight run.
  const label = document.createElement("div");
  label.className = "cm-lsp-signature-label";
  const activeParameter = signature.parameters[result.activeParameter];
  const activeIndex =
    activeParameter !== undefined ? signature.label.indexOf(activeParameter.label) : -1;
  appendHighlightedCode(
    label,
    signature.label,
    activeIndex,
    activeIndex >= 0 && activeParameter !== undefined
      ? activeIndex + activeParameter.label.length
      : -1,
  );
  dom.appendChild(label);

  const documentation = activeParameter?.documentation ?? signature.documentation;
  if (documentation !== undefined && documentation.length > 0) {
    const docs = document.createElement("div");
    docs.className = "cm-lsp-signature-docs";
    docs.appendChild(renderTooltipMarkdown(documentation));
    dom.appendChild(docs);
  }
  return dom;
}

/**
 * Parameter hints: querying on "(" and ",", clearing on ")" and Escape.
 */
export function lspSignatureHelpExtension(host: LspBridgeHost, sync: DocSyncController): Extension {
  const query = async (view: EditorView) => {
    await sync.flush();
    const pos = view.state.selection.main.head;
    const result = await host.signatureHelp(offsetToLspPosition(view.state.doc, pos));
    view.dispatch({
      effects: setSignatureTooltip.of(
        result === null
          ? null
          : {
              pos,
              above: true,
              arrow: false,
              create: () => ({ dom: signatureDom(result) }),
            },
      ),
    });
  };

  return [
    signatureTooltipField,
    EditorView.updateListener.of((update) => {
      if (!update.docChanged) return;
      let typed: string | null = null;
      update.changes.iterChanges((_fromA, _toA, _fromB, _toB, inserted) => {
        if (inserted.length > 0) typed = inserted.sliceString(inserted.length - 1);
      });
      if (typed === "(" || typed === ",") {
        void query(update.view);
      } else if (typed === ")") {
        update.view.dispatch({ effects: setSignatureTooltip.of(null) });
      }
    }),
    keymap.of([
      {
        key: "Escape",
        run: (view) => {
          if (view.state.field(signatureTooltipField) === null) return false;
          view.dispatch({ effects: setSignatureTooltip.of(null) });
          return true;
        },
      },
      {
        key: "Mod-Shift-Space",
        run: (view) => {
          void query(view);
          return true;
        },
      },
    ]),
  ];
}

// ── Hover / definition / diagnostics (unchanged behavior) ──────

function hoverDom(contents: string): HTMLElement {
  const dom = document.createElement("div");
  dom.className = "cm-lsp-hover";
  dom.appendChild(renderTooltipMarkdown(contents));
  return dom;
}

/** Hover tooltips backed by the LSP host. */
export function lspHoverExtension(host: LspBridgeHost): Extension {
  return hoverTooltip(async (view, pos): Promise<Tooltip | null> => {
    const result: LspHoverResult = await host.hover(offsetToLspPosition(view.state.doc, pos));
    if (result === null) return null;
    const offsets =
      result.range !== undefined
        ? lspRangeToOffsets(view.state.doc, result.range)
        : { from: pos, to: pos };
    return {
      pos: offsets.from,
      end: offsets.to,
      above: true,
      create: () => ({ dom: hoverDom(result.contents) }),
    };
  });
}

async function goToDefinition(view: EditorView, host: LspBridgeHost): Promise<boolean> {
  const position = offsetToLspPosition(view.state.doc, view.state.selection.main.head);
  const result = await host.definition(position);
  const location = result?.locations[0];
  if (location === undefined) return false;
  host.openLocation(location);
  return true;
}

/** F12 / Cmd+click go-to-definition. */
export function lspDefinitionExtension(host: LspBridgeHost): Extension {
  return [
    keymap.of([
      {
        key: "F12",
        run: (view) => {
          void goToDefinition(view, host);
          return true;
        },
      },
    ]),
    EditorView.domEventHandlers({
      mousedown: (event, view) => {
        if (!(event.metaKey || event.ctrlKey) || event.button !== 0) return false;
        const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
        if (pos === null) return false;
        view.dispatch({ selection: { anchor: pos } });
        void goToDefinition(view, host);
        return true;
      },
    }),
  ];
}

function severityToCm(severity: number): Diagnostic["severity"] {
  switch (severity) {
    case 1:
      return "error";
    case 2:
      return "warning";
    case 4:
      return "hint";
    default:
      return "info";
  }
}

/** Map LSP diagnostics to CM6 lint diagnostics for the current doc. */
export function toCmDiagnostics(
  doc: Text,
  diagnostics: ReadonlyArray<LspDiagnostic>,
): Array<Diagnostic> {
  return diagnostics.map((diagnostic) => {
    const { from, to } = lspRangeToOffsets(doc, diagnostic.range);
    return {
      from,
      to: to === from ? Math.min(from + 1, doc.length) : to,
      severity: severityToCm(diagnostic.severity),
      message: diagnostic.message,
      ...(diagnostic.source !== undefined ? { source: diagnostic.source } : {}),
    };
  });
}

/** Push a diagnostics snapshot into the editor (call when the atom updates). */
export function applyLspDiagnostics(
  view: EditorView,
  diagnostics: ReadonlyArray<LspDiagnostic>,
): void {
  view.dispatch(setDiagnostics(view.state, toCmDiagnostics(view.state.doc, diagnostics)));
}

/** Bundle of always-on LSP extensions for a supported document. */
export function lspExtensions(host: LspBridgeHost): Extension {
  const sync = createDocSyncController(host);
  return [
    sync.extension,
    autocompletion({ override: [lspCompletionSource(host, sync)] }),
    lspSignatureHelpExtension(host, sync),
    lspHoverExtension(host),
    lspDefinitionExtension(host),
  ];
}
