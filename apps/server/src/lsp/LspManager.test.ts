import * as NodeServices from "@effect/platform-node/NodeServices";
import { it, describe, expect } from "@effect/vitest";
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as Fiber from "effect/Fiber";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Path from "effect/Path";
import * as Stream from "effect/Stream";
import * as TestClock from "effect/testing/TestClock";

import * as LspManager from "./LspManager.ts";
import { languageBindingForPath } from "./LanguageServers.ts";
import { layerTest as serverSettingsLayerTest } from "../serverSettings.ts";
import * as WorkspacePaths from "../workspace/WorkspacePaths.ts";

const TestLayer = LspManager.layer.pipe(
  Layer.provide(WorkspacePaths.layer),
  Layer.provide(serverSettingsLayerTest()),
  Layer.provideMerge(NodeServices.layer),
);

const makeTempDir = Effect.gen(function* () {
  const fileSystem = yield* FileSystem.FileSystem;
  return yield* fileSystem.makeTempDirectoryScoped({ prefix: "t3code-lsp-" });
});

describe("languageBindingForPath", () => {
  it("binds TypeScript/JavaScript extensions to the bundled server", () => {
    expect(languageBindingForPath("src/index.ts")?.languageId).toBe("typescript");
    expect(languageBindingForPath("src/App.tsx")?.languageId).toBe("typescriptreact");
    expect(languageBindingForPath("src/legacy.cjs")?.languageId).toBe("javascript");
  });

  it("returns null for unsupported files", () => {
    expect(languageBindingForPath("README.md")).toBe(null);
    expect(languageBindingForPath("Makefile")).toBe(null);
  });
});

it.layer(TestLayer, { excludeTestServices: true })("LspManagerLive", (it) => {
  describe("typescript smoke test", () => {
    it.effect(
      "opens a document, receives diagnostics, and answers hover",
      () =>
        Effect.gen(function* () {
          const lspManager = yield* LspManager.LspManager;
          const fileSystem = yield* FileSystem.FileSystem;
          const path = yield* Path.Path;
          const cwd = yield* makeTempDir;

          const contents = 'const broken: number = "oops";\nexport const fine = 1;\n';
          yield* fileSystem
            .writeFileString(path.join(cwd, "index.ts"), contents)
            .pipe(Effect.orDie);

          const diagnosticsFiber = yield* lspManager.subscribeDiagnostics({ cwd }).pipe(
            Stream.filter(
              (event) => event.relativePath === "index.ts" && event.diagnostics.length > 0,
            ),
            Stream.take(1),
            Stream.runCollect,
            Effect.forkChild,
          );

          yield* lspManager.didOpen({ cwd, relativePath: "index.ts", contents });

          const events = yield* Fiber.join(diagnosticsFiber).pipe(Effect.timeout("60 seconds"));
          expect(events).toHaveLength(1);
          const diagnostic = events[0]!.diagnostics[0]!;
          expect(diagnostic.severity).toBe(1);
          expect(diagnostic.message.toLowerCase()).toContain("not assignable");

          const hover = yield* lspManager.hover({
            cwd,
            relativePath: "index.ts",
            position: { line: 1, character: 13 },
          });
          expect(hover).not.toBe(null);
          expect(hover!.contents).toContain("fine");

          yield* lspManager.didClose({ cwd, relativePath: "index.ts" });
        }),
      { timeout: 90_000 },
    );

    it.effect(
      "returns member completions with resolvable auto-import data",
      () =>
        Effect.gen(function* () {
          const lspManager = yield* LspManager.LspManager;
          const fileSystem = yield* FileSystem.FileSystem;
          const path = yield* Path.Path;
          const cwd = yield* makeTempDir;

          const contents = 'const s = "abc";\ns.\n';
          yield* fileSystem.writeFileString(path.join(cwd, "main.ts"), contents).pipe(Effect.orDie);
          yield* lspManager.didOpen({ cwd, relativePath: "main.ts", contents });

          // Member completion after "s." — string prototype methods.
          const members = yield* lspManager.completion({
            cwd,
            relativePath: "main.ts",
            position: { line: 1, character: 2 },
          });
          const labels = members.items.map((item) => item.label);
          expect(labels).toContain("charAt");
          expect(labels).toContain("toUpperCase");

          // Every item carries an opaque resolve payload the client can echo
          // back; resolving must round-trip without error.
          const charAt = members.items.find((item) => item.label === "charAt")!;
          expect(charAt.resolveData).toBeDefined();
          const resolved = yield* lspManager.resolveCompletion({
            cwd,
            relativePath: "main.ts",
            resolveData: charAt.resolveData!,
          });
          expect(resolved.label).toBe("charAt");
          expect(resolved.documentation ?? resolved.detail ?? "").not.toBe("");

          // Signature help inside a call expression.
          const withCall = 'const s = "abc";\ns.charAt(\n';
          yield* lspManager.didChange({
            cwd,
            relativePath: "main.ts",
            contents: withCall,
            version: 1,
          });
          const signature = yield* lspManager.signatureHelp({
            cwd,
            relativePath: "main.ts",
            position: { line: 1, character: 10 },
          });
          expect(signature).not.toBe(null);
          expect(signature!.signatures[0]!.label).toContain("charAt");

          yield* lspManager.didClose({ cwd, relativePath: "main.ts" });
        }),
      { timeout: 90_000 },
    );

    it.effect("fails with unsupported_language for unknown extensions", () =>
      Effect.gen(function* () {
        const lspManager = yield* LspManager.LspManager;
        const cwd = yield* makeTempDir;

        const error = yield* lspManager
          .hover({ cwd, relativePath: "notes.txt", position: { line: 0, character: 0 } })
          .pipe(Effect.flip);

        expect(error.failure).toBe("unsupported_language");
      }),
    );
  });
});

