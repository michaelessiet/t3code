import type { LspCodeAction, LspSemanticToken } from "@t3tools/contracts";
import type * as Protocol from "vscode-languageserver-protocol";
import { mapWorkspaceEdit, uriToLocationPath } from "./LspMappings.ts";

/** A full replacement expressed in the old document's UTF-16 coordinates.
 * Incremental-sync servers require this even when clients send full buffers. */
export function fullDocumentRange(contents: string): Protocol.Range {
  let line = 0;
  let start = 0;
  for (const ending of contents.matchAll(/\r\n|\r|\n/g)) {
    line++;
    start = ending.index + ending[0].length;
  }
  return { start: { line: 0, character: 0 }, end: { line, character: contents.length - start } };
}

/** The bundled @vtsls/language-service 0.3.0 attaches bookkeeping commands
 * even to plain text fixes. Its quickFix.ts applies `edit` first; the command
 * only removes the old diagnostic / reports telemetry unless TS supplies
 * additional commands. didApplyRefactoring is telemetry-only. We do not run
 * either command: normal didChange/disk notifications refresh diagnostics. */
function optionalTypescriptBookkeeping(action: Protocol.CodeAction): boolean {
  const command = action.command;
  if (command?.command === "_typescript.didApplyRefactoring") return true;
  if (command?.command !== "_typescript.applyCodeActionCommand" || action.edit === undefined)
    return false;
  const args = command.arguments;
  if (args?.length !== 3) return false;
  const fix = args[0];
  return (
    fix !== null &&
    typeof fix === "object" &&
    typeof fix.fixName === "string" &&
    (fix.commands === undefined || (Array.isArray(fix.commands) && fix.commands.length === 0))
  );
}

/** Never silently offer only part of a refactor. Resource operations and
 * command execution need distinct client support, not an optimistic success. */
export function mapCodeAction(
  root: string,
  action: Protocol.CodeAction | Protocol.Command,
): LspCodeAction {
  const literal = typeof action.command !== "string" ? (action as Protocol.CodeAction) : undefined;
  let disabledReason = literal?.disabled?.reason;
  if (action.command !== undefined && !(literal && optionalTypescriptBookkeeping(literal)))
    disabledReason ??= "This action requires a server command, not a previewable text edit.";
  const edit = literal?.edit;
  const uris = Object.keys(edit?.changes ?? {});
  for (const change of edit?.documentChanges ?? []) {
    if ("textDocument" in change) uris.push(change.textDocument.uri);
    else
      disabledReason ??=
        "This action creates, renames or deletes files. Resource refactors are not supported yet.";
  }
  if (uris.some((uri) => uriToLocationPath(root, uri).relativePath === undefined)) {
    disabledReason ??= "This action edits a file outside the workspace.";
  }
  return {
    title: action.title,
    ...(literal?.kind !== undefined ? { kind: literal.kind } : {}),
    preferred: literal?.isPreferred ?? false,
    ...(disabledReason !== undefined ? { disabledReason } : {}),
    files: disabledReason === undefined ? mapWorkspaceEdit(root, edit ?? null) : [],
    resolveData: JSON.stringify(action),
  };
}

export function mapSemanticTokens(
  result: Protocol.SemanticTokens | null,
  legend: Protocol.SemanticTokensLegend,
): Array<LspSemanticToken> {
  if (result === null) return [];
  if (result.data.length % 5 !== 0 || result.data.some((n) => !Number.isSafeInteger(n) || n < 0)) {
    throw new Error("Invalid semantic token data");
  }
  let line = 0,
    character = 0;
  const tokens: Array<LspSemanticToken> = [];
  for (let i = 0; i < result.data.length; i += 5) {
    const deltaLine = result.data[i]!;
    line += deltaLine;
    character = deltaLine === 0 ? character + result.data[i + 1]! : result.data[i + 1]!;
    const length = result.data[i + 2]!;
    if (line > 0x7fffffff || character + length > 0x7fffffff)
      throw new Error("Semantic token position overflow");
    const kind = legend.tokenTypes[result.data[i + 3]!];
    if (kind === undefined || length === 0) continue;
    tokens.push({
      range: { start: { line, character }, end: { line, character: character + length } },
      kind,
      modifiers: legend.tokenModifiers.filter(
        (_, index) => index < 31 && (result.data[i + 4]! & (1 << index)) !== 0,
      ),
    });
  }
  return tokens;
}
