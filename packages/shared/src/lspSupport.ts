/**
 * Client-visible view of which files the server's *built-in* LSP servers can
 * handle.
 *
 * Must stay in sync with the server's built-in language-server registry
 * (apps/server/src/lsp/LanguageServers.ts) — a server-side test asserts
 * parity. User-defined servers add extensions at runtime; clients discover
 * those via the `supportedExtensions` field of `lsp.serverStatus` and use
 * this static set only as the pre-load fallback.
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
