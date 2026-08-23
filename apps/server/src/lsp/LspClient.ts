// @effect-diagnostics nodeBuiltinImport:off
// @effect-diagnostics globalTimers:off
/**
 * LspClient - one running language-server process speaking LSP over stdio.
 *
 * Plain timers are intentional here: this module wraps vscode-jsonrpc's
 * callback-based connection and must not depend on an Effect runtime.
 *
 * Deliberately a plain promise-based class: vscode-jsonrpc owns the wire
 * protocol and its connection lifecycle callbacks; LspManager wraps this in
 * Effect and owns registry/refcount/diagnostics concerns. Documents are
 * synced with full text (TextDocumentSyncKind.Full).
 *
 * @module LspClient
 */
import * as NodeChildProcess from "node:child_process";
import * as NodeModule from "node:module";
import * as NodePath from "node:path";
import * as NodeURL from "node:url";

import {
  createMessageConnection,
  StreamMessageReader,
  StreamMessageWriter,
  type MessageConnection,
} from "vscode-jsonrpc/node.js";
import type * as Protocol from "vscode-languageserver-protocol";

import type { LanguageServerConfig } from "./LanguageServers.ts";

const REQUEST_TIMEOUT_MS = 15_000;
/** Default initialize budget; configs may override via initializeTimeoutMs. */
const INITIALIZE_TIMEOUT_MS = 30_000;
/** Grace period after SIGTERM before escalating to SIGKILL on dispose. */
const KILL_ESCALATION_MS = 5_000;

export type LspClientFailure =
  | { readonly kind: "not_installed" }
  | { readonly kind: "start_failed"; readonly detail: string }
  | { readonly kind: "crashed"; readonly detail: string }
  | { readonly kind: "request_failed"; readonly detail: string }
  | { readonly kind: "timed_out" };

export class LspClientError extends Error {
  readonly failure: LspClientFailure;

  constructor(failure: LspClientFailure) {
    super(`LSP client failure: ${failure.kind}`);
    this.failure = failure;
  }
}

export interface LspClientOptions {
  readonly workspaceRoot: string;
  readonly config: LanguageServerConfig;
  readonly onDiagnostics: (uri: string, diagnostics: ReadonlyArray<Protocol.Diagnostic>) => void;
  readonly onExit: (detail: string) => void;
}

function withTimeout<A>(promise: Promise<A>, timeoutMs: number): Promise<A> {
  return new Promise<A>((resolve, reject) => {
    const timer = setTimeout(() => reject(new LspClientError({ kind: "timed_out" })), timeoutMs);
    promise.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (error: unknown) => {
        clearTimeout(timer);
        reject(
          error instanceof LspClientError
            ? error
            : new LspClientError({ kind: "request_failed", detail: String(error) }),
        );
      },
    );
  });
}

/**
 * Rewrite a path resolved through Electron's app.asar to its app.asar.unpacked
 * twin. Child processes must not run from inside the archive: tsserver sets
 * `process.noAsar = true` before serving requests, after which every fs call
 * on an app.asar path reports "not found" — its default-lib directory (derived
 * from its own __filename) turns up empty and every ambient global (Pick,
 * JSON, Error, ...) becomes "Cannot find name". asarUnpack ships all of
 * node_modules on the real filesystem, so the unpacked twin always exists.
 * Outside a packaged build the path contains no app.asar and passes through.
 */
export function rewriteAsarPath(resolvedPath: string): string {
  return resolvedPath.replace(
    `${NodePath.sep}app.asar${NodePath.sep}`,
    `${NodePath.sep}app.asar.unpacked${NodePath.sep}`,
  );
}

function resolveSpawn(config: LanguageServerConfig): {
  readonly command: string;
  readonly args: ReadonlyArray<string>;
} {
  if (!config.bundled) return { command: config.command, args: config.args };
  // Bundled servers resolve from the app's node_modules and run under the
  // current Node executable so they work regardless of the user's PATH. vtsls
  // bundles its own TypeScript, so unlike a bare typescript-language-server it
  // needs no separately-shipped tsserver to drive.
  const require = NodeModule.createRequire(import.meta.url);
  const cliPath = rewriteAsarPath(require.resolve("@vtsls/language-server/bin/vtsls.js"));
  return { command: process.execPath, args: [cliPath, ...config.args] };
}

