import * as Schema from "effect/Schema";

import { NonNegativeInt, TrimmedNonEmptyString } from "./baseSchemas.ts";

/**
 * Language-intelligence RPCs, proxied to per-workspace language servers.
 *
 * Shapes intentionally mirror a *subset* of the Language Server Protocol,
 * re-encoded as workspace-root-relative paths instead of URIs so clients
 * never handle server-local filesystem details. Documents are synced with
 * full text (LSP `TextDocumentSyncKind.Full`) — simple and robust; the
 * incremental-sync optimization can come later without changing consumers.
 *
 * All queries return edits/locations for the client to apply or navigate;
 * the LSP surface never mutates files itself (writes go through
 * `projects.writeFile` with its optimistic-concurrency guard).
 */

export const LspPosition = Schema.Struct({
  /** 0-based line. */
  line: NonNegativeInt,
  /** 0-based UTF-16 character offset within the line. */
  character: NonNegativeInt,
});
export type LspPosition = typeof LspPosition.Type;

export const LspRange = Schema.Struct({
  start: LspPosition,
  end: LspPosition,
});
export type LspRange = typeof LspRange.Type;

export const LspDocumentInput = Schema.Struct({
  cwd: TrimmedNonEmptyString,
  relativePath: TrimmedNonEmptyString,
});
export type LspDocumentInput = typeof LspDocumentInput.Type;

export const LspDidOpenInput = Schema.Struct({
  cwd: TrimmedNonEmptyString,
  relativePath: TrimmedNonEmptyString,
  contents: Schema.String,
});
export type LspDidOpenInput = typeof LspDidOpenInput.Type;

export const LspDidChangeInput = Schema.Struct({
  cwd: TrimmedNonEmptyString,
  relativePath: TrimmedNonEmptyString,
  contents: Schema.String,
  /** Monotonic per-document version supplied by the editing client. */
  version: NonNegativeInt,
});
export type LspDidChangeInput = typeof LspDidChangeInput.Type;

export const LspPositionInput = Schema.Struct({
  cwd: TrimmedNonEmptyString,
  relativePath: TrimmedNonEmptyString,
  position: LspPosition,
});
export type LspPositionInput = typeof LspPositionInput.Type;

export const LspTextEdit = Schema.Struct({
  range: LspRange,
  newText: Schema.String,
});
export type LspTextEdit = typeof LspTextEdit.Type;

export const LspCompletionItem = Schema.Struct({
  label: Schema.String,
  /** LSP CompletionItemKind numeric code. */
  kind: Schema.optional(NonNegativeInt),
  detail: Schema.optional(Schema.String),
  /** Markdown documentation. */
  documentation: Schema.optional(Schema.String),
  insertText: Schema.optional(Schema.String),
  sortText: Schema.optional(Schema.String),
  filterText: Schema.optional(Schema.String),
  /** Range to replace when accepting the item; falls back to the word at the cursor. */
  range: Schema.optional(LspRange),
  /**
   * Extra edits applied alongside acceptance — auto-import statements land
   * here. Often absent until the item is resolved.
   */
  additionalTextEdits: Schema.optional(Schema.Array(LspTextEdit)),
  /**
   * Opaque server-side payload (JSON) for `lsp.resolveCompletion`. Present
   * when the server supports lazy resolution (documentation, auto-import
   * edits). Clients echo it back verbatim.
   */
  resolveData: Schema.optional(Schema.String),
});
export type LspCompletionItem = typeof LspCompletionItem.Type;

export const LspResolveCompletionInput = Schema.Struct({
  cwd: TrimmedNonEmptyString,
  relativePath: TrimmedNonEmptyString,
  /** The `resolveData` payload from a completion item. */
  resolveData: Schema.String,
});
export type LspResolveCompletionInput = typeof LspResolveCompletionInput.Type;

export const LspCompletionResult = Schema.Struct({
  items: Schema.Array(LspCompletionItem),
  isIncomplete: Schema.Boolean,
});
export type LspCompletionResult = typeof LspCompletionResult.Type;

export const LspHoverResult = Schema.NullOr(
  Schema.Struct({
    /** Markdown contents. */
    contents: Schema.String,
    range: Schema.optional(LspRange),
  }),
);
export type LspHoverResult = typeof LspHoverResult.Type;

export const LspLocation = Schema.Struct({
  /** Workspace-root-relative path when the location is inside the workspace. */
  relativePath: Schema.optional(TrimmedNonEmptyString),
  /** Absolute path for locations outside the workspace (read-only targets). */
  absolutePath: Schema.optional(TrimmedNonEmptyString),
  range: LspRange,
});
export type LspLocation = typeof LspLocation.Type;

export const LspLocationsResult = Schema.Struct({
  locations: Schema.Array(LspLocation),
});
export type LspLocationsResult = typeof LspLocationsResult.Type;

