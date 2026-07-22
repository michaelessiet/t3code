/**
 * LanguageServers - static registry of supported language servers.
 *
 * Maps file extensions to the language server that should handle them plus
 * the LSP `languageId` sent in document-sync notifications. The TypeScript
 * server ships with the app (dependency of apps/server); other servers are
 * resolved from PATH and degrade to a `not_installed` status when absent.
 *
 * @module LanguageServers
 */

export interface LanguageServerConfig {
  /** Stable id used in status reporting and registry keys. */
  readonly serverId: string;
  readonly displayName: string;
  /** Executable + args; `bundled:` command values resolve from node_modules. */
  readonly command: string;
  readonly args: ReadonlyArray<string>;
  /** True when the binary ships with the app rather than user PATH. */
  readonly bundled: boolean;
}

const TYPESCRIPT_SERVER: LanguageServerConfig = {
  serverId: "typescript",
  displayName: "typescript-language-server",
  command: "typescript-language-server",
  args: ["--stdio"],
  bundled: true,
};

const RUST_ANALYZER: LanguageServerConfig = {
  serverId: "rust",
  displayName: "rust-analyzer",
  command: "rust-analyzer",
  args: [],
  bundled: false,
};

const PYRIGHT: LanguageServerConfig = {
  serverId: "python",
  displayName: "pyright-langserver",
  command: "pyright-langserver",
  args: ["--stdio"],
  bundled: false,
};

const GOPLS: LanguageServerConfig = {
  serverId: "go",
  displayName: "gopls",
  command: "gopls",
  args: [],
  bundled: false,
};

interface LanguageBinding {
  readonly server: LanguageServerConfig;
  readonly languageId: string;
}

const EXTENSION_BINDINGS: Record<string, LanguageBinding> = {
  ".ts": { server: TYPESCRIPT_SERVER, languageId: "typescript" },
  ".mts": { server: TYPESCRIPT_SERVER, languageId: "typescript" },
  ".cts": { server: TYPESCRIPT_SERVER, languageId: "typescript" },
  ".tsx": { server: TYPESCRIPT_SERVER, languageId: "typescriptreact" },
  ".js": { server: TYPESCRIPT_SERVER, languageId: "javascript" },
  ".mjs": { server: TYPESCRIPT_SERVER, languageId: "javascript" },
  ".cjs": { server: TYPESCRIPT_SERVER, languageId: "javascript" },
  ".jsx": { server: TYPESCRIPT_SERVER, languageId: "javascriptreact" },
  ".rs": { server: RUST_ANALYZER, languageId: "rust" },
  ".py": { server: PYRIGHT, languageId: "python" },
  ".pyi": { server: PYRIGHT, languageId: "python" },
  ".go": { server: GOPLS, languageId: "go" },
};

export const KNOWN_SERVERS: ReadonlyArray<LanguageServerConfig> = [
  TYPESCRIPT_SERVER,
  RUST_ANALYZER,
  PYRIGHT,
  GOPLS,
];

/** Resolve the language binding for a workspace-relative path, if any. */
export function languageBindingForPath(relativePath: string): LanguageBinding | null {
  const dotIndex = relativePath.lastIndexOf(".");
  if (dotIndex === -1) return null;
  const extension = relativePath.slice(dotIndex).toLowerCase();
  return EXTENSION_BINDINGS[extension] ?? null;
}
