/**
 * LspManager - Effect service owning language-server lifecycles per
 * (workspace root, server) pair and proxying language-intelligence requests.
 *
 * Servers start lazily on the first document open for a supported language,
 * are shared across clients, and shut down after all documents have been
 * closed for an idle period. Failures are cached so a missing binary or a
 * crash-looping server doesn't respawn-loop (crash exits back off
 * exponentially), then surface as typed LspError failures the UI can render
 * ("rust-analyzer is not installed").
 *
 * Diagnostics push through a service-level PubSub keyed by workspace root,
 * mirroring the WorkspaceWatcher/VcsStatusBroadcaster broadcast pattern.
 *
 * @module LspManager
 */
import {
  LspError,
  type CustomLanguageServer,
  type LspCompletionItem,
  type LspCompletionResult,
  type LspDiagnosticsStreamEvent,
  type LspDidChangeInput,
  type LspDidOpenInput,
  type LspDocumentInput,
  type LspFormattingInput,
  type LspFormattingResult,
  type LspHoverResult,
  type LspLocationsResult,
  type LspPositionInput,
  type LspRenameInput,
  type LspCodeAction,
  type LspCodeActionsInput,
  type LspCodeActionsResult,
  type LspResolveCodeActionInput,
  type LspSemanticTokensResult,
  type LspResolveCompletionInput,
  type LspServerStatusResult,
  type LspSignatureHelpResult,
  type LspWorkspaceEditResult,
  type ProjectWatchStreamEvent,
} from "@t3tools/contracts";
import * as Clock from "effect/Clock";
import * as Context from "effect/Context";
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as Exit from "effect/Exit";
import * as Fiber from "effect/Fiber";
import * as Layer from "effect/Layer";
import * as PubSub from "effect/PubSub";
import * as Ref from "effect/Ref";
import * as Scope from "effect/Scope";
import * as Stream from "effect/Stream";
import type * as Protocol from "vscode-languageserver-protocol";
import { fullDocumentRange, mapCodeAction, mapSemanticTokens } from "./LspEditorFeatures.ts";

import { resolveRegistry, type LanguageServerConfig } from "./LanguageServers.ts";
import { LspClient, LspClientError } from "./LspClient.ts";
import {
  documentUri,
  mapCompletion,
  mapCompletionItem,
  mapDiagnostics,
  mapHover,
  mapLocations,
  mapSignatureHelp,
  mapTextEdits,
  uriToLocationPath,
} from "./LspMappings.ts";
import { ServerSettingsService } from "../serverSettings.ts";
import * as WorkspacePaths from "../workspace/WorkspacePaths.ts";
import { WorkspaceWatcher } from "../workspace/WorkspaceWatcher.ts";

const IDLE_SHUTDOWN_AFTER = Duration.minutes(5);
const IDLE_SWEEP_INTERVAL = Duration.minutes(1);
const FAILURE_RETRY_AFTER = Duration.minutes(1);
/** Ceiling for the exponential crash-retry backoff. */
const CRASH_RETRY_MAX = Duration.minutes(10);
/** A run at least this long counts as healthy and resets the crash streak. */
const CRASH_RESET_AFTER = Duration.minutes(5);

type DiagnosticsChange = {
  readonly cwd: string;
  readonly event: LspDiagnosticsStreamEvent;
};

type ManagedServer = {
  readonly serverId: string;
  readonly displayName: string;
  readonly workspaceRoot: string;
  clientPromise: Promise<LspClient>;
  state: "starting" | "running" | "failed" | "not_installed";
  readonly openDocuments: Map<string, { readonly version: number; readonly contents: string }>;
  lastActivityAt: number;
  failedAt: number | null;
  /** Crash exits without a healthy run in between; drives retry backoff. */
  consecutiveCrashes: number;
  /** Clock time the client was first observed running; null until then. */
  runningAt: number | null;
};

/**
 * How long a failed server stays cached before a retry is allowed. Start
 * failures (missing binary) retry on a flat interval; crash exits back off
 * exponentially (1min, 2min, 4min, ...) so a crash-looping server does not
 * respawn per keystroke.
 */
