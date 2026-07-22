/**
 * Client-visible view of which files the server's LSP proxy can handle.
 *
 * Must stay in sync with the server's language-server registry
 * (apps/server/src/lsp/LanguageServers.ts) — a server-side test asserts
 * parity. Clients use this to skip doc-sync and language queries for files
 * no language server will ever answer.
 *
 * @module lspSupport
 */
export const LSP_SUPPORTED_EXTENSIONS: ReadonlySet<string> = new Set([
  ".ts",
  ".mts",
  ".cts",
  ".tsx",
  ".js",
  ".mjs",
  ".cjs",
  ".jsx",
  ".rs",
  ".py",
  ".pyi",
  ".go",
]);

export function isLspSupportedPath(relativePath: string): boolean {
  const dotIndex = relativePath.lastIndexOf(".");
  if (dotIndex === -1) return false;
  return LSP_SUPPORTED_EXTENSIONS.has(relativePath.slice(dotIndex).toLowerCase());
}
