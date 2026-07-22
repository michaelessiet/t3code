import * as NodeServices from "@effect/platform-node/NodeServices";
import { it, describe, expect } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as Fiber from "effect/Fiber";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Path from "effect/Path";
import * as Stream from "effect/Stream";

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