/**
 * Environment for spawned language servers. Node's watch mode (`node --watch`,
 * the dev runner) injects WATCH_REPORT_DEPENDENCIES=1 into our environment;
 * a language server that forks a Node child over IPC (vtsls forking tsserver)
 * would inherit it, making Node's watch reporter emit `{"watch:require": ...}`
 * IPC messages the server can't parse — vtsls throws "Unknown message type
 * undefined received" and exits 1 within seconds of starting.
 */
export function languageServerEnv(
  base: NodeJS.ProcessEnv = process.env,
): Record<string, string | undefined> {
  const env = { ...base };
  delete env.WATCH_REPORT_DEPENDENCIES;
  return env;
}

/**
 * VS Code-style workspace settings handed to vtsls.
 *
 * typescript-language-server read tsserver user preferences straight from
 * `initializationOptions.preferences`; vtsls instead only takes configuration
 * through `workspace/configuration` (and `didChangeConfiguration`), keyed by
 * the same `typescript.*` / `javascript.*` sections VS Code uses. We surface
 * only the auto-import knobs; everything else falls back to vtsls' bundled
 * defaults, which already match VS Code.
 */
const VTSLS_WORKSPACE_CONFIGURATION = {
  typescript: {
    suggest: { autoImports: true },
    preferences: { includePackageJsonAutoImports: "auto" },
  },
  javascript: {
    suggest: { autoImports: true },
    preferences: { includePackageJsonAutoImports: "auto" },
  },
} as const;

/**
 * Answer one `workspace/configuration` item. vtsls requests the whole tree
 * (empty section); non-TypeScript servers request their own sections, for
 * which we return null so they fall back to their defaults.
 */
function configurationForSection(section: string | undefined): unknown {
  if (section === undefined || section === "") return VTSLS_WORKSPACE_CONFIGURATION;
  return (VTSLS_WORKSPACE_CONFIGURATION as Record<string, unknown>)[section] ?? null;
}

export class LspClient {
  private connection: MessageConnection | null = null;
  private child: NodeChildProcess.ChildProcess | null = null;
  private disposed = false;
  private readonly options: LspClientOptions;

  private constructor(options: LspClientOptions) {
    this.options = options;
  }

  static async start(options: LspClientOptions): Promise<LspClient> {
    const client = new LspClient(options);
    try {
      await client.spawnAndInitialize();
    } catch (error) {
      // A failed initialize (timeout, error response, exit mid-handshake)
      // must not leak the spawned process; dispose() is safe when the child
      // already exited (kill on a dead pid is a no-op).
      await client.dispose().catch(() => {});
      throw error;
    }
    return client;
  }

