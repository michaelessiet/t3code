// @effect-diagnostics nodeBuiltinImport:off
/**
 * LspMappings - pure conversions between LSP protocol shapes and the
 * @t3tools/contracts LSP schema (path-based, markdown-only documentation).
 *
 * @module LspMappings
 */
import * as NodePath from "node:path";
import * as NodeURL from "node:url";

import type {
  LspCompletionItem,
  LspCompletionResult,
  LspDiagnostic,
  LspFileEdits,
  LspHoverResult,
  LspLocation,
  LspTextEdit,
} from "@t3tools/contracts";
import type * as Protocol from "vscode-languageserver-protocol";

const COMPLETION_MAX_ITEMS = 200;

export function documentUri(workspaceRoot: string, relativePath: string): string {
  return NodeURL.pathToFileURL(NodePath.join(workspaceRoot, relativePath)).toString();
}

/** Map a file:// URI back to a workspace-relative (or absolute) location path. */
export function uriToLocationPath(
  workspaceRoot: string,
  uri: string,
): { readonly relativePath?: string; readonly absolutePath?: string } {
  let absolute: string;
  try {
    absolute = NodeURL.fileURLToPath(uri);
  } catch {
    return { absolutePath: uri };
  }
  const relative = NodePath.relative(workspaceRoot, absolute);
  if (relative.length === 0 || relative.startsWith("..") || NodePath.isAbsolute(relative)) {
    return { absolutePath: absolute };
  }
  return { relativePath: relative.replaceAll("\\", "/") };
}

function markupToMarkdown(
  value:
    | string
    | Protocol.MarkupContent
    | Protocol.MarkedString
    | ReadonlyArray<Protocol.MarkedString>
    | undefined
    | null,
): string {
  if (value === undefined || value === null) return "";
  if (typeof value === "string") return value;
  if (Array.isArray(value)) {
    return value
      .map((entry) => markupToMarkdown(entry))
      .filter((entry) => entry.length > 0)
      .join("\n\n");
  }
  if ("kind" in value) return value.value;
  if ("language" in value) return `\`\`\`${value.language}\n${value.value}\n\`\`\``;
  return "";
}

export function mapCompletionItem(item: Protocol.CompletionItem): LspCompletionItem {
  const textEdit = item.textEdit;
  const range =
    textEdit === undefined ? undefined : "range" in textEdit ? textEdit.range : textEdit.replace;
  const documentation = markupToMarkdown(item.documentation);
  const additionalTextEdits = mapTextEdits(item.additionalTextEdits ?? null);
  return {
    label: item.label,
    ...(item.kind !== undefined ? { kind: item.kind } : {}),
    ...(item.detail !== undefined ? { detail: item.detail } : {}),
    ...(documentation.length > 0 ? { documentation } : {}),
    ...(textEdit !== undefined
      ? { insertText: textEdit.newText }
      : item.insertText !== undefined
        ? { insertText: item.insertText }
        : {}),
    ...(item.sortText !== undefined ? { sortText: item.sortText } : {}),
    ...(item.filterText !== undefined ? { filterText: item.filterText } : {}),
    ...(range !== undefined ? { range } : {}),
    ...(additionalTextEdits.length > 0 ? { additionalTextEdits } : {}),
    // The full protocol item goes back to the server verbatim on resolve;
    // servers stash their bookkeeping in `data`, so the whole item is the
    // safest opaque token.
    resolveData: JSON.stringify(item),
  };
}

export function mapCompletion(
  result: Protocol.CompletionItem[] | Protocol.CompletionList | null,
): LspCompletionResult {
  if (result === null) return { items: [], isIncomplete: false };
  const items = Array.isArray(result) ? result : result.items;
  const isIncomplete = Array.isArray(result) ? false : result.isIncomplete;
  return {
    items: items.slice(0, COMPLETION_MAX_ITEMS).map(mapCompletionItem),
    isIncomplete: isIncomplete || items.length > COMPLETION_MAX_ITEMS,
  };
}

