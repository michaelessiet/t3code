// @effect-diagnostics nodeBuiltinImport:off - builds a real gzipped npm tarball and hashes it.
import * as NodeChildProcess from "node:child_process";
import * as NodeCrypto from "node:crypto";

import {
  HostProcessArchitecture,
  HostProcessEnvironment,
  HostProcessPlatform,
} from "@t3tools/shared/hostProcess";
import * as NodeServices from "@effect/platform-node/NodeServices";
import { describe, expect, it } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import { HttpClient, HttpClientResponse } from "effect/unstable/http";

import {
  ClaudeBinaryInstallError,
  claudeNpmTarballUrl,
  makeClaudeManagedBinary,
  resolveClaudePlatformKey,
  type ClaudeSdkPackageInfo,
} from "./ClaudeManagedBinary.ts";

const CLI_VERSION = "9.9.9";
const NPM_VERSION = "0.0.1";
// A real executable so the post-install `--version` validation can run it.
const FAKE_BINARY_CONTENTS = "#!/bin/sh\nexit 0\n";

const sha256Hex = (contents: string): string =>
  NodeCrypto.createHash("sha256").update(contents).digest("hex");

// The installer extracts with the system tar and runs the extracted script, so
// the fake SDK identity must target the real host platform.
const hostPlatformKey = Effect.gen(function* () {
  return resolveClaudePlatformKey(yield* HostProcessPlatform, yield* HostProcessArchitecture);
});

const makeSdkInfo = (platformKey: string, checksum: string): ClaudeSdkPackageInfo => ({
  npmVersion: NPM_VERSION,
  cliVersion: CLI_VERSION,
  platforms: {
    [platformKey]: {
      binary: "claude",
      checksum,
      size: FAKE_BINARY_CONTENTS.length,
    },
  },
});

const makeHttpClientLayer = (bytes: Uint8Array) =>
  Layer.succeed(
    HttpClient.HttpClient,
    HttpClient.make((request) =>
      Effect.succeed(
        HttpClientResponse.fromWeb(request, new Response(bytes.buffer as ArrayBuffer)),
      ),
    ),
  );

/** Build a real `package/claude` npm-style tarball with the system tar. */
const makeClaudeTarball = Effect.fn("test.makeClaudeTarball")(function* (binaryContents: string) {
  const fileSystem = yield* FileSystem.FileSystem;
  const workDir = yield* fileSystem.makeTempDirectoryScoped({ prefix: "t3-claude-tarball-" });
  yield* fileSystem.makeDirectory(`${workDir}/package`, { recursive: true });
  yield* fileSystem.writeFileString(`${workDir}/package/claude`, binaryContents);
  yield* fileSystem.chmod(`${workDir}/package/claude`, 0o755);
  const tarballPath = `${workDir}/claude.tgz`;
  yield* Effect.sync(() =>
    NodeChildProcess.execFileSync("tar", ["-czf", tarballPath, "-C", workDir, "package/claude"]),
  );
  return yield* fileSystem.readFile(tarballPath);
});

const emptyProcessEnvironmentLayer = Layer.succeed(HostProcessEnvironment, { PATH: "" });