// Custom servers from the `languageServers` setting: extensions are merged
// into the effective registry and reported through serverStatus, and files
// route to the configured command. The command is intentionally nonexistent
// so the not_installed path is exercised deterministically on any machine.
const CustomServerTestLayer = LspManager.layer.pipe(
  Layer.provide(WorkspacePaths.layer),
  Layer.provide(
    serverSettingsLayerTest({
      languageServers: [
        {
          serverId: "fake-lang",
          displayName: "fake-language-server",
          command: "t3code-nonexistent-language-server",
          args: ["--stdio"],
          extensions: [".fake"],
          languageId: "fakelang",
        },
      ],
    }),
  ),
  Layer.provideMerge(NodeServices.layer),
);

it.layer(CustomServerTestLayer, { excludeTestServices: true })(
  "LspManagerLive custom servers",
  (it) => {
    it.effect("reports custom extensions alongside built-ins in serverStatus", () =>
      Effect.gen(function* () {
        const lspManager = yield* LspManager.LspManager;
        const cwd = yield* makeTempDir;

        const status = yield* lspManager.serverStatus({ cwd });
        expect(status.supportedExtensions).toContain(".fake");
        expect(status.supportedExtensions).toContain(".ts");
      }),
    );

    it.effect("routes matching files to the custom server and surfaces not_installed", () =>
      Effect.gen(function* () {
        const lspManager = yield* LspManager.LspManager;
        const cwd = yield* makeTempDir;

        const error = yield* lspManager
          .hover({ cwd, relativePath: "main.fake", position: { line: 0, character: 0 } })
          .pipe(Effect.flip);
        expect(error.failure).toBe("server_not_installed");
        expect(error.serverId).toBe("fake-lang");

        const status = yield* lspManager.serverStatus({ cwd });
        const fake = status.servers.find((server) => server.serverId === "fake-lang");
        expect(fake?.state).toBe("not_installed");
      }),
    );
  },
);

// A minimal LSP server that completes the initialize handshake, then kills
// itself on the first didChange — a deterministic post-init crash on any
// machine. Backoff timing is driven by the TestClock provided per test; the
// real time the child takes to spawn never touches the retry math.
const CRASHY_SERVER_SCRIPT = `
let buffer = Buffer.alloc(0);
const respond = (id, result) => {
  const body = JSON.stringify({ jsonrpc: "2.0", id, result });
  process.stdout.write("Content-Length: " + Buffer.byteLength(body) + "\\r\\n\\r\\n" + body);
};
process.stdin.on("data", (chunk) => {
  buffer = Buffer.concat([buffer, chunk]);
  for (;;) {
    const separator = buffer.indexOf("\\r\\n\\r\\n");
    if (separator === -1) return;
    const length = Number(/Content-Length: (\\d+)/.exec(buffer.slice(0, separator).toString())[1]);
    if (buffer.length < separator + 4 + length) return;
    const message = JSON.parse(buffer.slice(separator + 4, separator + 4 + length).toString());
    buffer = buffer.slice(separator + 4 + length);
    if (message.method === "initialize") respond(message.id, { capabilities: {} });
    else if (message.method === "shutdown") respond(message.id, null);
    else if (message.method === "exit") process.exit(0);
    else if (message.method === "textDocument/didChange") process.exit(1);
  }
});
setInterval(() => {}, 1000);
`.trim();

const CrashServerTestLayer = LspManager.layer.pipe(
  Layer.provide(WorkspacePaths.layer),
  Layer.provide(
    serverSettingsLayerTest({
      languageServers: [
        {
          serverId: "crashy",
          displayName: "crashy-language-server",
          command: process.execPath,
          args: ["-e", CRASHY_SERVER_SCRIPT],
          extensions: [".crashy"],
          languageId: "crashy",
        },
      ],
    }),
  ),
  Layer.provideMerge(NodeServices.layer),
);

