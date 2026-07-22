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
const INITIALIZE_TIMEOUT_MS = 30_000;

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

function resolveSpawn(config: LanguageServerConfig): {
  readonly command: string;
  readonly args: ReadonlyArray<string>;
} {
  if (!config.bundled) return { command: config.command, args: config.args };
  // Bundled servers resolve from the app's node_modules and run under the
  // current Node executable so they work regardless of the user's PATH.
  const require = NodeModule.createRequire(import.meta.url);
  const cliPath = require.resolve("typescript-language-server/lib/cli.mjs");
  return { command: process.execPath, args: [cliPath, ...config.args] };
}

/**
 * tsserver.js from the `typescript` package shipped with this server.
 *
 * typescript-language-server does NOT depend on typescript; it discovers
 * tsserver by walking up from its own install or the workspace. That works
 * from a source checkout (the monorepo has typescript) but not from the
 * packaged desktop app, where nothing above `app.asar.unpacked/node_modules`
 * exists. Shipping typescript and passing it explicitly as the *fallback*
 * keeps packaged builds working while still preferring a workspace-local
 * typescript version when the opened project has one.
 */
function resolveBundledTsserverPath(): string | undefined {
  try {
    const require = NodeModule.createRequire(import.meta.url);
    return require.resolve("typescript/lib/tsserver.js");
  } catch {
    return undefined;
  }
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
    await client.spawnAndInitialize();
    return client;
  }

  private async spawnAndInitialize(): Promise<void> {
    const { command, args } = resolveSpawn(this.options.config);
    const child = NodeChildProcess.spawn(command, [...args], {
      cwd: this.options.workspaceRoot,
      stdio: ["pipe", "pipe", "pipe"],
      env: process.env,
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
    connection.onRequest("workspace/configuration", () => [null]);
    connection.onRequest("window/workDoneProgress/create", () => null);
    connection.onError(() => {});
    connection.listen();

    const rootUri = NodeURL.pathToFileURL(this.options.workspaceRoot).toString();
    const initializeParams: Protocol.InitializeParams = {
      processId: process.pid,
      rootUri,
      workspaceFolders: [{ uri: rootUri, name: "workspace" }],
      capabilities: {
        textDocument: {
          synchronization: { didSave: false },
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
        workspace: { workspaceFolders: true, configuration: false },
      },
      // typescript-language-server reads tsserver user preferences from
      // initializationOptions; auto-import and module-specifier completions
      // hinge on these. Other servers ignore unknown options.
      initializationOptions: {
        preferences: {
          includeCompletionsForModuleExports: true,
          includeCompletionsForImportStatements: true,
          includeCompletionsWithInsertText: true,
          includeAutomaticOptionalChainCompletions: true,
          includePackageJsonAutoImports: "auto",
        },
        ...(() => {
          const fallbackPath = this.options.config.bundled
            ? resolveBundledTsserverPath()
            : undefined;
          return fallbackPath === undefined ? {} : { tsserver: { fallbackPath } };
        })(),
      },
    };
    await withTimeout(
      connection.sendRequest("initialize", initializeParams),
      INITIALIZE_TIMEOUT_MS,
    );
    await connection.sendNotification("initialized", {});
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
    } finally {
      this.child?.kill();
    }
  }
}