function retryDelayMillis(managed: ManagedServer): number {
  return Math.min(
    Duration.toMillis(FAILURE_RETRY_AFTER) * 2 ** Math.max(0, managed.consecutiveCrashes - 1),
    Duration.toMillis(CRASH_RETRY_MAX),
  );
}

function serverKey(workspaceRoot: string, serverId: string): string {
  return `${workspaceRoot}\0${serverId}`;
}

function clientFailureToLspError(
  error: unknown,
  context: { readonly cwd: string; readonly relativePath?: string; readonly serverId: string },
): LspError {
  if (error instanceof LspClientError) {
    switch (error.failure.kind) {
      case "not_installed":
        return new LspError({ ...context, failure: "server_not_installed" });
      case "start_failed":
        return new LspError({
          ...context,
          failure: "server_start_failed",
          detail: error.failure.detail,
        });
      case "crashed":
        return new LspError({
          ...context,
          failure: "server_crashed",
          detail: error.failure.detail,
        });
      case "timed_out":
        return new LspError({ ...context, failure: "request_timed_out" });
      case "request_failed":
        return new LspError({
          ...context,
          failure: "request_failed",
          detail: error.failure.detail,
        });
    }
  }
  return new LspError({ ...context, failure: "request_failed", cause: error });
}

/** Service tag for the language-intelligence proxy. */
export class LspManager extends Context.Service<
  LspManager,
  {
    readonly didOpen: (input: LspDidOpenInput) => Effect.Effect<void, LspError>;
    readonly didChange: (input: LspDidChangeInput) => Effect.Effect<void, LspError>;
    readonly didClose: (input: LspDocumentInput) => Effect.Effect<void, LspError>;
    readonly completion: (input: LspPositionInput) => Effect.Effect<LspCompletionResult, LspError>;
    readonly resolveCompletion: (
      input: LspResolveCompletionInput,
    ) => Effect.Effect<LspCompletionItem, LspError>;
    readonly signatureHelp: (
      input: LspPositionInput,
    ) => Effect.Effect<LspSignatureHelpResult, LspError>;
    readonly hover: (input: LspPositionInput) => Effect.Effect<LspHoverResult, LspError>;
    readonly definition: (input: LspPositionInput) => Effect.Effect<LspLocationsResult, LspError>;
    readonly references: (input: LspPositionInput) => Effect.Effect<LspLocationsResult, LspError>;
    readonly rename: (input: LspRenameInput) => Effect.Effect<LspWorkspaceEditResult, LspError>;
    readonly codeActions: (
      input: LspCodeActionsInput,
    ) => Effect.Effect<LspCodeActionsResult, LspError>;
    readonly resolveCodeAction: (
      input: LspResolveCodeActionInput,
    ) => Effect.Effect<LspCodeAction, LspError>;
    readonly semanticTokens: (
      input: LspCodeActionsInput,
    ) => Effect.Effect<LspSemanticTokensResult, LspError>;
    readonly format: (input: LspFormattingInput) => Effect.Effect<LspFormattingResult, LspError>;
    readonly serverStatus: (input: {
      readonly cwd: string;
    }) => Effect.Effect<LspServerStatusResult, LspError>;
    readonly subscribeDiagnostics: (input: {
      readonly cwd: string;
    }) => Stream.Stream<LspDiagnosticsStreamEvent, LspError>;
  }
>()("t3/lsp/LspManager") {}