// Real-time sleep for polling on child-process events; Effect.sleep would
// hang under the TestClock these tests drive.
const realSleep = (millis: number) =>
  // @effect-diagnostics-next-line globalTimers:off - polls real child-process exits under a frozen TestClock.
  Effect.promise(() => new Promise<void>((resolve) => setTimeout(resolve, millis)));

const awaitCrashyState = (
  lspManager: LspManager.LspManager["Service"],
  cwd: string,
  state: string,
) =>
  Effect.gen(function* () {
    for (let attempt = 0; attempt < 400; attempt++) {
      const status = yield* lspManager.serverStatus({ cwd });
      if (status.servers.find((server) => server.serverId === "crashy")?.state === state) return;
      yield* realSleep(25);
    }
    return yield* Effect.die(new Error(`crashy server never reached state '${state}'`));
  });

it.layer(CrashServerTestLayer, { excludeTestServices: true })(
  "LspManagerLive crash backoff",
  (it) => {
    it.effect(
      "backs off before respawning a crashed server instead of respawning per keystroke",
      () =>
        Effect.gen(function* () {
          const lspManager = yield* LspManager.LspManager;
          const cwd = yield* makeTempDir;
          const send = (version: number) =>
            lspManager.didChange({ cwd, relativePath: "main.crashy", contents: "x", version });

          // Spawns the server; the fake crashes on receiving the didChange.
          yield* send(0);
          yield* awaitCrashyState(lspManager, cwd, "failed");

          // Every keystroke while the failure is cached surfaces the crash
          // instead of respawning.
          const first = yield* send(1).pipe(Effect.flip);
          expect(first.failure).toBe("server_crashed");
          const second = yield* send(2).pipe(Effect.flip);
          expect(second.failure).toBe("server_crashed");

          // Still gated just before the first retry window (1 min) elapses.
          yield* TestClock.adjust(Duration.seconds(59));
          const gated = yield* send(3).pipe(Effect.flip);
          expect(gated.failure).toBe("server_crashed");

          // Past the window: respawns (and crashes on the didChange again).
          yield* TestClock.adjust(Duration.seconds(2));
          yield* send(4);
          yield* awaitCrashyState(lspManager, cwd, "failed");

          // Second consecutive crash: the retry window doubles to 2 minutes.
          const observed = yield* send(5).pipe(Effect.flip);
          expect(observed.failure).toBe("server_crashed");
          yield* TestClock.adjust(Duration.seconds(61));
          const stillGated = yield* send(6).pipe(Effect.flip);
          expect(stillGated.failure).toBe("server_crashed");
          yield* TestClock.adjust(Duration.seconds(60));
          yield* send(7);
          yield* awaitCrashyState(lspManager, cwd, "failed");
        }).pipe(Effect.provide(TestClock.layer())),
      { timeout: 60_000 },
    );

    it.effect(
      "resets the crash streak after a healthy stretch of uptime",
      () =>
        Effect.gen(function* () {
          const lspManager = yield* LspManager.LspManager;
          const cwd = yield* makeTempDir;
          const send = (version: number) =>
            lspManager.didChange({ cwd, relativePath: "main.crashy", contents: "x", version });

          // Two quick crashes build a streak of 2 (retry window: 2 minutes).
          yield* send(0);
          yield* awaitCrashyState(lspManager, cwd, "failed");
          expect((yield* send(1).pipe(Effect.flip)).failure).toBe("server_crashed");
          yield* TestClock.adjust(Duration.seconds(61));
          yield* send(2);
          yield* awaitCrashyState(lspManager, cwd, "failed");
          expect((yield* send(3).pipe(Effect.flip)).failure).toBe("server_crashed");

          // Respawn, keep it alive through a healthy stretch, then crash it.
          yield* TestClock.adjust(Duration.seconds(121));
          yield* lspManager.didOpen({ cwd, relativePath: "main.crashy", contents: "x" });
          yield* awaitCrashyState(lspManager, cwd, "running");
          yield* TestClock.adjust(Duration.minutes(6));
          yield* send(4);
          yield* awaitCrashyState(lspManager, cwd, "failed");

          // The healthy run reset the streak: the retry window is back to
          // 1 minute — a streak of 3 would have required 4.
          expect((yield* send(5).pipe(Effect.flip)).failure).toBe("server_crashed");
          yield* TestClock.adjust(Duration.seconds(59));
          expect((yield* send(6).pipe(Effect.flip)).failure).toBe("server_crashed");
          yield* TestClock.adjust(Duration.seconds(2));
          yield* send(7);
          yield* awaitCrashyState(lspManager, cwd, "failed");
        }).pipe(Effect.provide(TestClock.layer())),
      { timeout: 60_000 },
    );
  },
);
