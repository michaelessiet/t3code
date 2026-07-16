/**
 * React glue between an open CodeMirror file surface and the server's LSP
 * proxy: owns the document lifecycle (didOpen/didChange/didClose), feeds
 * diagnostics snapshots into the editor, and supplies the transport host for
 * the pure CM6 extensions in lspBridge.ts.
 */
import { keymap, type EditorView } from "@codemirror/view";
import type { Extension } from "@codemirror/state";
import type { EnvironmentId, LspLocation } from "@t3tools/contracts";
import { isLspSupportedPath } from "@t3tools/shared/lspSupport";
import { useAtomValue } from "@effect/atom-react";
import * as Option from "effect/Option";
import { AsyncResult } from "effect/unstable/reactivity";
import { useEffect, useMemo, useRef } from "react";

import { lspEnvironment } from "~/state/lsp";
import { useAtomCommand } from "~/state/use-atom-command";

import { applyLspDiagnostics, lspExtensions, type LspBridgeHost } from "./lspBridge";
import { lspPositionToOffset } from "./lspPositions";

export interface UseLspBridgeOptions {
  readonly environmentId: EnvironmentId;
  readonly cwd: string;
  readonly relativePath: string;
  readonly view: EditorView | null;
  /** Navigate to another file (workspace-relative) at a 1-based line. */
  readonly onOpenFileAtLine: (relativePath: string, line: number) => void;
}

/**
 * Returns the LSP extension bundle for the file surface, or null when the
 * file's language has no server. Format-on-demand is bound to Shift-Alt-F.
 */
export function useLspBridge({
  environmentId,
  cwd,
  relativePath,
  view,
  onOpenFileAtLine,
}: UseLspBridgeOptions): Extension | null {
  const supported = isLspSupportedPath(relativePath);
  const didOpen = useAtomCommand(lspEnvironment.didOpen);
  const didChange = useAtomCommand(lspEnvironment.didChange);
  const didClose = useAtomCommand(lspEnvironment.didClose);
  const completion = useAtomCommand(lspEnvironment.completion);
  const resolveCompletion = useAtomCommand(lspEnvironment.resolveCompletion);
  const signatureHelp = useAtomCommand(lspEnvironment.signatureHelp);
  const hover = useAtomCommand(lspEnvironment.hover);
  const definition = useAtomCommand(lspEnvironment.definition);
  const format = useAtomCommand(lspEnvironment.format);

  const versionRef = useRef(0);
  const viewRef = useRef<EditorView | null>(view);
  useEffect(() => {
    viewRef.current = view;
  }, [view]);
  const onOpenFileAtLineRef = useRef(onOpenFileAtLine);
  useEffect(() => {
    onOpenFileAtLineRef.current = onOpenFileAtLine;
  }, [onOpenFileAtLine]);

  // Document lifecycle: open when the editor mounts for a supported file,
  // close when the surface unmounts or switches files. Failures are
  // intentionally quiet — a missing language server degrades to plain
  // editing, not an error surface.
  useEffect(() => {
    if (!supported || view === null) return;
    versionRef.current = 0;
    void didOpen({
      environmentId,
      input: { cwd, relativePath, contents: view.state.doc.toString() },
    });
    return () => {
      void didClose({ environmentId, input: { cwd, relativePath } });
    };
  }, [cwd, didClose, didOpen, environmentId, relativePath, supported, view]);

  const host = useMemo<LspBridgeHost | null>(() => {
    if (!supported) return null;
    return {
      didChange: async (contents) => {
        versionRef.current += 1;
        await didChange({
          environmentId,
          input: { cwd, relativePath, contents, version: versionRef.current },
        });
      },
      completion: async (position) => {
        const result = await completion({ environmentId, input: { cwd, relativePath, position } });
        return result._tag === "Success" ? Option.getOrNull(AsyncResult.value(result)) : null;
      },
      resolveCompletion: async (resolveData) => {
        const result = await resolveCompletion({
          environmentId,
          input: { cwd, relativePath, resolveData },
        });
        return result._tag === "Success" ? Option.getOrNull(AsyncResult.value(result)) : null;
      },
      signatureHelp: async (position) => {
        const result = await signatureHelp({
          environmentId,
          input: { cwd, relativePath, position },
        });
        return result._tag === "Success"
          ? (Option.getOrNull(AsyncResult.value(result)) ?? null)
          : null;
      },
      hover: async (position) => {
        const result = await hover({ environmentId, input: { cwd, relativePath, position } });
        return result._tag === "Success"
          ? (Option.getOrNull(AsyncResult.value(result)) ?? null)
          : null;
      },
      definition: async (position) => {
        const result = await definition({ environmentId, input: { cwd, relativePath, position } });
        return result._tag === "Success" ? Option.getOrNull(AsyncResult.value(result)) : null;
      },
      openLocation: (location: LspLocation) => {
        if (location.relativePath === undefined) return;
        if (location.relativePath === relativePath) {
          const currentView = viewRef.current;
          if (currentView === null) return;
          const offset = lspPositionToOffset(currentView.state.doc, location.range.start);
          currentView.dispatch({
            selection: { anchor: offset },
            scrollIntoView: true,
          });
          currentView.focus();
          return;
        }
        onOpenFileAtLineRef.current(location.relativePath, location.range.start.line + 1);
      },
    };
  }, [
    completion,
    cwd,
    definition,
    didChange,
    environmentId,
    hover,
    relativePath,
    resolveCompletion,
    signatureHelp,
    supported,
  ]);

  // Push diagnostics snapshots into the live editor as they stream in.
  const diagnosticsResult = useAtomValue(
    lspEnvironment.diagnostics({ environmentId, input: { cwd } }),
  );
  const snapshot = Option.getOrNull(AsyncResult.value(diagnosticsResult));
  useEffect(() => {
    if (!supported || view === null) return;
    applyLspDiagnostics(view, snapshot?.get(relativePath) ?? []);
  }, [relativePath, snapshot, supported, view]);

  return useMemo(() => {
    if (host === null) return null;
    return [
      lspExtensions(host),
      keymap.of([
        {
          key: "Shift-Alt-f",
          run: (editorView) => {
            void (async () => {
              const result = await format({
                environmentId,
                input: { cwd, relativePath },
              });
              if (result._tag !== "Success") return;
              const formatting = Option.getOrNull(AsyncResult.value(result));
              if (formatting === undefined || formatting === null) return;
              if (formatting.edits.length === 0) return;
              const doc = editorView.state.doc;
              editorView.dispatch({
                changes: formatting.edits.map((edit) => ({
                  from: lspPositionToOffset(doc, edit.range.start),
                  to: lspPositionToOffset(doc, edit.range.end),
                  insert: edit.newText,
                })),
              });
            })();
            return true;
          },
        },
      ]),
    ];
  }, [cwd, environmentId, format, host, relativePath]);
}
