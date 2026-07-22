/**
 * LanguageServers - registry of supported language servers.
 *
 * Built-in servers are static defaults: the TypeScript server ships with the
 * app (dependency of apps/server); other built-ins are resolved from PATH and
 * degrade to a `not_installed` status when absent. User-defined servers from
 * the `languageServers` server setting are merged in via `resolveRegistry`;
 * they are never bundled and always spawn from PATH.
 *
 * @module LanguageServers
 */
import type { CustomLanguageServer } from "@t3tools/contracts";

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
  displayName: "vtsls",
  command: "vtsls",
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

export interface LanguageBinding {
  readonly server: LanguageServerConfig;
  readonly languageId: string;
}

export const BUILTIN_EXTENSION_BINDINGS: Record<string, LanguageBinding> = {
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

export const BUILTIN_SERVERS: ReadonlyArray<LanguageServerConfig> = [
  TYPESCRIPT_SERVER,
  RUST_ANALYZER,
  PYRIGHT,
  GOPLS,
];

export const BUILTIN_EXTENSIONS: ReadonlySet<string> = new Set(
  Object.keys(BUILTIN_EXTENSION_BINDINGS),
);

export interface ResolvedRegistry {
  readonly servers: ReadonlyArray<LanguageServerConfig>;
  readonly bindings: Readonly<Record<string, LanguageBinding>>;
  readonly supportedExtensions: ReadonlyArray<string>;
  readonly bindingForPath: (relativePath: string) => LanguageBinding | null;
}

function bindingForPathIn(
  bindings: Readonly<Record<string, LanguageBinding>>,
  relativePath: string,
): LanguageBinding | null {
  const dotIndex = relativePath.lastIndexOf(".");
  if (dotIndex === -1) return null;
  const extension = relativePath.slice(dotIndex).toLowerCase();
  return bindings[extension] ?? null;
}

/**
 * Merge built-in servers with user-defined ones into an effective registry.
 * Pure. Built-ins win on serverId/extension conflicts — settings validation
 * already rejects collisions; this is defense-in-depth.
 */
export function resolveRegistry(custom: ReadonlyArray<CustomLanguageServer>): ResolvedRegistry {
  const bindings: Record<string, LanguageBinding> = { ...BUILTIN_EXTENSION_BINDINGS };
  const servers: LanguageServerConfig[] = [...BUILTIN_SERVERS];
  for (const entry of custom) {
    if (servers.some((server) => server.serverId === entry.serverId)) continue;
    const config: LanguageServerConfig = {
      serverId: entry.serverId,
      displayName: entry.displayName,
      command: entry.command,
      args: entry.args,
      bundled: false,
    };
    servers.push(config);
    for (const extension of entry.extensions) {
      if (extension in bindings) continue;
      bindings[extension] = { server: config, languageId: entry.languageId };
    }
  }
  return {
    servers,
    bindings,
    supportedExtensions: Object.keys(bindings),
    bindingForPath: (relativePath) => bindingForPathIn(bindings, relativePath),
  };
}

/** Resolve the built-in language binding for a workspace-relative path, if any. */
export function languageBindingForPath(relativePath: string): LanguageBinding | null {
  return bindingForPathIn(BUILTIN_EXTENSION_BINDINGS, relativePath);
}