export const LspFileEdits = Schema.Struct({
  relativePath: TrimmedNonEmptyString,
  edits: Schema.Array(LspTextEdit),
});
export type LspFileEdits = typeof LspFileEdits.Type;

export const LspRenameInput = Schema.Struct({
  cwd: TrimmedNonEmptyString,
  relativePath: TrimmedNonEmptyString,
  position: LspPosition,
  newName: TrimmedNonEmptyString,
});
export type LspRenameInput = typeof LspRenameInput.Type;

export const LspWorkspaceEditResult = Schema.Struct({
  /** Per-file edit sets; empty when the server had nothing to change. */
  files: Schema.Array(LspFileEdits),
});
export type LspWorkspaceEditResult = typeof LspWorkspaceEditResult.Type;

export const LspFormattingInput = Schema.Struct({
  cwd: TrimmedNonEmptyString,
  relativePath: TrimmedNonEmptyString,
  tabSize: Schema.optional(NonNegativeInt),
  insertSpaces: Schema.optional(Schema.Boolean),
});
export type LspFormattingInput = typeof LspFormattingInput.Type;

export const LspFormattingResult = Schema.Struct({
  edits: Schema.Array(LspTextEdit),
});
export type LspFormattingResult = typeof LspFormattingResult.Type;

export const LspSignatureParameter = Schema.Struct({
  /** Parameter label, e.g. "value: string". */
  label: Schema.String,
  documentation: Schema.optional(Schema.String),
});
export type LspSignatureParameter = typeof LspSignatureParameter.Type;

export const LspSignature = Schema.Struct({
  /** Full signature label, e.g. "map(fn: (a: A) => B): Array<B>". */
  label: Schema.String,
  documentation: Schema.optional(Schema.String),
  parameters: Schema.Array(LspSignatureParameter),
});
export type LspSignature = typeof LspSignature.Type;

export const LspSignatureHelpResult = Schema.NullOr(
  Schema.Struct({
    signatures: Schema.Array(LspSignature),
    activeSignature: NonNegativeInt,
    activeParameter: NonNegativeInt,
  }),
);
export type LspSignatureHelpResult = typeof LspSignatureHelpResult.Type;

export const LspDiagnostic = Schema.Struct({
  range: LspRange,
  /** LSP DiagnosticSeverity: 1 error, 2 warning, 3 info, 4 hint. */
  severity: NonNegativeInt,
  message: Schema.String,
  source: Schema.optional(Schema.String),
  code: Schema.optional(Schema.String),
});
export type LspDiagnostic = typeof LspDiagnostic.Type;

export const LspDiagnosticsStreamEvent = Schema.Struct({
  relativePath: TrimmedNonEmptyString,
  /** Full replacement set for the document (LSP publishDiagnostics semantics). */
  diagnostics: Schema.Array(LspDiagnostic),
});
export type LspDiagnosticsStreamEvent = typeof LspDiagnosticsStreamEvent.Type;

export const LspSubscribeDiagnosticsInput = Schema.Struct({
  cwd: TrimmedNonEmptyString,
});
export type LspSubscribeDiagnosticsInput = typeof LspSubscribeDiagnosticsInput.Type;

export const LspServerStatus = Schema.Struct({
  /** Registry id, e.g. "typescript". */
  serverId: TrimmedNonEmptyString,
  /** Human-readable server name, e.g. "typescript-language-server". */
  displayName: TrimmedNonEmptyString,
  state: Schema.Literals(["starting", "running", "failed", "not_installed"]),
});
export type LspServerStatus = typeof LspServerStatus.Type;

export const LspServerStatusResult = Schema.Struct({
  servers: Schema.Array(LspServerStatus),
});
export type LspServerStatusResult = typeof LspServerStatusResult.Type;

export const LspFailure = Schema.Literals([
  "workspace_root_not_found",
  "unsupported_language",
  "server_not_installed",
  "server_start_failed",
  "server_crashed",
  "request_failed",
  "request_timed_out",
]);
export type LspFailure = typeof LspFailure.Type;

export class LspError extends Schema.TaggedErrorClass<LspError>()("LspError", {
  cwd: Schema.optional(TrimmedNonEmptyString),
  relativePath: Schema.optional(TrimmedNonEmptyString),
  failure: Schema.optional(LspFailure),
  serverId: Schema.optional(TrimmedNonEmptyString),
  detail: Schema.optional(TrimmedNonEmptyString),
  message: TrimmedNonEmptyString,
  cause: Schema.optional(Schema.Defect()),
}) {
  // Structured fields stay optional on the wire for cross-version decoding;
  // application code supplies them through this constructor.
  // @effect-diagnostics-next-line overriddenSchemaConstructor:off
  constructor(props: {
    readonly cwd: string;
    readonly relativePath?: string;
    readonly failure: LspFailure;
    readonly serverId?: string;
    readonly detail?: string;
    readonly cause?: unknown;
  }) {
    super({
      ...props,
      message: `Language service request failed (${props.failure}) in '${props.cwd}'.`,
    } as any);
  }
}