export const make = Effect.gen(function* () {
  const workspacePaths = yield* WorkspacePaths.WorkspacePaths;
  const serverSettings = yield* ServerSettingsService;
  const workspaceWatcher = yield* WorkspaceWatcher;
  const rawDiagnostics = new Map<string, ReadonlyArray<Protocol.Diagnostic>>();

  // Custom-server definitions from user settings, kept fresh by the
  // settings watcher below. Read per-request and merged with built-ins via
  // resolveRegistry (cheap: a handful of entries).
  const initialCustomServers = yield* serverSettings.getSettings.pipe(
    Effect.map((settings) => settings.languageServers),
    Effect.orElseSucceed((): ReadonlyArray<CustomLanguageServer> => []),
  );
  const customServersRef = yield* Ref.make(initialCustomServers);
  const currentRegistry = Effect.map(Ref.get(customServersRef), resolveRegistry);

  const diagnosticsPubSub = yield* Effect.acquireRelease(
    PubSub.unbounded<DiagnosticsChange>(),
    (pubsub) => PubSub.shutdown(pubsub),
  );
  const managerScope = yield* Effect.acquireRelease(Scope.make(), (scope) =>
    Scope.close(scope, Exit.void),
  );
  // Mutated only from Effect land; plain Map keeps the promise-based client
  // lifecycle (start/dispose) straightforward.
  const servers = new Map<string, ManagedServer>();

  const runtimePublish = (change: DiagnosticsChange) =>
    Effect.runFork(PubSub.publish(diagnosticsPubSub, change));

  const normalizeRoot = (cwd: string, relativePath?: string) =>
    workspacePaths.normalizeWorkspaceRoot(cwd).pipe(
      Effect.mapError(
        (cause) =>
          new LspError({
            cwd,
            ...(relativePath !== undefined ? { relativePath } : {}),
            failure: "workspace_root_not_found",
            cause,
          }),
      ),
    );

  const startServer = (
    workspaceRoot: string,
    config: LanguageServerConfig,
    now: number,
    previousCrashes = 0,
  ): ManagedServer => {
    const managed: ManagedServer = {
      serverId: config.serverId,
      displayName: config.displayName,
      workspaceRoot,
      state: "starting",
      openDocuments: new Map(),
      lastActivityAt: now,
      failedAt: null,
      consecutiveCrashes: previousCrashes,
      runningAt: null,
      clientPromise: LspClient.start({
        workspaceRoot,
        config,
        onDiagnostics: (uri, diagnostics) => {
          const location = uriToLocationPath(workspaceRoot, uri);
          if (location.relativePath === undefined) return;
          if (diagnostics.length === 0) rawDiagnostics.delete(uri);
          else rawDiagnostics.set(uri, diagnostics);
          runtimePublish({
            cwd: workspaceRoot,
            event: {
              relativePath: location.relativePath,
              diagnostics: mapDiagnostics(diagnostics),
            },
          });
        },
        onExit: (detail) => {
          // Keep the entry in the map so the failure cache gates respawns;
          // deleting it here would let the next didChange respawn a
          // crash-looping server at keystroke rate.
          managed.state = "failed";
          // A dead server's diagnostics are stale; mirror the settings-watcher
          // cleanup so clients drop the squiggles.
          for (const uri of managed.openDocuments.keys()) {
            rawDiagnostics.delete(uri);
            const location = uriToLocationPath(workspaceRoot, uri);
            if (location.relativePath === undefined) continue;
            runtimePublish({
              cwd: workspaceRoot,
              event: { relativePath: location.relativePath, diagnostics: [] },
            });
          }
          managed.openDocuments.clear();
          // Later requests must observe the crash (and start the retry
          // backoff) instead of talking to a dead connection.
          const crashed = Promise.reject(new LspClientError({ kind: "crashed", detail }));
          crashed.catch(() => {});
          managed.clientPromise = crashed;
        },
      }),
    };
    managed.clientPromise.then(
      () => {
        // A crash observed between initialize completing and start()
        // resolving must win; only a still-starting server becomes running.
        if (managed.state === "starting") managed.state = "running";
      },
      (error: unknown) => {
        if (managed.state !== "starting") return;
        managed.state =
          error instanceof LspClientError && error.failure.kind === "not_installed"
            ? "not_installed"
            : "failed";
      },
    );
    servers.set(serverKey(workspaceRoot, config.serverId), managed);
    return managed;
  };

  const getOrStartServer = (
    workspaceRoot: string,
    config: LanguageServerConfig,
    now: number,
  ): ManagedServer => {
    const key = serverKey(workspaceRoot, config.serverId);
    const existing = servers.get(key);
    if (existing !== undefined) {
      const failed = existing.state === "failed" || existing.state === "not_installed";
      const retryDue =
        existing.failedAt !== null && now - existing.failedAt > retryDelayMillis(existing);
      if (!failed || !retryDue) return existing;
      servers.delete(key);
      // Carry the crash streak into the respawn so a crash loop keeps
      // doubling its backoff; a healthy run resets it (see clientFor).
      return startServer(workspaceRoot, config, now, existing.consecutiveCrashes);
    }
    return startServer(workspaceRoot, config, now);
  };

  /**
   * Per-workspace fibers forwarding `textDocument/didSave`. A reserved entry is
   * inserted synchronously (null until its fiber exists) so two concurrent
   * document opens cannot start two watches on the same root.
   */
  const saveWatchers = new Map<string, Fiber.Fiber<void> | null>();

  /**
   * Tell servers a document was saved, so save-triggered analysis re-runs.
   *
   * rust-analyzer only re-runs `cargo check` on save, and its most valuable
   * diagnostics (the `rustc`-sourced type errors) come from there — without a
   * didSave they persist unchanged after the code that caused them is fixed,
   * which is worse than reporting nothing. Disk is the right trigger rather
   * than an editor save action: agents write files directly, and every client
   * gets the refresh without its own plumbing.
   */
  const notifySaved = Effect.fn("LspManager.notifySaved")(function* (
    workspaceRoot: string,
    event: ProjectWatchStreamEvent,
  ) {
    const registry = yield* currentRegistry;
    const targets: Array<{ readonly managed: ManagedServer; readonly uri: string }> = [];
    if (event._tag === "overflow") {
      // The changed paths are unknown, so refresh everything currently open.
      // Open documents are editor tabs, which keeps this small.
      for (const managed of servers.values()) {
        if (managed.workspaceRoot !== workspaceRoot || managed.state !== "running") continue;
        for (const uri of managed.openDocuments.keys()) targets.push({ managed, uri });
      }
    } else {
      for (const path of event.paths) {
        const binding = registry.bindingForPath(path);
        if (binding === null) continue;
        const managed = servers.get(serverKey(workspaceRoot, binding.server.serverId));
        if (managed === undefined || managed.state !== "running") continue;
        const uri = documentUri(workspaceRoot, path);
        // didSave is only meaningful for documents the server has open.
        if (!managed.openDocuments.has(uri)) continue;
        targets.push({ managed, uri });
      }
    }
    for (const { managed, uri } of targets) {
      // Best effort: a server that died or rejects the notification must not
      // tear down the watch loop for the rest of the workspace.
      yield* Effect.promise(() =>
        managed.clientPromise
          .then((client) => client.notify("textDocument/didSave", { textDocument: { uri } }))
          .then(
            () => undefined,
            () => undefined,
          ),
      );
    }
  });

  /** Start this workspace's save-forwarding watch, once. */
  const ensureSaveWatcher = Effect.fn("LspManager.ensureSaveWatcher")(function* (
    workspaceRoot: string,
  ) {
    if (saveWatchers.has(workspaceRoot)) return;
    saveWatchers.set(workspaceRoot, null);
    const fiber = yield* workspaceWatcher.subscribe({ cwd: workspaceRoot }).pipe(
      Stream.runForEach((event) => notifySaved(workspaceRoot, event)),
      // A watch that cannot start costs diagnostic freshness, not correctness.
      Effect.catchCause(() => Effect.void),
      Effect.forkIn(managerScope),
    );
    saveWatchers.set(workspaceRoot, fiber);
  });

  /** Drop a workspace's watch once it has no servers left to notify. */
  const releaseSaveWatcher = Effect.fn("LspManager.releaseSaveWatcher")(function* (
    workspaceRoot: string,
  ) {
    for (const managed of servers.values()) {
      if (managed.workspaceRoot === workspaceRoot) return;
    }
    const fiber = saveWatchers.get(workspaceRoot);
    saveWatchers.delete(workspaceRoot);
    if (fiber != null) yield* Fiber.interrupt(fiber);
  });

  /** Resolve root + binding + running client for a document-scoped request. */
  const clientFor = Effect.fn("LspManager.clientFor")(function* (input: {
    readonly cwd: string;
    readonly relativePath: string;
  }) {
    const workspaceRoot = yield* normalizeRoot(input.cwd, input.relativePath);
    const registry = yield* currentRegistry;
    const binding = registry.bindingForPath(input.relativePath);
    if (binding === null) {
      return yield* new LspError({
        cwd: input.cwd,
        relativePath: input.relativePath,
        failure: "unsupported_language",
      });
    }
    const now = yield* Clock.currentTimeMillis;
    const managed = getOrStartServer(workspaceRoot, binding.server, now);
    managed.lastActivityAt = now;
    yield* ensureSaveWatcher(workspaceRoot);
    const client = yield* Effect.tryPromise({
      try: () => managed.clientPromise,
      catch: (error) =>
        clientFailureToLspError(error, {
          cwd: input.cwd,
          relativePath: input.relativePath,
          serverId: binding.server.serverId,
        }),
    }).pipe(
      // Failure timestamps are recorded when an Effect first observes the
      // failed start so retry backoff uses Clock time, not wall-callback time.
      Effect.tapError((error) =>
        Clock.currentTimeMillis.pipe(
          Effect.map((failureNow) => {
            if (managed.failedAt !== null) return;
            managed.failedAt = failureNow;
            if (error.failure !== "server_crashed") return;
            // First observation of a crash exit: extend the streak, or reset
            // it when the server had survived a healthy stretch first.
            const healthy =
              managed.runningAt !== null &&
              failureNow - managed.runningAt >= Duration.toMillis(CRASH_RESET_AFTER);
            managed.consecutiveCrashes = healthy ? 1 : managed.consecutiveCrashes + 1;
          }),
        ),
      ),
    );
    if (managed.runningAt === null) {
      // Anchors the healthy-uptime check above in Clock time.
      managed.runningAt = yield* Clock.currentTimeMillis;
    }
    return { workspaceRoot, binding, managed, client };
  });

  const clientRequest = <A>(
    input: { readonly cwd: string; readonly relativePath: string },
    run: (context: {
      readonly workspaceRoot: string;
      readonly client: LspClient;
      readonly uri: string;
    }) => Promise<A>,
  ): Effect.Effect<A, LspError> =>
    clientFor(input).pipe(
      Effect.flatMap(({ workspaceRoot, binding, client }) =>
        Effect.tryPromise({
          try: () =>
            run({ workspaceRoot, client, uri: documentUri(workspaceRoot, input.relativePath) }),
          catch: (error) =>
            clientFailureToLspError(error, {
              cwd: input.cwd,
              relativePath: input.relativePath,
              serverId: binding.server.serverId,
            }),
        }),
      ),
    );

  const didOpen: LspManager["Service"]["didOpen"] = Effect.fn("LspManager.didOpen")(
    function* (input) {
      if ((yield* currentRegistry).bindingForPath(input.relativePath) === null) return;
      const { workspaceRoot, binding, managed, client } = yield* clientFor(input);
      const uri = documentUri(workspaceRoot, input.relativePath);
      managed.openDocuments.set(uri, { version: 0, contents: input.contents });
      yield* Effect.tryPromise({
        try: () =>
          client.notify("textDocument/didOpen", {
            textDocument: {
              uri,
              languageId: binding.languageId,
              version: 0,
              text: input.contents,
            },
          }),
        catch: (error) =>
          clientFailureToLspError(error, {
            cwd: input.cwd,
            relativePath: input.relativePath,
            serverId: binding.server.serverId,
          }),
      });
    },
  );

  const didChange: LspManager["Service"]["didChange"] = Effect.fn("LspManager.didChange")(
    function* (input) {
      if ((yield* currentRegistry).bindingForPath(input.relativePath) === null) return;
      const { workspaceRoot, binding, managed, client } = yield* clientFor(input);
      const uri = documentUri(workspaceRoot, input.relativePath);
      const previous = managed.openDocuments.get(uri);
      const sync = client.serverCapabilities.textDocumentSync;
      const incremental = (typeof sync === "number" ? sync : sync?.change) === 2;
      managed.openDocuments.set(uri, { version: input.version, contents: input.contents });
      yield* Effect.tryPromise({
        try: () =>
          incremental && previous === undefined
            ? // A newly restarted server needs the document before any changes.
              client.notify("textDocument/didOpen", {
                textDocument: {
                  uri,
                  languageId: binding.languageId,
                  version: input.version,
                  text: input.contents,
                },
              })
            : client.notify("textDocument/didChange", {
                textDocument: { uri, version: input.version },
                contentChanges: [
                  {
                    ...(incremental && previous !== undefined
                      ? { range: fullDocumentRange(previous.contents) }
                      : {}),
                    text: input.contents,
                  },
                ],
              }),
        catch: (error) =>
          clientFailureToLspError(error, {
            cwd: input.cwd,
            relativePath: input.relativePath,
            serverId: binding.server.serverId,
          }),
      });
    },
  );

  const didClose: LspManager["Service"]["didClose"] = Effect.fn("LspManager.didClose")(
    function* (input) {
      if ((yield* currentRegistry).bindingForPath(input.relativePath) === null) return;
      const { workspaceRoot, binding, managed, client } = yield* clientFor(input);
      const uri = documentUri(workspaceRoot, input.relativePath);
      managed.openDocuments.delete(uri);
      rawDiagnostics.delete(uri);
      yield* Effect.tryPromise({
        try: () => client.notify("textDocument/didClose", { textDocument: { uri } }),
        catch: (error) =>
          clientFailureToLspError(error, {
            cwd: input.cwd,
            relativePath: input.relativePath,
            serverId: binding.server.serverId,
          }),
      });
      // Closing the last document publishes an empty diagnostic set so
      // clients drop stale squiggles for files nobody has open.
      runtimePublish({
        cwd: workspaceRoot,
        event: { relativePath: input.relativePath, diagnostics: [] },
      });
    },
  );

  const positionParams = (uri: string, input: LspPositionInput) => ({
    textDocument: { uri },
    position: input.position,
  });

  const completion: LspManager["Service"]["completion"] = (input) =>
    clientRequest(input, async ({ client, uri }) =>
      mapCompletion(await client.request("textDocument/completion", positionParams(uri, input))),
    );

  const resolveCompletion: LspManager["Service"]["resolveCompletion"] = (input) =>
    clientRequest(input, async ({ client }) => {
      let rawItem: object;
      try {
        rawItem = JSON.parse(input.resolveData) as object;
      } catch {
        // Malformed payloads are a client bug; return an empty item rather
        // than crashing the accept flow.
        return { label: "" };
      }
      return mapCompletionItem(await client.request("completionItem/resolve", rawItem));
    });

  const signatureHelp: LspManager["Service"]["signatureHelp"] = (input) =>
    clientRequest(input, async ({ client, uri }) =>
      mapSignatureHelp(
        await client.request("textDocument/signatureHelp", positionParams(uri, input)),
      ),
    );

  const hover: LspManager["Service"]["hover"] = (input) =>
    clientRequest(input, async ({ client, uri }) =>
      mapHover(await client.request("textDocument/hover", positionParams(uri, input))),
    );

  const definition: LspManager["Service"]["definition"] = (input) =>
    clientRequest(input, async ({ client, uri, workspaceRoot }) => ({
      locations: mapLocations(
        workspaceRoot,
        await client.request("textDocument/definition", positionParams(uri, input)),
      ),
    }));

  const references: LspManager["Service"]["references"] = (input) =>
    clientRequest(input, async ({ client, uri, workspaceRoot }) => ({
      locations: mapLocations(
        workspaceRoot,
        await client.request("textDocument/references", {
          ...positionParams(uri, input),
          context: { includeDeclaration: false },
        }),
      ),
    }));

  const rename: LspManager["Service"]["rename"] = (input) =>
    clientRequest(input, async ({ client, uri, workspaceRoot }) => {
      const edit = await client.request<Protocol.WorkspaceEdit | null>("textDocument/rename", {
        textDocument: { uri },
        position: input.position,
        newName: input.newName,
      });
      const action = mapCodeAction(workspaceRoot, {
        title: "Rename",
        ...(edit === null ? {} : { edit }),
      });
      if (action.disabledReason !== undefined) throw new Error(action.disabledReason);
      return { files: action.files };
    });

  const codeActions: LspManager["Service"]["codeActions"] = (input) =>
    clientRequest(input, async ({ client, uri, workspaceRoot }) => {
      if (!client.serverCapabilities.codeActionProvider) return { actions: [] };
      const result = await client.request<Array<Protocol.CodeAction | Protocol.Command> | null>(
        "textDocument/codeAction",
        {
          textDocument: { uri },
          range: input.range,
          context: {
            diagnostics: (rawDiagnostics.get(uri) ?? []).filter(
              (d) =>
                d.range.start.line <= input.range.end.line &&
                d.range.end.line >= input.range.start.line,
            ),
          },
        },
      );
      return { actions: (result ?? []).map((action) => mapCodeAction(workspaceRoot, action)) };
    });

  const resolveCodeAction: LspManager["Service"]["resolveCodeAction"] = (input) =>
    clientRequest(input, async ({ client, workspaceRoot }) => {
      const raw: Protocol.CodeAction = JSON.parse(input.resolveData);
      if (raw === null || typeof raw !== "object" || typeof raw.title !== "string")
        throw new Error("Invalid code action");
      const capability = client.serverCapabilities.codeActionProvider;
      const action =
        typeof capability === "object" && capability.resolveProvider && raw.data !== undefined
          ? await client.request<Protocol.CodeAction>("codeAction/resolve", raw)
          : raw;
      return mapCodeAction(workspaceRoot, action);
    });

  const semanticTokens: LspManager["Service"]["semanticTokens"] = (input) =>
    clientRequest(input, async ({ client, uri }) => {
      const capability = client.serverCapabilities.semanticTokensProvider;
      if (!capability) return { tokens: [] };
      const result = capability.full
        ? await client.request<Protocol.SemanticTokens | null>("textDocument/semanticTokens/full", {
            textDocument: { uri },
          })
        : capability.range
          ? await client.request<Protocol.SemanticTokens | null>(
              "textDocument/semanticTokens/range",
              { textDocument: { uri }, range: input.range },
            )
          : null;
      return { tokens: mapSemanticTokens(result, capability.legend) };
    });

  const format: LspManager["Service"]["format"] = (input) =>
    clientRequest(input, async ({ client, uri }) => ({
      edits: mapTextEdits(
        await client.request("textDocument/formatting", {
          textDocument: { uri },
          options: {
            tabSize: input.tabSize ?? 2,
            insertSpaces: input.insertSpaces ?? true,
          },
        }),
      ),
    }));

  const serverStatus: LspManager["Service"]["serverStatus"] = Effect.fn("LspManager.serverStatus")(
    function* (input) {
      const workspaceRoot = yield* normalizeRoot(input.cwd);
      const registry = yield* currentRegistry;
      const statuses = [];
      for (const config of registry.servers) {
        const managed = servers.get(serverKey(workspaceRoot, config.serverId));
        if (managed !== undefined) {
          statuses.push({
            serverId: config.serverId,
            displayName: config.displayName,
            state: managed.state,
          });
        }
      }
      // supportedExtensions reflects the full configured registry (not just
      // started servers) so clients can gate doc-sync before any lazy spawn.
      return { servers: statuses, supportedExtensions: registry.supportedExtensions };
    },
  );

  const subscribeDiagnostics: LspManager["Service"]["subscribeDiagnostics"] = (input) =>
    Stream.unwrap(
      Effect.gen(function* () {
        const workspaceRoot = yield* normalizeRoot(input.cwd);
        const subscription = yield* PubSub.subscribe(diagnosticsPubSub);
        return Stream.fromSubscription(subscription).pipe(
          Stream.filter((change) => change.cwd === workspaceRoot),
          Stream.map((change) => change.event),
        );
      }),
    );

  const sameCustomServer = (a: CustomLanguageServer, b: CustomLanguageServer): boolean =>
    a.displayName === b.displayName &&
    a.command === b.command &&
    a.languageId === b.languageId &&
    a.args.length === b.args.length &&
    a.args.every((arg, index) => arg === b.args[index]) &&
    a.extensions.length === b.extensions.length &&
    a.extensions.every((extension, index) => extension === b.extensions[index]);

  // Settings watcher: refresh the custom-server ref and dispose running
  // servers whose definition changed or disappeared, so the next request
  // respawns them with the new config. Unchanged servers keep running.
  yield* serverSettings.streamChanges.pipe(
    Stream.runForEach((next) =>
      Effect.gen(function* () {
        const previous = yield* Ref.get(customServersRef);
        yield* Ref.set(customServersRef, next.languageServers);
        const nextById = new Map(next.languageServers.map((server) => [server.serverId, server]));
        const changed = new Set<string>();
        for (const server of previous) {
          const replacement = nextById.get(server.serverId);
          if (replacement === undefined || !sameCustomServer(server, replacement)) {
            changed.add(server.serverId);
          }
        }
        if (changed.size === 0) return;
        for (const [key, managed] of servers.entries()) {
          if (!changed.has(managed.serverId)) continue;
          servers.delete(key);
          // Clear stale squiggles for documents the disposed server had open.
          for (const uri of managed.openDocuments.keys()) {
            rawDiagnostics.delete(uri);
            const location = uriToLocationPath(managed.workspaceRoot, uri);
            if (location.relativePath === undefined) continue;
            runtimePublish({
              cwd: managed.workspaceRoot,
              event: { relativePath: location.relativePath, diagnostics: [] },
            });
          }
          yield* Effect.promise(() =>
            managed.clientPromise.then(
              (client) => client.dispose(),
              () => undefined,
            ),
          );
        }
      }).pipe(
        Effect.catchCause((cause) =>
          Effect.logError("LspManager language-server settings reconcile failed", cause),
        ),
      ),
    ),
    Effect.forkIn(managerScope),
  );

  // Idle sweep: dispose servers whose documents are all closed and that have
  // been inactive past the shutdown window.
  yield* Effect.gen(function* () {
    while (true) {
      yield* Effect.sleep(IDLE_SWEEP_INTERVAL);
      const now = yield* Clock.currentTimeMillis;
      for (const [key, managed] of servers.entries()) {
        // Failed entries are the failure cache, not idle servers: they hold no
        // process to reclaim, and a server that never started has no open
        // documents, so sweeping them would erase the record the moment it was
        // written. That cost both the retry backoff (consecutiveCrashes resets)
        // and the reported status, which fell back to "Idle" — indistinguishable
        // from a server nothing had needed yet. getOrStartServer evicts these
        // once their retry is due.
        if (managed.state === "failed" || managed.state === "not_installed") continue;
        const idle =
          managed.openDocuments.size === 0 &&
          now - managed.lastActivityAt > Duration.toMillis(IDLE_SHUTDOWN_AFTER);
        if (!idle) continue;
        servers.delete(key);
        yield* Effect.promise(() =>
          managed.clientPromise.then(
            (client) => client.dispose(),
            () => undefined,
          ),
        );
        yield* releaseSaveWatcher(managed.workspaceRoot);
      }
    }
  }).pipe(Effect.forkIn(managerScope));

  // Dispose every running server when the service scope closes.
  yield* Effect.addFinalizer(() =>
    Effect.promise(async () => {
      for (const managed of servers.values()) {
        await managed.clientPromise.then(
          (client) => client.dispose(),
          () => undefined,
        );
      }
      servers.clear();
    }),
  );

  return LspManager.of({
    didOpen,
    didChange,
    didClose,
    completion,
    resolveCompletion,
    signatureHelp,
    hover,
    definition,
    references,
    rename,
    codeActions,
    resolveCodeAction,
    semanticTokens,
    format,
    serverStatus,
    subscribeDiagnostics,
  });
});

export const layer = Layer.effect(LspManager, make);