describe("ClaudeManagedBinary", () => {
  it.effect(
    "resolves explicit paths, env overrides, and reports missing with the pinned size",
    () =>
      Effect.gen(function* () {
        const fileSystem = yield* FileSystem.FileSystem;
        const baseDir = yield* fileSystem.makeTempDirectoryScoped({ prefix: "t3-claude-test-" });
        const overridePath = `${baseDir}/override-claude`;
        yield* fileSystem.writeFileString(overridePath, FAKE_BINARY_CONTENTS);
        yield* fileSystem.chmod(overridePath, 0o755);
        const manager = yield* makeClaudeManagedBinary({
          baseDir,
          sdkInfo: makeSdkInfo(yield* hostPlatformKey, sha256Hex(FAKE_BINARY_CONTENTS)),
        });

        expect(yield* manager.status("/explicit/claude", { PATH: "" })).toEqual({
          status: "available",
          executablePath: "/explicit/claude",
          source: "explicit",
          version: CLI_VERSION,
        });
        expect(
          yield* manager.status("claude", { PATH: "", T3CODE_CLAUDE_BINARY_PATH: overridePath }),
        ).toEqual({
          status: "available",
          executablePath: overridePath,
          source: "override",
          version: CLI_VERSION,
        });
        expect(
          yield* manager.status("claude", {
            PATH: "",
            T3CODE_CLAUDE_BINARY_PATH: `${baseDir}/does-not-exist`,
          }),
        ).toEqual({
          status: "missing",
          version: CLI_VERSION,
          binarySizeBytes: FAKE_BINARY_CONTENTS.length,
        });
        expect(yield* manager.status("claude", { PATH: "" })).toEqual({
          status: "missing",
          version: CLI_VERSION,
          binarySizeBytes: FAKE_BINARY_CONTENTS.length,
        });
      }).pipe(
        Effect.scoped,
        Effect.provide(Layer.mergeAll(NodeServices.layer, makeHttpClientLayer(new Uint8Array()))),
      ),
  );

  it.effect("resolves executables from PATH before reporting missing", () =>
    Effect.gen(function* () {
      const fileSystem = yield* FileSystem.FileSystem;
      const baseDir = yield* fileSystem.makeTempDirectoryScoped({ prefix: "t3-claude-test-" });
      const binDir = `${baseDir}/bin`;
      const executablePath = `${binDir}/claude`;
      yield* fileSystem.makeDirectory(binDir);
      yield* fileSystem.writeFileString(executablePath, FAKE_BINARY_CONTENTS);
      yield* fileSystem.chmod(executablePath, 0o755);
      const manager = yield* makeClaudeManagedBinary({
        baseDir,
        sdkInfo: makeSdkInfo(yield* hostPlatformKey, sha256Hex(FAKE_BINARY_CONTENTS)),
      });

      expect(yield* manager.status("claude", { PATH: binDir })).toEqual({
        status: "available",
        executablePath,
        source: "path",
        version: CLI_VERSION,
      });
    }).pipe(
      Effect.scoped,
      Effect.provide(Layer.mergeAll(NodeServices.layer, makeHttpClientLayer(new Uint8Array()))),
    ),
  );

  it.effect("reports unsupported platforms with the pinned version", () =>
    Effect.gen(function* () {
      const fileSystem = yield* FileSystem.FileSystem;
      const baseDir = yield* fileSystem.makeTempDirectoryScoped({ prefix: "t3-claude-test-" });
      const manager = yield* makeClaudeManagedBinary({
        baseDir,
        sdkInfo: {
          npmVersion: NPM_VERSION,
          cliVersion: CLI_VERSION,
          platforms: {},
        },
      });

      expect(yield* manager.status("claude", { PATH: "" })).toEqual({
        status: "unsupported",
        platform: yield* HostProcessPlatform,
        arch: yield* HostProcessArchitecture,
        version: CLI_VERSION,
      });
      const error = yield* manager.install.pipe(Effect.flip);
      expect(error).toBeInstanceOf(ClaudeBinaryInstallError);
      expect(error.reason).toBe("unsupported_platform");
    }).pipe(
      Effect.scoped,
      Effect.provide(
        Layer.mergeAll(
          NodeServices.layer,
          makeHttpClientLayer(new Uint8Array()),
          emptyProcessEnvironmentLayer,
        ),
      ),
    ),
  );

  it.effect("downloads, extracts, verifies, validates, and activates the managed binary", () =>
    Effect.gen(function* () {
      const fileSystem = yield* FileSystem.FileSystem;
      const baseDir = yield* fileSystem.makeTempDirectoryScoped({ prefix: "t3-claude-test-" });
      const tarball = yield* makeClaudeTarball(FAKE_BINARY_CONTENTS);
      // A stale sibling version that the post-activate GC must remove.
      const staleVersionDir = `${baseDir}/tools/claude/0.0.0`;
      yield* fileSystem.makeDirectory(staleVersionDir, { recursive: true });

      const manager = yield* makeClaudeManagedBinary({
        baseDir,
        sdkInfo: makeSdkInfo(yield* hostPlatformKey, sha256Hex(FAKE_BINARY_CONTENTS)),
      }).pipe(Effect.provide(makeHttpClientLayer(tarball)));

      const progress: Array<string> = [];
      const installed = yield* manager.installWithProgress((event) =>
        Effect.sync(() => {
          if (progress.at(-1) !== event.stage) {
            progress.push(event.stage);
          }
        }),
      );
      const managedPath = `${baseDir}/tools/claude/${NPM_VERSION}/${yield* hostPlatformKey}/claude`;
      expect(installed).toEqual({
        status: "available",
        executablePath: managedPath,
        source: "managed",
        version: CLI_VERSION,
      });
      expect(new TextDecoder().decode(yield* fileSystem.readFile(managedPath))).toBe(
        FAKE_BINARY_CONTENTS,
      );
      expect(progress).toEqual([
        "checking",
        "waiting_for_lock",
        "downloading",
        "installing",
        "verifying",
        "validating",
        "activating",
      ]);
      expect(yield* fileSystem.exists(staleVersionDir)).toBe(false);
      expect(yield* manager.status("claude", { PATH: "" })).toEqual(installed);
      // A second install is a no-op that returns the existing binary.
      expect(yield* manager.install).toEqual(installed);
    }).pipe(
      Effect.scoped,
      Effect.provide(
        Layer.mergeAll(
          NodeServices.layer,
          makeHttpClientLayer(new Uint8Array()),
          emptyProcessEnvironmentLayer,
        ),
      ),
    ),
  );

  it.effect("rejects downloads whose checksum does not match the SDK's pinned build", () =>
    Effect.gen(function* () {
      const fileSystem = yield* FileSystem.FileSystem;
      const baseDir = yield* fileSystem.makeTempDirectoryScoped({ prefix: "t3-claude-test-" });
      const tarball = yield* makeClaudeTarball(FAKE_BINARY_CONTENTS);
      const manager = yield* makeClaudeManagedBinary({
        baseDir,
        sdkInfo: makeSdkInfo(yield* hostPlatformKey, sha256Hex("a different pinned binary")),
      }).pipe(Effect.provide(makeHttpClientLayer(tarball)));

      const error = yield* manager.install.pipe(Effect.flip);
      expect(error).toBeInstanceOf(ClaudeBinaryInstallError);
      expect(error.reason).toBe("invalid_checksum");
      expect(
        yield* fileSystem.exists(
          `${baseDir}/tools/claude/${NPM_VERSION}/${yield* hostPlatformKey}/claude`,
        ),
      ).toBe(false);
    }).pipe(
      Effect.scoped,
      Effect.provide(
        Layer.mergeAll(
          NodeServices.layer,
          makeHttpClientLayer(new Uint8Array()),
          emptyProcessEnvironmentLayer,
        ),
      ),
    ),
  );

  it("builds npm registry tarball URLs from the platform key and npm version", () => {
    expect(claudeNpmTarballUrl("https://registry.npmjs.org", "darwin-arm64", "0.3.170")).toBe(
      "https://registry.npmjs.org/@anthropic-ai/claude-agent-sdk-darwin-arm64/-/claude-agent-sdk-darwin-arm64-0.3.170.tgz",
    );
  });
});
