import * as NodeModule from "node:module";

import * as NodeServices from "@effect/platform-node/NodeServices";
import { assert, it } from "@effect/vitest";
import * as ConfigProvider from "effect/ConfigProvider";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Path from "effect/Path";
import * as Option from "effect/Option";
import * as Sink from "effect/Sink";
import * as Stream from "effect/Stream";
import { ChildProcessSpawner } from "effect/unstable/process";

import {
  BUILDER_HOOKS_FILENAME,
  BUILDER_HOOKS_SOURCE,
  BuildCommandFailedError,
  ClaudeSdkPlatformPackagesNotFoundError,
  isClaudeSdkPlatformPackagePath,
  pruneClaudeSdkPlatformPackages,
  createStageWorkspaceConfig,
  createStagePatchedDependencies,
  createBuildConfig,
  DESKTOP_ASAR_UNPACK,
  InvalidMacPasskeyRpDomainError,
  InvalidMacPasskeyPublishableKeyError,
  InvalidMockUpdateServerPortError,
  isMacPasskeySigningConfigurationError,
  LinuxIconResizeError,
  MacPasskeySigningConfigurationResolutionError,
  MissingMacPasskeyProvisioningProfileError,
  renderMacPasskeyEntitlements,
  resolveClerkPasskeyNativeArtifacts,
  resolveMacPasskeySigningConfiguration,
  resolveDesktopRuntimeDependencies,
  resolveFffNativeDependencies,
  resolveBuildOptions,
  resolveDesktopBuildIconAssets,
  resolveDesktopProductName,
  resolveDesktopUpdateChannel,
  resolveDesktopWebAssetBrand,
  resolveGitHubPublishConfig,
  resolveMockUpdateServerPort,
  resolveMockUpdateServerUrl,
  resolvePackageManagerUserAgent,
  stageLinuxIconSize,
  STAGE_INSTALL_ARGS,
  stripStagedSourcemaps,
} from "./build-desktop-artifact.ts";
import { BRAND_ASSET_PATHS } from "./lib/brand-assets.ts";
import { HostProcessArchitecture, HostProcessPlatform } from "@t3tools/shared/hostProcess";

function mockProcess(exitCode: number) {
  return ChildProcessSpawner.makeHandle({
    pid: ChildProcessSpawner.ProcessId(1),
    exitCode: Effect.succeed(ChildProcessSpawner.ExitCode(exitCode)),
    isRunning: Effect.succeed(false),
    kill: () => Effect.void,
    unref: Effect.succeed(Effect.void),
    stdin: Sink.drain,
    stdout: Stream.empty,
    stderr: Stream.empty,
    all: Stream.empty,
    getInputFd: () => Sink.drain,
    getOutputFd: () => Stream.empty,
  });
}

function iconResizeSpawnerLayer(
  commands: Array<{ readonly command: string; readonly args: ReadonlyArray<string> }>,
  exitCodes: ReadonlyArray<number>,
) {
  let commandIndex = 0;
  return Layer.succeed(
    ChildProcessSpawner.ChildProcessSpawner,
    ChildProcessSpawner.make((command) => {
      const childProcess = command as unknown as {
        readonly command: string;
        readonly args: ReadonlyArray<string>;
      };
      commands.push({
        command: childProcess.command,
        args: childProcess.args,
      });
      return Effect.succeed(mockProcess(exitCodes[commandIndex++] ?? 0));
    }),
  );
}