  private async spawnAndInitialize(): Promise<void> {
    const { command, args } = resolveSpawn(this.options.config);
    const child = NodeChildProcess.spawn(command, [...args], {
      cwd: this.options.workspaceRoot,
      stdio: ["pipe", "pipe", "pipe"],
      env: languageServerEnv(),
    });
    this.child = child;

    const spawned = await new Promise<boolean>((resolve, reject) => {
      child.once("spawn", () => resolve(true));
      child.once("error", (error: NodeJS.ErrnoException) => {
        if (error.code === "ENOENT") {
          reject(new LspClientError({ kind: "not_installed" }));
        } else {
          reject(new LspClientError({ kind: "start_failed", detail: error.message }));
        }
      });
    });
    if (!spawned || child.stdout === null || child.stdin === null) {
      throw new LspClientError({ kind: "start_failed", detail: "missing stdio pipes" });
    }

    let stderrTail = "";
    child.stderr?.setEncoding("utf8");
    child.stderr?.on("data", (chunk: string) => {
      stderrTail = (stderrTail + chunk).slice(-2000);
    });
    child.on("exit", (code, signal) => {
      if (this.disposed) return;
      this.options.onExit(
        `exited with ${code ?? signal ?? "unknown"}: ${stderrTail.trim().slice(0, 500)}`,
      );
      // Reject in-flight requests (initialize included) now rather than
      // letting them run out their timeouts against a dead connection.
      this.connection?.dispose();
    });

    const connection = createMessageConnection(
      new StreamMessageReader(child.stdout),
      new StreamMessageWriter(child.stdin),
    );
    this.connection = connection;
    connection.onNotification(
      "textDocument/publishDiagnostics",
      (params: Protocol.PublishDiagnosticsParams) => {
        this.options.onDiagnostics(params.uri, params.diagnostics);
      },
    );
    // Some servers (tsserver) send requests we don't need; answer politely.
    connection.onRequest(
      "workspace/configuration",
      (params: { readonly items: ReadonlyArray<{ readonly section?: string }> }) =>
        params.items.map((item) => configurationForSection(item.section)),
    );
    connection.onRequest("window/workDoneProgress/create", () => null);
    connection.onError(() => {});
    connection.listen();

    const rootUri = NodeURL.pathToFileURL(this.options.workspaceRoot).toString();
    const initializationOptions = await this.resolveInitializationOptions();
    const initializeParams: Protocol.InitializeParams = {
      processId: process.pid,
      rootUri,
      workspaceFolders: [{ uri: rootUri, name: "workspace" }],
      ...(initializationOptions !== undefined ? { initializationOptions } : {}),
      capabilities: {
        textDocument: {
          // LspManager forwards didSave from workspace file changes. It gates
          // save-triggered analysis: rust-analyzer runs `cargo check` only on
          // save, so without it those diagnostics never refresh.
          synchronization: { didSave: true },
          completion: {
            contextSupport: true,
            completionItem: {
              snippetSupport: false,
              documentationFormat: ["markdown", "plaintext"],
              // Lazy resolution: servers defer expensive fields (docs,
              // auto-import edits) to completionItem/resolve.
              resolveSupport: {
                properties: ["documentation", "detail", "additionalTextEdits"],
              },
            },
          },
          hover: { contentFormat: ["markdown", "plaintext"] },
          signatureHelp: {
            signatureInformation: {
              documentationFormat: ["markdown", "plaintext"],
              parameterInformation: { labelOffsetSupport: false },
              activeParameterSupport: true,
            },
          },
          publishDiagnostics: {},
          definition: {},
          references: {},
          rename: {},
          formatting: {},
        },
        // vtsls pulls settings via workspace/configuration during init; the
        // request handler above returns VTSLS_WORKSPACE_CONFIGURATION.
        workspace: { workspaceFolders: true, configuration: true },
      },
    };
    await withTimeout(
      connection.sendRequest("initialize", initializeParams),
      this.options.config.initializeTimeoutMs ?? INITIALIZE_TIMEOUT_MS,
    );
    await connection.sendNotification("initialized", {});
  }

  /**
   * Workspace-specific initialize options, or undefined when the server has no
   * resolver or the resolver declines. A resolver failure degrades to the
   * server's defaults rather than aborting the handshake: losing
   * `linkedProjects` costs language features, while failing the start costs
   * the editor its server entirely.
   */
  private async resolveInitializationOptions(): Promise<Record<string, unknown> | undefined> {
    const resolve = this.options.config.resolveInitializationOptions;
    if (resolve === undefined) return undefined;
    try {
      return await resolve(this.options.workspaceRoot);
    } catch {
      return undefined;
    }
  }

  private requireConnection(): MessageConnection {
    if (this.connection === null || this.disposed) {
      throw new LspClientError({ kind: "crashed", detail: "connection is not available" });
    }
    return this.connection;
  }

  notify(method: string, params: object): Promise<void> {
    return Promise.resolve(this.requireConnection().sendNotification(method, params));
  }

  request<A>(method: string, params: object): Promise<A> {
    return withTimeout(this.requireConnection().sendRequest<A>(method, params), REQUEST_TIMEOUT_MS);
  }

  async dispose(): Promise<void> {
    if (this.disposed) return;
    this.disposed = true;
    try {
      if (this.connection !== null) {
        await withTimeout(this.connection.sendRequest("shutdown", null), 2000).catch(() => {});
        await this.connection.sendNotification("exit", null).catch(() => {});
        this.connection.dispose();
      }
    } catch {
      // Connection already torn down (e.g. by the exit handler after the
      // child died); sendRequest throws synchronously on a disposed
      // connection and there is nothing left to say goodbye to.
    } finally {
      this.killChild();
    }
  }

  /**
   * Kill the child, escalating to SIGKILL for servers that ignore SIGTERM —
   * a server hung during initialize is exactly that case. No-op when the
   * child already exited.
   */
  private killChild(): void {
    const child = this.child;
    if (child === null || child.exitCode !== null || child.signalCode !== null) return;
    child.kill();
    const escalation = setTimeout(() => {
      if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
    }, KILL_ESCALATION_MS);
    escalation.unref();
    child.once("exit", () => clearTimeout(escalation));
  }
}