export function mapHover(result: Protocol.Hover | null): LspHoverResult {
  if (result === null) return null;
  const contents = markupToMarkdown(result.contents);
  if (contents.length === 0) return null;
  return { contents, ...(result.range !== undefined ? { range: result.range } : {}) };
}

export function mapLocations(
  workspaceRoot: string,
  result: Protocol.Location | Protocol.Location[] | Protocol.LocationLink[] | null,
): Array<LspLocation> {
  if (result === null) return [];
  const entries = Array.isArray(result) ? result : [result];
  const locations: Array<LspLocation> = [];
  for (const entry of entries) {
    if ("targetUri" in entry) {
      locations.push({
        ...uriToLocationPath(workspaceRoot, entry.targetUri),
        range: entry.targetSelectionRange ?? entry.targetRange,
      });
    } else {
      locations.push({
        ...uriToLocationPath(workspaceRoot, entry.uri),
        range: entry.range,
      });
    }
  }
  return locations;
}

export function mapTextEdits(edits: ReadonlyArray<Protocol.TextEdit> | null): Array<LspTextEdit> {
  if (edits === null) return [];
  return edits.map((edit) => ({ range: edit.range, newText: edit.newText }));
}

/**
 * Flatten an LSP WorkspaceEdit into per-file edit lists. Resource operations
 * (create/rename/delete files) are not supported and are skipped; locations
 * outside the workspace are skipped (we never edit files we can't address
 * workspace-relatively).
 */
export function mapWorkspaceEdit(
  workspaceRoot: string,
  edit: Protocol.WorkspaceEdit | null,
): Array<LspFileEdits> {
  if (edit === null) return [];
  const byPath = new Map<string, Array<LspTextEdit>>();

  const addEdits = (uri: string, edits: ReadonlyArray<Protocol.TextEdit>) => {
    const location = uriToLocationPath(workspaceRoot, uri);
    if (location.relativePath === undefined) return;
    const existing = byPath.get(location.relativePath) ?? [];
    existing.push(...mapTextEdits([...edits]));
    byPath.set(location.relativePath, existing);
  };

  if (edit.changes !== undefined) {
    for (const [uri, edits] of Object.entries(edit.changes)) addEdits(uri, edits);
  }
  if (edit.documentChanges !== undefined) {
    for (const change of edit.documentChanges) {
      if ("textDocument" in change)
        addEdits(change.textDocument.uri, change.edits as Protocol.TextEdit[]);
    }
  }
  return [...byPath.entries()].map(([relativePath, edits]) => ({ relativePath, edits }));
}

export function mapSignatureHelp(result: Protocol.SignatureHelp | null): {
  signatures: Array<{
    label: string;
    documentation?: string;
    parameters: Array<{ label: string; documentation?: string }>;
  }>;
  activeSignature: number;
  activeParameter: number;
} | null {
  if (result === null || result.signatures.length === 0) return null;
  return {
    signatures: result.signatures.map((signature) => {
      const documentation = markupToMarkdown(signature.documentation);
      return {
        label: signature.label,
        ...(documentation.length > 0 ? { documentation } : {}),
        parameters: (signature.parameters ?? []).map((parameter) => {
          const parameterDocumentation = markupToMarkdown(parameter.documentation);
          return {
            label:
              typeof parameter.label === "string"
                ? parameter.label
                : signature.label.slice(parameter.label[0], parameter.label[1]),
            ...(parameterDocumentation.length > 0 ? { documentation: parameterDocumentation } : {}),
          };
        }),
      };
    }),
    activeSignature: Math.max(0, result.activeSignature ?? 0),
    activeParameter: Math.max(0, result.activeParameter ?? 0),
  };
}

export function mapDiagnostics(
  diagnostics: ReadonlyArray<Protocol.Diagnostic>,
): Array<LspDiagnostic> {
  return diagnostics.map((diagnostic) => ({
    range: diagnostic.range,
    severity: diagnostic.severity ?? 1,
    message: diagnostic.message,
    ...(diagnostic.source !== undefined ? { source: diagnostic.source } : {}),
    ...(diagnostic.code !== undefined ? { code: String(diagnostic.code) } : {}),
  }));
}