it.layer(NodeServices.layer)("build-desktop-artifact", (it) => {
  it("resolves the dedicated nightly updater channel from nightly versions", () => {
    assert.equal(resolveDesktopUpdateChannel("0.0.17-nightly.20260413.42"), "nightly");
    assert.equal(resolveDesktopUpdateChannel("0.0.17"), "latest");
  });

  it("switches desktop packaging product names to nightly for nightly builds", () => {
    assert.equal(resolveDesktopProductName("0.0.17"), "T3 Code (Alpha)");
    assert.equal(resolveDesktopProductName("0.0.17-nightly.20260413.42"), "T3 Code (Nightly)");
  });

  it("switches desktop packaging icons to the nightly artwork for nightly versions", () => {
    assert.deepStrictEqual(resolveDesktopBuildIconAssets("0.0.17"), {
      macIconPng: BRAND_ASSET_PATHS.productionMacIconPng,
      linuxIconPng: BRAND_ASSET_PATHS.productionLinuxIconPng,
      windowsIconIco: BRAND_ASSET_PATHS.productionWindowsIconIco,
    });

    assert.deepStrictEqual(resolveDesktopBuildIconAssets("0.0.17-nightly.20260413.42"), {
      macIconPng: BRAND_ASSET_PATHS.nightlyMacIconPng,
      linuxIconPng: BRAND_ASSET_PATHS.nightlyLinuxIconPng,
      windowsIconIco: BRAND_ASSET_PATHS.nightlyWindowsIconIco,
    });
  });

  it("switches the bundled splash and favicon branding for nightly versions", () => {
    assert.equal(resolveDesktopWebAssetBrand("0.0.17"), "production");
    assert.equal(resolveDesktopWebAssetBrand("0.0.17-nightly.20260413.42"), "nightly");
  });

  it.effect("resolves GitHub desktop publish config from Effect config", () =>
    Effect.gen(function* () {
      const latestConfig = yield* resolveGitHubPublishConfig("latest").pipe(
        Effect.provide(
          ConfigProvider.layer(
            ConfigProvider.fromEnv({
              env: {
                T3CODE_DESKTOP_UPDATE_REPOSITORY: "pingdotgg/t3code",
              },
            }),
          ),
        ),
      );
      const nightlyConfig = yield* resolveGitHubPublishConfig("nightly").pipe(
        Effect.provide(
          ConfigProvider.layer(
            ConfigProvider.fromEnv({
              env: {
                GITHUB_REPOSITORY: "pingdotgg/t3code",
              },
            }),
          ),
        ),
      );

      assert.deepStrictEqual(latestConfig, {
        provider: "github",
        owner: "pingdotgg",
        repo: "t3code",
        releaseType: "release",
      });
      assert.deepStrictEqual(nightlyConfig, {
        provider: "github",
        owner: "pingdotgg",
        repo: "t3code",
        releaseType: "prerelease",
        channel: "nightly",
      });
    }),
  );

  it("omits bundled workspace packages from staged desktop dependencies", () => {
    assert.deepStrictEqual(
      resolveDesktopRuntimeDependencies(
        {
          "@effect/platform-node": "catalog:",
          "@t3tools/contracts": "workspace:*",
          "@t3tools/shared": "workspace:*",
          "@t3tools/ssh": "workspace:*",
          "@t3tools/tailscale": "workspace:*",
          effect: "catalog:",
          electron: "41.5.0",
        },
        {
          "@effect/platform-node": "4.0.0-beta.59",
          effect: "4.0.0-beta.59",
        },
      ),
      {
        "@effect/platform-node": "4.0.0-beta.59",
        effect: "4.0.0-beta.59",
      },
    );
  });

  it("carries only staged dependency patch metadata into staged desktop installs", () => {
    assert.deepStrictEqual(
      createStagePatchedDependencies(
        {
          "@expo/metro-config@56.0.13": "patches/@expo%2Fmetro-config@56.0.13.patch",
          "@ff-labs/fff-node@0.9.4": "patches/@ff-labs__fff-node@0.9.4.patch",
          "@pierre/diffs@1.1.20": "patches/@pierre%2Fdiffs@1.1.20.patch",
          "alchemy@2.0.0-beta.49": "patches/alchemy@2.0.0-beta.49.patch",
          "effect@4.0.0-beta.73": "patches/effect@4.0.0-beta.73.patch",
        },
        {
          "@ff-labs/fff-node": "0.9.4",
          "@pierre/diffs": "1.1.20",
          effect: "4.0.0-beta.73",
        },
      ),
      {
        "@ff-labs/fff-node@0.9.4": "patches/@ff-labs__fff-node@0.9.4.patch",
        "@pierre/diffs@1.1.20": "patches/@pierre%2Fdiffs@1.1.20.patch",
        "effect@4.0.0-beta.73": "patches/effect@4.0.0-beta.73.patch",
      },
    );

    assert.deepStrictEqual(
      createStagePatchedDependencies(
        {
          "@expo/metro-config@56.0.13": "patches/@expo%2Fmetro-config@56.0.13.patch",
        },
        { effect: "4.0.0-beta.73" },
      ),
      {},
    );
  });

  it("installs optional native dependencies for the target desktop architecture", () => {
    assert.deepStrictEqual(STAGE_INSTALL_ARGS, ["install", "--prod"]);
    assert.deepStrictEqual(createStageWorkspaceConfig({ platform: "mac", arch: "x64" }), {
      supportedArchitectures: {
        os: ["darwin"],
        cpu: ["x64"],
      },
    });
    assert.deepStrictEqual(createStageWorkspaceConfig({ platform: "linux", arch: "x64" }), {
      supportedArchitectures: {
        os: ["linux"],
        cpu: ["x64"],
        libc: ["glibc"],
      },
    });
    // Windows artifacts also bundle the same-architecture WSL (Linux, glibc) backend, so the
    // staged install must fetch its native optional deps (e.g. ffi-rs) too.
    assert.deepStrictEqual(createStageWorkspaceConfig({ platform: "win", arch: "x64" }), {
      supportedArchitectures: {
        os: ["win32", "linux"],
        cpu: ["x64"],
        libc: ["glibc"],
      },
    });
    assert.deepStrictEqual(createStageWorkspaceConfig({ platform: "win", arch: "arm64" }), {
      supportedArchitectures: {
        os: ["win32", "linux"],
        cpu: ["arm64"],
        libc: ["glibc"],
      },
    });
    assert.deepStrictEqual(createStageWorkspaceConfig({ platform: "mac", arch: "universal" }), {
      supportedArchitectures: {
        os: ["darwin"],
        cpu: ["arm64", "x64"],
      },
    });
  });

  it("stages pnpm 11 allowBuilds and patchedDependencies in the workspace yaml", () => {
    assert.deepStrictEqual(
      createStageWorkspaceConfig({
        platform: "linux",
        arch: "x64",
        allowBuilds: {
          electron: true,
          "node-pty": true,
          "browser-tabs-lock": false,
        },
        patchedDependencies: {
          "effect@4.0.0-beta.73": "patches/effect@4.0.0-beta.73.patch",
        },
        overrides: {
          effect: "4.0.0-beta.73",
        },
      }),
      {
        supportedArchitectures: {
          os: ["linux"],
          cpu: ["x64"],
          libc: ["glibc"],
        },
        allowBuilds: {
          electron: true,
          "node-pty": true,
          "browser-tabs-lock": false,
        },
        patchedDependencies: {
          "effect@4.0.0-beta.73": "patches/effect@4.0.0-beta.73.patch",
        },
        overrides: {
          effect: "4.0.0-beta.73",
        },
      },
    );

    // Empty maps must not be written — pnpm would still require reviewed
    // packages if allowBuilds is present but incomplete, and omitting empty
    // patchedDependencies keeps the stage yaml minimal.
    assert.deepStrictEqual(
      createStageWorkspaceConfig({
        platform: "mac",
        arch: "arm64",
        allowBuilds: {},
        patchedDependencies: {},
        overrides: {},
      }),
      {
        supportedArchitectures: {
          os: ["darwin"],
          cpu: ["arm64"],
        },
      },
    );
  });

  it("unpacks the fff shared library for filesystem and FFI access", () => {
    assert.deepStrictEqual(DESKTOP_ASAR_UNPACK, ["node_modules/@ff-labs/fff-bin-*/**/*"]);
  });

  it.effect("preserves both Linux icon resize failures with structural context", () => {
    const commands: Array<{ readonly command: string; readonly args: ReadonlyArray<string> }> = [];

    return Effect.gen(function* () {
      const error = yield* stageLinuxIconSize("source.png", "target.png", 512, false).pipe(
        Effect.provide(iconResizeSpawnerLayer(commands, [1, 2])),
        Effect.flip,
      );

      assert.instanceOf(error, LinuxIconResizeError);
      assert.equal(error.operation, "resize");
      assert.equal(error.iconSize, 512);
      assert.equal(error.primaryTool, "magick");
      assert.equal(error.fallbackTool, "convert");
      assert.include(error.message, "512x512");
      assert.include(error.message, "`magick`");
      assert.include(error.message, "`convert`");
      assert.notInclude(error.message, "non-zero exit code");

      assert.instanceOf(error.cause, AggregateError);
      const aggregateCause = error.cause as AggregateError;
      assert.lengthOf(aggregateCause.errors, 2);
      assert.strictEqual(aggregateCause.cause, aggregateCause.errors[0]);
      assert.instanceOf(aggregateCause.errors[0], BuildCommandFailedError);
      assert.instanceOf(aggregateCause.errors[1], BuildCommandFailedError);
      const primaryError = aggregateCause.errors[0] as BuildCommandFailedError;
      const fallbackError = aggregateCause.errors[1] as BuildCommandFailedError;
      assert.equal(primaryError.command, "magick linux icon 512x512");
      assert.equal(primaryError.exitCode, 1);
      assert.include(primaryError.message, "magick linux icon");
      assert.equal(fallbackError.command, "convert linux icon 512x512");
      assert.equal(fallbackError.exitCode, 2);
      assert.include(fallbackError.message, "convert linux icon");
      assert.deepStrictEqual(
        commands.map(({ command }) => command),
        ["magick", "convert"],
      );
    });
  });

  it("derives macOS passkey signing configuration from the Clerk publishable key", () => {
    const configuration = resolveMacPasskeySigningConfiguration({
      T3CODE_APPLE_TEAM_ID: "abc1234567",
      T3CODE_MACOS_PROVISIONING_PROFILE: "/tmp/t3code.provisionprofile",
      T3CODE_CLERK_PUBLISHABLE_KEY: `pk_test_${btoa("example.clerk.accounts.dev$")}`,
    });

    assert.deepStrictEqual(configuration, {
      appId: "com.t3tools.t3code",
      teamId: "ABC1234567",
      rpDomains: ["example.clerk.accounts.dev"],
      provisioningProfilePath: "/tmp/t3code.provisionprofile",
    });
  });

  it("normalizes explicit macOS passkey RP domains and renders required entitlements", () => {
    const configuration = resolveMacPasskeySigningConfiguration({
      T3CODE_APPLE_TEAM_ID: "ABC1234567",
      T3CODE_MACOS_PROVISIONING_PROFILE: "/tmp/t3code.provisionprofile",
      T3CODE_CLERK_PASSKEY_RP_DOMAINS:
        " Clerk.Example.com,example.clerk.accounts.dev,clerk.example.com ",
    });
    const entitlements = renderMacPasskeyEntitlements(configuration);

    assert.deepStrictEqual(configuration.rpDomains, [
      "clerk.example.com",
      "example.clerk.accounts.dev",
    ]);
    assert.include(entitlements, "<string>ABC1234567.com.t3tools.t3code</string>");
    assert.include(entitlements, "<string>webcredentials:clerk.example.com</string>");
    assert.include(entitlements, "<string>webcredentials:example.clerk.accounts.dev</string>");
    assert.include(entitlements, "<key>com.apple.security.cs.allow-jit</key>");
  });

  it("rejects incomplete macOS passkey signing configuration", () => {
    const captureError = (env: Readonly<Record<string, string | undefined>>) => {
      try {
        resolveMacPasskeySigningConfiguration(env);
      } catch (error) {
        return error;
      }
      return assert.fail("Expected passkey signing configuration to fail.");
    };

    const missingProfileError = captureError({
      T3CODE_APPLE_TEAM_ID: "ABC1234567",
      T3CODE_CLERK_PASSKEY_RP_DOMAINS: "example.clerk.accounts.dev",
    });
    assert.instanceOf(missingProfileError, MissingMacPasskeyProvisioningProfileError);
    assert.equal(
      missingProfileError.message,
      "T3CODE_MACOS_PROVISIONING_PROFILE must point to an Associated Domains provisioning profile.",
    );

    const unsafeDomain =
      "https://domain-user:domain-secret@example.clerk.accounts.dev/path?token=query-secret";
    const invalidDomainError = captureError({
      T3CODE_APPLE_TEAM_ID: "ABC1234567",
      T3CODE_MACOS_PROVISIONING_PROFILE: "/tmp/t3code.provisionprofile",
      T3CODE_CLERK_PASSKEY_RP_DOMAINS: unsafeDomain,
    });
    assert.instanceOf(invalidDomainError, InvalidMacPasskeyRpDomainError);
    assert.equal(invalidDomainError.reason, "scheme-not-allowed");
    assert.equal(invalidDomainError.inputLength, unsafeDomain.length);
    assert.equal(invalidDomainError.message, "Invalid passkey RP domain (scheme-not-allowed).");
    assert.notProperty(invalidDomainError, "domain");
    assert.notProperty(invalidDomainError, "cause");
    const serializedInvalidDomainError = JSON.stringify(invalidDomainError);
    assert.notInclude(serializedInvalidDomainError, unsafeDomain);
    assert.notInclude(serializedInvalidDomainError, "domain-user");
    assert.notInclude(serializedInvalidDomainError, "domain-secret");
    assert.notInclude(serializedInvalidDomainError, "query-secret");
    assert.throws(
      () =>
        resolveMacPasskeySigningConfiguration({
          T3CODE_APPLE_TEAM_ID: "ABC1234567",
          T3CODE_MACOS_PROVISIONING_PROFILE: "/tmp/t3code.provisionprofile",
          T3CODE_CLERK_PASSKEY_RP_DOMAINS: "example.clerk.accounts.dev:8443",
        }),
      /Invalid passkey RP domain/u,
    );
    const invalidPublishableKeyError = captureError({
      T3CODE_APPLE_TEAM_ID: "ABC1234567",
      T3CODE_MACOS_PROVISIONING_PROFILE: "/tmp/t3code.provisionprofile",
      T3CODE_CLERK_PUBLISHABLE_KEY: "pk_test_%",
    });
    assert.instanceOf(invalidPublishableKeyError, InvalidMacPasskeyPublishableKeyError);
    assert.ok(invalidPublishableKeyError.cause);
    assert.equal(invalidPublishableKeyError.message, "T3CODE_CLERK_PUBLISHABLE_KEY is invalid.");
    assert.notProperty(invalidPublishableKeyError, "publishableKey");
    assert.notInclude(invalidPublishableKeyError.message, "pk_test_%");
  });

  it("preserves known passkey signing configuration errors at the build boundary", () => {
    const decodingCause = new Error("publishable-key-decode-failed");
    const knownError = new InvalidMacPasskeyPublishableKeyError({ cause: decodingCause });
    const error = MacPasskeySigningConfigurationResolutionError.fromCause(knownError);

    assert.strictEqual(error, knownError);
    assert.instanceOf(error, InvalidMacPasskeyPublishableKeyError);
    assert.strictEqual(error.cause, decodingCause);
    assert.isTrue(isMacPasskeySigningConfigurationError(error));
  });

  it("wraps unknown passkey signing configuration defects without copying cause text", () => {
    const secret = "pk_test_do-not-retain";
    const cause = new Error(secret);
    const error = MacPasskeySigningConfigurationResolutionError.fromCause(cause);

    assert.instanceOf(error, MacPasskeySigningConfigurationResolutionError);
    assert.strictEqual(error.cause, cause);
    assert.equal(error.message, "Failed to resolve macOS passkey signing configuration.");
    assert.notInclude(error.message, secret);
  });

  it.effect("adds passkey entitlements and both renderer protocols to signed macOS builds", () =>
    Effect.gen(function* () {
      const config = yield* createBuildConfig(
        "mac",
        "dmg",
        "1.2.3",
        true,
        false,
        undefined,
        {
          entitlementsPath: "/tmp/entitlements.mac.plist",
          provisioningProfilePath: "/tmp/t3code.provisionprofile",
        },
        "/tmp/stage/electron-builder-hooks.cjs",
      );

      const mac = config.mac as Record<string, unknown>;
      assert.equal(config.appId, "com.t3tools.t3code");
      assert.equal(mac.entitlements, "/tmp/entitlements.mac.plist");
      assert.equal(mac.provisioningProfile, "/tmp/t3code.provisionprofile");
      assert.deepStrictEqual(mac.protocols, [
        { name: "T3 Code", schemes: ["t3code", "t3code-dev"] },
      ]);
    }).pipe(Effect.provide(ConfigProvider.layer(ConfigProvider.fromEnv({ env: {} })))),
  );

  it.effect("keeps executable resource editing enabled for unsigned Windows builds", () =>
    Effect.gen(function* () {
      const config = yield* createBuildConfig(
        "win",
        "nsis",
        "1.2.3",
        false,
        false,
        undefined,
        undefined,
        "/tmp/stage/electron-builder-hooks.cjs",
      );

      const win = config.win as Record<string, unknown>;
      assert.equal(win.icon, "icon.ico");
      assert.equal(win.signAndEditExecutable, true);
      assert.notProperty(win, "azureSignOptions");
    }).pipe(Effect.provide(ConfigProvider.layer(ConfigProvider.fromEnv({ env: {} })))),
  );

  it.effect("keeps TypeScript's standard-library declarations in packaged builds", () =>
    Effect.gen(function* () {
      const config = yield* createBuildConfig(
        "mac",
        "dmg",
        "1.2.3",
        false,
        false,
        undefined,
        undefined,
        "/tmp/stage/electron-builder-hooks.cjs",
      );

      // onNodeModuleFile rescues typescript/lib/*.d.ts from electron-builder's
      // default *.d.ts exclusion; without it the packaged vtsls loads zero
      // default libs and flags every ambient global (Error, JSON, ...) as
      // "Cannot find name". The hook is only honored alongside a files matcher.
      assert.equal(config.onNodeModuleFile, "/tmp/stage/electron-builder-hooks.cjs");
      assert.equal(config.afterPack, "/tmp/stage/electron-builder-hooks.cjs");
      assert.deepStrictEqual(config.files, [{ filter: ["**/*"] }]);
    }).pipe(Effect.provide(ConfigProvider.layer(ConfigProvider.fromEnv({ env: {} })))),
  );

  it.effect("force-includes exactly typescript/lib declaration files", () =>
    Effect.gen(function* () {
      const fs = yield* FileSystem.FileSystem;
      const path = yield* Path.Path;
      const hooksDir = yield* fs.makeTempDirectoryScoped({ prefix: "t3-builder-hooks-" });
      const hooksPath = path.join(hooksDir, BUILDER_HOOKS_FILENAME);
      yield* fs.writeFileString(hooksPath, BUILDER_HOOKS_SOURCE);
      // The hooks module is loaded exactly the way electron-builder loads it:
      // a CommonJS require of the staged absolute path.
      const hooks = NodeModule.createRequire(import.meta.url)(hooksPath) as {
        onNodeModuleFile: (filePath: string) => boolean;
      };

      assert.isTrue(hooks.onNodeModuleFile("/stage/node_modules/typescript/lib/lib.es5.d.ts"));
      assert.isTrue(
        hooks.onNodeModuleFile(
          "/stage/node_modules/@vtsls/language-service/node_modules/typescript/lib/lib.dom.d.ts",
        ),
      );
      assert.isTrue(
        hooks.onNodeModuleFile("C:\\stage\\node_modules\\typescript\\lib\\lib.es2020.d.ts"),
      );
      assert.isFalse(hooks.onNodeModuleFile("/stage/node_modules/typescript/lib/tsserver.js"));
      assert.isFalse(hooks.onNodeModuleFile("/stage/node_modules/effect/dist/index.d.ts"));
    }).pipe(Effect.scoped),
  );

  // Satisfies the afterPack probe for the SDK metadata the managed Claude Code
  // installer reads at runtime.
  const stageClaudeSdkRuntimeFixtures = Effect.fn("stageClaudeSdkRuntimeFixtures")(function* (
    unpackedDir: string,
  ) {
    const fs = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    const sdkDir = path.join(unpackedDir, "node_modules", "@anthropic-ai", "claude-agent-sdk");
    yield* fs.makeDirectory(sdkDir, { recursive: true });
    yield* fs.writeFileString(path.join(sdkDir, "sdk.mjs"), "export {};");
    yield* fs.writeFileString(path.join(sdkDir, "manifest.json"), "{}");
  });

  it.effect("afterPack asserts both packaged TypeScript standard-library copies", () =>
    Effect.gen(function* () {
      const fs = yield* FileSystem.FileSystem;
      const path = yield* Path.Path;
      const hooksDir = yield* fs.makeTempDirectoryScoped({ prefix: "t3-builder-hooks-" });
      const hooksPath = path.join(hooksDir, BUILDER_HOOKS_FILENAME);
      yield* fs.writeFileString(hooksPath, BUILDER_HOOKS_SOURCE);
      const hooks = NodeModule.createRequire(import.meta.url)(hooksPath) as {
        afterPack: (context: unknown) => Promise<void>;
      };

      const appOutDir = path.join(hooksDir, "out");
      const libDir = path.join(appOutDir, "resources", "app.asar.unpacked", "node_modules");
      const topLevelLib = path.join(libDir, "typescript", "lib", "lib.es5.d.ts");
      // The nested copy is the one vtsls actually forks tsserver from at runtime.
      const nestedLib = path.join(
        libDir,
        "@vtsls",
        "language-service",
        "node_modules",
        "typescript",
        "lib",
        "lib.es5.d.ts",
      );
      const context = { electronPlatformName: "linux", appOutDir };

      yield* fs.makeDirectory(path.dirname(topLevelLib), { recursive: true });
      yield* fs.writeFileString(topLevelLib, "declare interface Object {}");
      // Top-level copy alone is not enough: the build must fail while the
      // nested copy is still missing.
      const missingNested = yield* Effect.promise(() =>
        hooks.afterPack(context).then(
          () => "resolved",
          (error: unknown) => String(error),
        ),
      );
      assert.include(missingNested, "TypeScript standard library missing");
      assert.include(missingNested, "@vtsls");

      yield* fs.makeDirectory(path.dirname(nestedLib), { recursive: true });
      yield* fs.writeFileString(nestedLib, "declare interface Object {}");
      yield* stageClaudeSdkRuntimeFixtures(path.join(appOutDir, "resources", "app.asar.unpacked"));
      yield* Effect.promise(() => hooks.afterPack(context));
    }).pipe(Effect.scoped),
  );

  it.effect("stripStagedSourcemaps removes only sourcemap files", () =>
    Effect.gen(function* () {
      const fs = yield* FileSystem.FileSystem;
      const path = yield* Path.Path;
      const distDir = yield* fs.makeTempDirectoryScoped({ prefix: "t3-strip-maps-" });
      const assetsDir = path.join(distDir, "client", "assets");
      yield* fs.makeDirectory(assetsDir, { recursive: true });
      yield* fs.writeFileString(path.join(distDir, "bin.mjs"), "export {};");
      yield* fs.writeFileString(path.join(distDir, "bin.mjs.map"), "{}");
      yield* fs.writeFileString(path.join(assetsDir, "index.js"), "0;");
      yield* fs.writeFileString(path.join(assetsDir, "index.js.map"), "{}");
      yield* fs.writeFileString(path.join(assetsDir, "index.css.map"), "{}");
      // Files that merely end in .map without a sourcemap extension must survive.
      yield* fs.writeFileString(path.join(assetsDir, "regions.map"), "data");

      yield* stripStagedSourcemaps(distDir, "test-dist");

      assert.isTrue(yield* fs.exists(path.join(distDir, "bin.mjs")));
      assert.isTrue(yield* fs.exists(path.join(assetsDir, "index.js")));
      assert.isTrue(yield* fs.exists(path.join(assetsDir, "regions.map")));
      assert.isFalse(yield* fs.exists(path.join(distDir, "bin.mjs.map")));
      assert.isFalse(yield* fs.exists(path.join(assetsDir, "index.js.map")));
      assert.isFalse(yield* fs.exists(path.join(assetsDir, "index.css.map")));
    }).pipe(Effect.scoped),
  );

  it.effect("afterPack rejects shipped sourcemaps", () =>
    Effect.gen(function* () {
      const fs = yield* FileSystem.FileSystem;
      const path = yield* Path.Path;
      const hooksDir = yield* fs.makeTempDirectoryScoped({ prefix: "t3-builder-hooks-" });
      const hooksPath = path.join(hooksDir, BUILDER_HOOKS_FILENAME);
      yield* fs.writeFileString(hooksPath, BUILDER_HOOKS_SOURCE);
      const hooks = NodeModule.createRequire(import.meta.url)(hooksPath) as {
        afterPack: (context: unknown) => Promise<void>;
      };

      const appOutDir = path.join(hooksDir, "out");
      const unpackedDir = path.join(appOutDir, "resources", "app.asar.unpacked");
      const context = { electronPlatformName: "linux", appOutDir };

      // Satisfy the TypeScript standard-library probes first.
      for (const libSegments of [
        ["node_modules", "typescript", "lib", "lib.es5.d.ts"],
        [
          "node_modules",
          "@vtsls",
          "language-service",
          "node_modules",
          "typescript",
          "lib",
          "lib.es5.d.ts",
        ],
      ]) {
        const libPath = path.join(unpackedDir, ...libSegments);
        yield* fs.makeDirectory(path.dirname(libPath), { recursive: true });
        yield* fs.writeFileString(libPath, "declare interface Object {}");
      }
      yield* stageClaudeSdkRuntimeFixtures(unpackedDir);
      yield* Effect.promise(() => hooks.afterPack(context));

      const serverDistMap = path.join(unpackedDir, "apps", "server", "dist", "bin.mjs.map");
      yield* fs.makeDirectory(path.dirname(serverDistMap), { recursive: true });
      yield* fs.writeFileString(serverDistMap, "{}");
      const serverDistFailure = yield* Effect.promise(() =>
        hooks.afterPack(context).then(
          () => "resolved",
          (error: unknown) => String(error),
        ),
      );
      assert.include(serverDistFailure, "Sourcemap shipped");
      assert.include(serverDistFailure, "bin.mjs.map");
      yield* fs.remove(serverDistMap);

      const nodeModulesMap = path.join(
        unpackedDir,
        "node_modules",
        "effect",
        "dist",
        "index.js.map",
      );
      yield* fs.makeDirectory(path.dirname(nodeModulesMap), { recursive: true });
      yield* fs.writeFileString(nodeModulesMap, "{}");
      const nodeModulesFailure = yield* Effect.promise(() =>
        hooks.afterPack(context).then(
          () => "resolved",
          (error: unknown) => String(error),
        ),
      );
      assert.include(nodeModulesFailure, "Sourcemap shipped");
      yield* fs.remove(nodeModulesMap);

      yield* Effect.promise(() => hooks.afterPack(context));
    }).pipe(Effect.scoped),
  );

  it("matches Claude SDK platform packages in scope and pnpm-store layouts", () => {
    assert.isTrue(isClaudeSdkPlatformPackagePath("@anthropic-ai/claude-agent-sdk-darwin-arm64"));
    assert.isTrue(
      isClaudeSdkPlatformPackagePath(".pnpm/@anthropic-ai+claude-agent-sdk-darwin-arm64@0.3.170"),
    );
    assert.isTrue(
      isClaudeSdkPlatformPackagePath(
        ".pnpm/@anthropic-ai+claude-agent-sdk@0.3.170_abc/node_modules/@anthropic-ai/claude-agent-sdk-linux-x64",
      ),
    );
    // The SDK package itself (and its files) must never match.
    assert.isFalse(isClaudeSdkPlatformPackagePath("@anthropic-ai/claude-agent-sdk"));
    assert.isFalse(isClaudeSdkPlatformPackagePath("@anthropic-ai/claude-agent-sdk/sdk.mjs"));
    assert.isFalse(
      isClaudeSdkPlatformPackagePath(".pnpm/@anthropic-ai+claude-agent-sdk@0.3.170_abc"),
    );
    assert.isFalse(isClaudeSdkPlatformPackagePath("@anthropic-ai/sdk"));
  });

  it.effect("pruneClaudeSdkPlatformPackages removes platform payloads but keeps the SDK", () =>
    Effect.gen(function* () {
      const fs = yield* FileSystem.FileSystem;
      const path = yield* Path.Path;
      const stageAppDir = yield* fs.makeTempDirectoryScoped({ prefix: "t3-claude-prune-" });
      const nodeModulesDir = path.join(stageAppDir, "node_modules");
      const sdkDir = path.join(nodeModulesDir, "@anthropic-ai", "claude-agent-sdk");
      const scopePlatformDir = path.join(
        nodeModulesDir,
        "@anthropic-ai",
        "claude-agent-sdk-darwin-arm64",
      );
      const storePlatformDir = path.join(
        nodeModulesDir,
        ".pnpm",
        "@anthropic-ai+claude-agent-sdk-darwin-arm64@0.3.170",
        "node_modules",
        "@anthropic-ai",
        "claude-agent-sdk-darwin-arm64",
      );
      for (const dir of [sdkDir, scopePlatformDir, storePlatformDir]) {
        yield* fs.makeDirectory(dir, { recursive: true });
      }
      yield* fs.writeFileString(path.join(sdkDir, "sdk.mjs"), "export {};");
      yield* fs.writeFileString(path.join(sdkDir, "manifest.json"), "{}");
      yield* fs.writeFileString(path.join(scopePlatformDir, "claude"), "binary");
      yield* fs.writeFileString(path.join(storePlatformDir, "claude"), "binary");

      yield* pruneClaudeSdkPlatformPackages(stageAppDir);

      assert.isTrue(yield* fs.exists(path.join(sdkDir, "sdk.mjs")));
      assert.isTrue(yield* fs.exists(path.join(sdkDir, "manifest.json")));
      assert.isFalse(yield* fs.exists(scopePlatformDir));
      assert.isFalse(
        yield* fs.exists(
          path.join(nodeModulesDir, ".pnpm", "@anthropic-ai+claude-agent-sdk-darwin-arm64@0.3.170"),
        ),
      );

      // A tree with no platform packages means the SDK layout changed; the
      // build must fail rather than silently ship the bundled binary again.
      const failure = yield* pruneClaudeSdkPlatformPackages(stageAppDir).pipe(Effect.flip);
      assert.instanceOf(failure, ClaudeSdkPlatformPackagesNotFoundError);
    }).pipe(Effect.scoped),
  );

  it.effect("afterPack rejects shipped Claude platform packages and requires SDK metadata", () =>
    Effect.gen(function* () {
      const fs = yield* FileSystem.FileSystem;
      const path = yield* Path.Path;
      const hooksDir = yield* fs.makeTempDirectoryScoped({ prefix: "t3-builder-hooks-" });
      const hooksPath = path.join(hooksDir, BUILDER_HOOKS_FILENAME);
      yield* fs.writeFileString(hooksPath, BUILDER_HOOKS_SOURCE);
      const hooks = NodeModule.createRequire(import.meta.url)(hooksPath) as {
        afterPack: (context: unknown) => Promise<void>;
      };

      const appOutDir = path.join(hooksDir, "out");
      const unpackedDir = path.join(appOutDir, "resources", "app.asar.unpacked");
      const context = { electronPlatformName: "linux", appOutDir };

      for (const libSegments of [
        ["node_modules", "typescript", "lib", "lib.es5.d.ts"],
        [
          "node_modules",
          "@vtsls",
          "language-service",
          "node_modules",
          "typescript",
          "lib",
          "lib.es5.d.ts",
        ],
      ]) {
        const libPath = path.join(unpackedDir, ...libSegments);
        yield* fs.makeDirectory(path.dirname(libPath), { recursive: true });
        yield* fs.writeFileString(libPath, "declare interface Object {}");
      }

      // Missing SDK metadata must fail: the managed installer reads it at runtime.
      const missingMetadata = yield* Effect.promise(() =>
        hooks.afterPack(context).then(
          () => "resolved",
          (error: unknown) => String(error),
        ),
      );
      assert.include(missingMetadata, "Claude Agent SDK runtime file missing");

      yield* stageClaudeSdkRuntimeFixtures(unpackedDir);
      yield* Effect.promise(() => hooks.afterPack(context));

      const platformPackageFile = path.join(
        unpackedDir,
        "node_modules",
        "@anthropic-ai",
        "claude-agent-sdk-darwin-arm64",
        "claude",
      );
      yield* fs.makeDirectory(path.dirname(platformPackageFile), { recursive: true });
      yield* fs.writeFileString(platformPackageFile, "binary");
      const shippedPlatformPackage = yield* Effect.promise(() =>
        hooks.afterPack(context).then(
          () => "resolved",
          (error: unknown) => String(error),
        ),
      );
      assert.include(shippedPlatformPackage, "Claude Agent SDK platform binary package shipped");
      yield* fs.remove(path.dirname(platformPackageFile), { recursive: true });

      yield* Effect.promise(() => hooks.afterPack(context));
    }).pipe(Effect.scoped),
  );

  it("promotes target fff binaries to direct staged dependencies", () => {
    assert.deepStrictEqual(resolveFffNativeDependencies("mac", "arm64", "0.9.4"), {
      "@ff-labs/fff-bin-darwin-arm64": "0.9.4",
    });
    assert.deepStrictEqual(resolveFffNativeDependencies("mac", "universal", "0.9.4"), {
      "@ff-labs/fff-bin-darwin-arm64": "0.9.4",
      "@ff-labs/fff-bin-darwin-x64": "0.9.4",
    });
    assert.deepStrictEqual(resolveFffNativeDependencies("win", "x64", "0.9.4"), {
      "@ff-labs/fff-bin-win32-x64": "0.9.4",
    });
    assert.deepStrictEqual(resolveFffNativeDependencies("linux", "x64", "0.9.4"), {
      "@ff-labs/fff-bin-linux-x64-gnu": "0.9.4",
      "@ff-labs/fff-bin-linux-x64-musl": "0.9.4",
    });
    assert.deepStrictEqual(resolveFffNativeDependencies("linux", "arm64", "0.9.4"), {
      "@ff-labs/fff-bin-linux-arm64-gnu": "0.9.4",
      "@ff-labs/fff-bin-linux-arm64-musl": "0.9.4",
    });
  });

  it("resolves target Clerk passkey native artifacts", () => {
    assert.deepStrictEqual(resolveClerkPasskeyNativeArtifacts("mac", "universal"), [
      {
        packageName: "@clerk/electron-passkeys-darwin-arm64",
        binaryFileName: "electron-passkeys.darwin-arm64.node",
      },
      {
        packageName: "@clerk/electron-passkeys-darwin-x64",
        binaryFileName: "electron-passkeys.darwin-x64.node",
      },
    ]);
    assert.deepStrictEqual(resolveClerkPasskeyNativeArtifacts("win", "x64"), [
      {
        packageName: "@clerk/electron-passkeys-win32-x64-msvc",
        binaryFileName: "electron-passkeys.win32-x64-msvc.node",
      },
    ]);
    assert.deepStrictEqual(resolveClerkPasskeyNativeArtifacts("linux", "x64"), []);
  });

  it("falls back to the default mock update port when the configured port is blank", () => {
    assert.equal(resolveMockUpdateServerUrl(undefined), "http://localhost:3000");
    assert.equal(resolveMockUpdateServerUrl(4123), "http://localhost:4123");
  });

  it("derives the electron-builder package manager user agent from packageManager", () => {
    assert.equal(resolvePackageManagerUserAgent("pnpm@11.10.0"), "pnpm/11.10.0");
    assert.equal(resolvePackageManagerUserAgent(" yarn@4.9.2 "), "yarn/4.9.2");
    assert.equal(resolvePackageManagerUserAgent("pnpm"), "pnpm");
  });

  it.effect("normalizes mock update server ports from env-style strings", () =>
    Effect.gen(function* () {
      assert.equal(yield* resolveMockUpdateServerPort(undefined), undefined);
      assert.equal(yield* resolveMockUpdateServerPort(""), undefined);
      assert.equal(yield* resolveMockUpdateServerPort("   "), undefined);
      assert.equal(yield* resolveMockUpdateServerPort("4123"), 4123);
    }),
  );

  it.effect("rejects non-numeric or out-of-range mock update ports", () =>
    Effect.gen(function* () {
      const invalidPorts = ["abc", "12.5", "0", "65536"];
      for (const port of invalidPorts) {
        const exit = yield* Effect.exit(resolveMockUpdateServerPort(port));
        assert.equal(exit._tag, "Failure");
      }
    }),
  );

  it("classifies invalid configured ports with the decoder's number grammar", () => {
    const cause = new Error("invalid configured port");

    assert.equal(
      InvalidMockUpdateServerPortError.fromConfigValue("0x10", cause).reason,
      "not-numeric",
    );
    assert.equal(
      InvalidMockUpdateServerPortError.fromConfigValue("12.5", cause).reason,
      "not-integer",
    );
    assert.equal(
      InvalidMockUpdateServerPortError.fromConfigValue("65536", cause).reason,
      "out-of-range",
    );
    assert.strictEqual(
      InvalidMockUpdateServerPortError.fromConfigValue("0x10", cause).cause,
      cause,
    );
  });

  it.effect("resolves default platform and architecture from host references", () =>
    Effect.gen(function* () {
      const resolved = yield* resolveBuildOptions({
        platform: Option.none(),
        target: Option.none(),
        arch: Option.none(),
        buildVersion: Option.none(),
        outputDir: Option.none(),
        skipBuild: Option.none(),
        keepStage: Option.none(),
        signed: Option.none(),
        verbose: Option.none(),
        mockUpdates: Option.none(),
        mockUpdateServerPort: Option.none(),
        wslPrebuild: Option.none(),
      }).pipe(
        Effect.provide(
          Layer.mergeAll(
            Layer.succeed(HostProcessPlatform, "win32"),
            Layer.succeed(HostProcessArchitecture, "x64"),
            ConfigProvider.layer(
              ConfigProvider.fromEnv({
                env: {
                  PROCESSOR_ARCHITECTURE: "AMD64",
                  PROCESSOR_ARCHITEW6432: "ARM64",
                },
              }),
            ),
          ),
        ),
      );

      assert.equal(resolved.platform, "win");
      assert.equal(resolved.target, "nsis");
      assert.equal(resolved.arch, "arm64");
    }),
  );

  it.effect("preserves explicit false boolean flags over true env defaults", () =>
    Effect.gen(function* () {
      const resolved = yield* resolveBuildOptions({
        platform: Option.some("mac"),
        target: Option.none(),
        arch: Option.some("arm64"),
        buildVersion: Option.none(),
        outputDir: Option.some("release-test"),
        skipBuild: Option.some(false),
        keepStage: Option.some(false),
        signed: Option.some(false),
        verbose: Option.some(false),
        mockUpdates: Option.some(false),
        mockUpdateServerPort: Option.none(),
        wslPrebuild: Option.none(),
      }).pipe(
        Effect.provide(
          ConfigProvider.layer(
            ConfigProvider.fromEnv({
              env: {
                T3CODE_DESKTOP_SKIP_BUILD: "true",
                T3CODE_DESKTOP_KEEP_STAGE: "true",
                T3CODE_DESKTOP_SIGNED: "true",
                T3CODE_DESKTOP_VERBOSE: "true",
                T3CODE_DESKTOP_MOCK_UPDATES: "true",
              },
            }),
          ),
        ),
      );

      assert.equal(resolved.skipBuild, false);
      assert.equal(resolved.keepStage, false);
      assert.equal(resolved.signed, false);
      assert.equal(resolved.verbose, false);
      assert.equal(resolved.mockUpdates, false);
    }),
  );
});
