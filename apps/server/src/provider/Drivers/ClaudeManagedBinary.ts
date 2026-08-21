// @effect-diagnostics nodeBuiltinImport:off
import * as NodeCrypto from "node:crypto";
import * as NodeModule from "node:module";
import * as NodePath from "node:path";

import type { ClaudeBinaryInstallProgressStage, ClaudeBinaryStatus } from "@t3tools/contracts";
import { DEFAULT_CLAUDE_BINARY_PATH } from "@t3tools/contracts";
import {
  HostProcessArchitecture,
  HostProcessEnvironment,
  HostProcessPlatform,
} from "@t3tools/shared/hostProcess";
import { resolveCommandPath } from "@t3tools/shared/shell";
import * as Clock from "effect/Clock";
import * as Context from "effect/Context";
import * as Crypto from "effect/Crypto";
import * as Data from "effect/Data";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Option from "effect/Option";
import * as Path from "effect/Path";
import * as Schema from "effect/Schema";
import * as Semaphore from "effect/Semaphore";
import * as Sink from "effect/Sink";
import * as Stream from "effect/Stream";
import { HttpClient, HttpClientRequest, HttpClientResponse } from "effect/unstable/http";
import { ChildProcess, ChildProcessSpawner } from "effect/unstable/process";

import { resolveClaudeSdkExecutablePath } from "./ClaudeExecutable.ts";

export const CLAUDE_BINARY_PATH_ENV_NAME = "T3CODE_CLAUDE_BINARY_PATH";
export const NPM_REGISTRY_BASE_URL = "https://registry.npmjs.org";

const INSTALL_LOCK_RETRY_COUNT = 100;
const INSTALL_LOCK_RETRY_DELAY = "100 millis";
const INSTALL_LOCK_STALE_MS = 5 * 60 * 1_000;
/** Report download progress at most every 4MiB so the stream stays quiet. */
const DOWNLOAD_PROGRESS_STEP_BYTES = 4 * 1024 * 1024;

export interface ClaudeSdkPlatformBinary {
  readonly binary: string;
  readonly checksum: string;
  readonly size: number;
}

/**
 * Identity of the Claude Code build pinned by the installed
 * @anthropic-ai/claude-agent-sdk: its npm version selects the platform binary
 * package to download, its manifest.json carries the CLI version plus a
 * SHA-256 per platform binary.
 */
export interface ClaudeSdkPackageInfo {
  readonly npmVersion: string;
  readonly cliVersion: string;
  readonly platforms: Readonly<Record<string, ClaudeSdkPlatformBinary>>;
}

export interface ClaudeBinaryInstallProgress {
  readonly stage: ClaudeBinaryInstallProgressStage;
  readonly bytesDownloaded?: number;
  readonly totalBytes?: number;
}

export type AvailableClaudeBinary = Extract<ClaudeBinaryStatus, { status: "available" }>;

export class ClaudeBinaryInstallError extends Data.TaggedError("ClaudeBinaryInstallError")<{
  readonly reason:
    | "download_failed"
    | "invalid_checksum"
    | "install_locked"
    | "override_missing"
    | "unsupported_platform"
    | "validation_failed"
    | "write_failed";
  readonly message: string;
  readonly cause?: unknown;
}> {}

class ClaudeBinaryCommandError extends Data.TaggedError("ClaudeBinaryCommandError")<{
  readonly command: string;
  readonly exitCode: number;
}> {}

class ClaudeSdkPackageInfoError extends Data.TaggedError("ClaudeSdkPackageInfoError")<{
  readonly message: string;
  readonly cause?: unknown;
}> {}

export interface ClaudeManagedBinaryOptions {
  readonly baseDir: string;
  /** Overrides SDK package discovery; tests inject a fake build identity. */
  readonly sdkInfo?: ClaudeSdkPackageInfo;
  /** Overrides the npm registry origin; tests point this at a local server. */
  readonly registryBaseUrl?: string;
}

export interface ClaudeManagedBinaryShape {
  /**
   * Full executable resolution for the given (already decoded) binaryPath
   * setting: explicit user path → env override → managed download → PATH.
   */
  readonly status: (
    binaryPath: string,
    environment: NodeJS.ProcessEnv,
  ) => Effect.Effect<ClaudeBinaryStatus>;
  readonly install: Effect.Effect<AvailableClaudeBinary, ClaudeBinaryInstallError>;
  readonly installWithProgress: (
    report: (progress: ClaudeBinaryInstallProgress) => Effect.Effect<void>,
  ) => Effect.Effect<AvailableClaudeBinary, ClaudeBinaryInstallError>;
}

export class ClaudeManagedBinary extends Context.Service<
  ClaudeManagedBinary,
  ClaudeManagedBinaryShape
>()("t3/provider/Drivers/ClaudeManagedBinary") {}

export function resolveClaudePlatformKey(platform: NodeJS.Platform, arch: string): string {
  if (platform === "linux" && isMuslLibc()) {
    return `linux-${arch}-musl`;
  }
  return `${platform}-${arch}`;
}

function isMuslLibc(): boolean {
  try {
    const report = process.report?.getReport() as
      | { header?: { glibcVersionRuntime?: unknown } }
      | undefined;
    return report !== undefined && report.header?.glibcVersionRuntime === undefined;
  } catch {
    return false;
  }
}

export function claudePlatformPackageName(platformKey: string): string {
  return `@anthropic-ai/claude-agent-sdk-${platformKey}`;
}

export function claudeNpmTarballUrl(
  registryBaseUrl: string,
  platformKey: string,
  npmVersion: string,
): string {
  const unscopedName = `claude-agent-sdk-${platformKey}`;
  return `${registryBaseUrl}/@anthropic-ai/${unscopedName}/-/${unscopedName}-${npmVersion}.tgz`;
}

const decodeSdkPackageJson = Schema.decodeUnknownEffect(
  Schema.fromJsonString(Schema.Struct({ version: Schema.String })),
);
const decodeSdkManifestJson = Schema.decodeUnknownEffect(
  Schema.fromJsonString(
    Schema.Struct({
      version: Schema.String,
      platforms: Schema.optional(
        Schema.Record(
          Schema.String,
          Schema.Struct({
            binary: Schema.String,
            checksum: Schema.String,
            size: Schema.Number,
          }),
        ),
      ),
    }),
  ),
);

/**
 * Locates the installed SDK package on disk and reads its build identity. The
 * SDK's `exports` map has no `./package.json` subpath, so this resolves the
 * main entry and reads both JSON files beside it (real files even when
 * packaged — the desktop build asar-unpacks node_modules).
 */
const loadSdkPackageInfo = Effect.fn("claudeBinary.loadSdkPackageInfo")(function* () {
  const fileSystem = yield* FileSystem.FileSystem;
  const packageDir = yield* Effect.try({
    try: () =>
      NodePath.dirname(
        NodeModule.createRequire(import.meta.url).resolve("@anthropic-ai/claude-agent-sdk"),
      ),
    catch: (cause) =>
      new ClaudeSdkPackageInfoError({
        message: "Could not locate the installed Claude Agent SDK package.",
        cause,
      }),
  });
  const packageJson = yield* fileSystem
    .readFileString(NodePath.join(packageDir, "package.json"))
    .pipe(Effect.flatMap(decodeSdkPackageJson));
  const manifest = yield* fileSystem
    .readFileString(NodePath.join(packageDir, "manifest.json"))
    .pipe(Effect.flatMap(decodeSdkManifestJson));
  return {
    npmVersion: packageJson.version,
    cliVersion: manifest.version,
    platforms: manifest.platforms ?? {},
  } satisfies ClaudeSdkPackageInfo;
});

const wrapInstallFailure =
  (
    reason: ClaudeBinaryInstallError["reason"],
    message: string,
  ): (<A, E, R>(effect: Effect.Effect<A, E, R>) => Effect.Effect<A, ClaudeBinaryInstallError, R>) =>
  (effect) =>
    effect.pipe(
      Effect.mapError((cause) => new ClaudeBinaryInstallError({ reason, message, cause })),
    );

export const makeClaudeManagedBinary = Effect.fn("claudeBinary.make")(function* (
  options: ClaudeManagedBinaryOptions,
): Effect.fn.Return<
  ClaudeManagedBinaryShape,
  never,
  | ChildProcessSpawner.ChildProcessSpawner
  | Crypto.Crypto
  | FileSystem.FileSystem
  | HttpClient.HttpClient
  | Path.Path
> {
  const crypto = yield* Crypto.Crypto;
  const fileSystem = yield* FileSystem.FileSystem;
  const httpClient = yield* HttpClient.HttpClient;
  const path = yield* Path.Path;
  const spawner = yield* ChildProcessSpawner.ChildProcessSpawner;
  const installSemaphore = yield* Semaphore.make(1);
  const platform = yield* HostProcessPlatform;
  const arch = yield* HostProcessArchitecture;
  const registryBaseUrl = options.registryBaseUrl ?? NPM_REGISTRY_BASE_URL;

  const sdkInfo =
    options.sdkInfo ??
    (yield* loadSdkPackageInfo().pipe(
      Effect.tapCause((cause) =>
        Effect.logWarning("Could not read the Claude Agent SDK build identity.", { cause }),
      ),
      Effect.catch(() => Effect.succeed(null)),
    ));
  const platformKey = resolveClaudePlatformKey(platform, arch);
  const platformBinary = sdkInfo?.platforms[platformKey] ?? null;
  const cliVersion = sdkInfo?.cliVersion ?? "unknown";
  const managedPath =
    sdkInfo && platformBinary
      ? path.join(
          options.baseDir,
          "tools",
          "claude",
          sdkInfo.npmVersion,
          platformKey,
          platformBinary.binary,
        )
      : null;

  const isExecutableFile = Effect.fn("claudeBinary.isExecutableFile")(function* (
    executablePath: string,
  ) {
    const info = yield* fileSystem.stat(executablePath).pipe(Effect.option);
    if (Option.isNone(info) || info.value.type !== "File") return false;
    return platform === "win32" || (info.value.mode & 0o111) !== 0;
  });

  const missingStatus = (): ClaudeBinaryStatus =>
    platformBinary
      ? { status: "missing", version: cliVersion, binarySizeBytes: platformBinary.size }
      : { status: "unsupported", platform, arch, version: cliVersion };

  const statusImpl = Effect.fn("claudeBinary.status")(function* (
    binaryPath: string,
    environment: NodeJS.ProcessEnv,
  ) {
    // An explicit user-configured path always wins; it is passed through the
    // Windows shim resolution and trusted otherwise (the provider capability
    // probe surfaces a broken path with its own actionable error).
    if (binaryPath !== DEFAULT_CLAUDE_BINARY_PATH) {
      const executablePath = yield* resolveClaudeSdkExecutablePath(binaryPath, environment);
      return {
        status: "available",
        executablePath,
        source: "explicit",
        version: cliVersion,
      } satisfies ClaudeBinaryStatus;
    }
    const override = environment[CLAUDE_BINARY_PATH_ENV_NAME]?.trim();
    if (override) {
      return (yield* isExecutableFile(override))
        ? ({
            status: "available",
            executablePath: override,
            source: "override",
            version: cliVersion,
          } satisfies ClaudeBinaryStatus)
        : missingStatus();
    }
    if (managedPath && (yield* isExecutableFile(managedPath))) {
      return {
        status: "available",
        executablePath: managedPath,
        source: "managed",
        version: cliVersion,
      } satisfies ClaudeBinaryStatus;
    }
    const pathExecutable = yield* resolveCommandPath(DEFAULT_CLAUDE_BINARY_PATH, {
      env: environment,
    }).pipe(
      Effect.flatMap((resolved) => resolveClaudeSdkExecutablePath(resolved, environment)),
      Effect.catchTag("CommandResolutionError", () => Effect.succeed(null)),
    );
    if (pathExecutable) {
      return {
        status: "available",
        executablePath: pathExecutable,
        source: "path",
        version: cliVersion,
      } satisfies ClaudeBinaryStatus;
    }
    return missingStatus();
  });

  // resolveCommandPath reads PATH via FileSystem/Path; satisfy those from the
  // handles captured at construction so the shape stays context-free.
  const status: ClaudeManagedBinaryShape["status"] = (binaryPath, environment) =>
    statusImpl(binaryPath, environment).pipe(
      Effect.provideService(FileSystem.FileSystem, fileSystem),
      Effect.provideService(Path.Path, path),
    );

  const runCommand = Effect.fn("claudeBinary.runCommand")(function* (
    command: string,
    args: ReadonlyArray<string>,
  ) {
    const child = yield* spawner.spawn(
      ChildProcess.make(command, args, {
        shell: false,
        stdout: "ignore",
        stderr: "ignore",
      }),
    );
    const exitCode = Number(yield* child.exitCode);
    if (exitCode !== 0) {
      return yield* new ClaudeBinaryCommandError({ command, exitCode });
    }
  });

  const downloadTarball = Effect.fn("claudeBinary.downloadTarball")(function* (
    url: string,
    destination: string,
    report: (progress: ClaudeBinaryInstallProgress) => Effect.Effect<void>,
  ) {
    const response = yield* httpClient.execute(HttpClientRequest.get(url)).pipe(
      Effect.flatMap(HttpClientResponse.filterStatusOk),
      Effect.mapError(
        (cause) =>
          new ClaudeBinaryInstallError({
            reason: "download_failed",
            message: "Could not download the Claude Code binary package.",
            cause,
          }),
      ),
    );
    const totalBytes = Number(response.headers["content-length"] ?? "") || undefined;
    const downloadingProgress = (bytesDownloaded: number): ClaudeBinaryInstallProgress => ({
      stage: "downloading",
      bytesDownloaded,
      ...(totalBytes === undefined ? {} : { totalBytes }),
    });
    yield* report(downloadingProgress(0));
    let bytesDownloaded = 0;
    let lastReportedBytes = 0;
    yield* response.stream.pipe(
      Stream.tap((chunk) => {
        bytesDownloaded += chunk.byteLength;
        if (bytesDownloaded - lastReportedBytes < DOWNLOAD_PROGRESS_STEP_BYTES) {
          return Effect.void;
        }
        lastReportedBytes = bytesDownloaded;
        return report(downloadingProgress(bytesDownloaded));
      }),
      Stream.run(fileSystem.sink(destination)),
      Effect.mapError(
        (cause) =>
          new ClaudeBinaryInstallError({
            reason: "download_failed",
            message: "Could not read the downloaded Claude Code binary package.",
            cause,
          }),
      ),
    );
  });

  // The pinned checksum covers the extracted binary (not the tarball), and the
  // binary is a few hundred MB — hash it as a stream instead of buffering it.
  const digestFileSha256Hex = Effect.fn("claudeBinary.digestFileSha256Hex")(function* (
    filePath: string,
  ) {
    const hash = NodeCrypto.createHash("sha256");
    yield* fileSystem.stream(filePath).pipe(
      Stream.tap((chunk) => Effect.sync(() => hash.update(chunk))),
      Stream.run(Sink.drain),
    );
    return hash.digest("hex");
  });

  const acquireInstallLock = Effect.fn("claudeBinary.acquireInstallLock")(function* (
    lockPath: string,
  ) {
    for (let attempt = 0; attempt < INSTALL_LOCK_RETRY_COUNT; attempt += 1) {
      const acquired = yield* fileSystem.writeFileString(lockPath, "", { flag: "wx" }).pipe(
        Effect.as(true),
        Effect.catch((error) =>
          error.reason._tag === "AlreadyExists" ? Effect.succeed(false) : Effect.fail(error),
        ),
      );
      if (acquired) return;

      const now = yield* Clock.currentTimeMillis;
      const lockInfo = yield* fileSystem.stat(lockPath).pipe(Effect.option);
      const mtime = Option.flatMap(lockInfo, (info) => info.mtime);
      if (Option.isSome(mtime) && now - mtime.value.getTime() > INSTALL_LOCK_STALE_MS) {
        yield* fileSystem.remove(lockPath, { force: true });
        continue;
      }
      yield* Effect.sleep(INSTALL_LOCK_RETRY_DELAY);
    }
    return yield* new ClaudeBinaryInstallError({
      reason: "install_locked",
      message: "Another Claude Code installation is still in progress.",
    });
  });

  // The managed binaries are ~300MB each, so unlike the cloudflared installer
  // this GC step matters: after activating a version, drop the older ones.
  const removeStaleVersions = (currentVersion: string) =>
    Effect.gen(function* () {
      const toolRoot = path.join(options.baseDir, "tools", "claude");
      const entries = yield* fileSystem.readDirectory(toolRoot);
      for (const entry of entries) {
        if (entry === currentVersion) continue;
        yield* fileSystem.remove(path.join(toolRoot, entry), { recursive: true, force: true });
      }
    }).pipe(Effect.ignore);

  const installUnlocked = Effect.fn("claudeBinary.installUnlocked")(function* (
    report: (progress: ClaudeBinaryInstallProgress) => Effect.Effect<void>,
  ) {
    yield* report({ stage: "checking" });
    const environment = yield* HostProcessEnvironment;
    const existing = yield* status(DEFAULT_CLAUDE_BINARY_PATH, environment);
    if (existing.status === "available") return existing;
    if (environment[CLAUDE_BINARY_PATH_ENV_NAME]?.trim()) {
      return yield* new ClaudeBinaryInstallError({
        reason: "override_missing",
        message: `${CLAUDE_BINARY_PATH_ENV_NAME} does not point to an executable file.`,
      });
    }
    if (!sdkInfo || !platformBinary || !managedPath) {
      return yield* new ClaudeBinaryInstallError({
        reason: "unsupported_platform",
        message: `No Claude Code binary is published for ${platform}-${arch}.`,
      });
    }

    const managedDirectory = path.dirname(managedPath);
    const lockPath = `${managedPath}.lock`;
    yield* fileSystem
      .makeDirectory(managedDirectory, { recursive: true })
      .pipe(wrapInstallFailure("write_failed", "Could not create the Claude Code tool directory."));
    yield* report({ stage: "waiting_for_lock" });
    yield* acquireInstallLock(lockPath).pipe(
      Effect.catchTag("PlatformError", (cause) =>
        Effect.fail(
          new ClaudeBinaryInstallError({
            reason: "write_failed",
            message: "Could not acquire the Claude Code installation lock.",
            cause,
          }),
        ),
      ),
    );
    return yield* Effect.gen(function* () {
      const afterLock = yield* status(DEFAULT_CLAUDE_BINARY_PATH, environment);
      if (afterLock.status === "available") return afterLock;

      const tempDirectory = yield* fileSystem.makeTempDirectoryScoped({
        directory: managedDirectory,
        prefix: ".install-",
      });
      const tarballPath = path.join(tempDirectory, "claude.tgz");
      const tarballUrl = claudeNpmTarballUrl(registryBaseUrl, platformKey, sdkInfo.npmVersion);
      yield* downloadTarball(tarballUrl, tarballPath, report);

      yield* report({ stage: "installing" });
      // npm tarballs root everything under "package/"; extract only the binary.
      const memberPath = `package/${platformBinary.binary}`;
      yield* runCommand("tar", ["-xzf", tarballPath, "-C", tempDirectory, memberPath]).pipe(
        wrapInstallFailure("write_failed", "Could not extract the Claude Code binary."),
      );
      const executablePath = path.join(tempDirectory, "package", platformBinary.binary);

      yield* report({ stage: "verifying" });
      const checksum = yield* digestFileSha256Hex(executablePath).pipe(
        wrapInstallFailure(
          "validation_failed",
          "Could not verify the downloaded Claude Code checksum.",
        ),
      );
      if (checksum !== platformBinary.checksum) {
        return yield* new ClaudeBinaryInstallError({
          reason: "invalid_checksum",
          message: "Downloaded Claude Code checksum did not match the SDK's pinned build.",
        });
      }

      if (platform !== "win32") {
        yield* fileSystem
          .chmod(executablePath, 0o755)
          .pipe(wrapInstallFailure("write_failed", "Could not make Claude Code executable."));
      }
      yield* report({ stage: "validating" });
      yield* runCommand(executablePath, ["--version"]).pipe(
        wrapInstallFailure("validation_failed", "The downloaded Claude Code binary did not run."),
      );

      const stagedPath = `${managedPath}.${yield* crypto.randomUUIDv4}.tmp`;
      yield* report({ stage: "activating" });
      yield* fileSystem
        .rename(executablePath, stagedPath)
        .pipe(wrapInstallFailure("write_failed", "Could not stage the Claude Code binary."));
      yield* fileSystem
        .rename(stagedPath, managedPath)
        .pipe(
          wrapInstallFailure("write_failed", "Could not activate the Claude Code binary."),
          Effect.ensuring(fileSystem.remove(stagedPath, { force: true }).pipe(Effect.ignore)),
        );
      yield* removeStaleVersions(sdkInfo.npmVersion);
      return {
        status: "available",
        executablePath: managedPath,
        source: "managed",
        version: cliVersion,
      } satisfies AvailableClaudeBinary;
    }).pipe(
      Effect.scoped,
      Effect.ensuring(fileSystem.remove(lockPath, { force: true }).pipe(Effect.ignore)),
      Effect.catch((cause) =>
        cause instanceof ClaudeBinaryInstallError
          ? Effect.fail(cause)
          : Effect.fail(
              new ClaudeBinaryInstallError({
                reason: "write_failed",
                message: "Could not install the Claude Code binary.",
                cause,
              }),
            ),
      ),
    );
  });

  const installWithProgress: ClaudeManagedBinaryShape["installWithProgress"] = (report) =>
    installSemaphore.withPermit(installUnlocked(report));
  const install = installWithProgress(() => Effect.void);

  return ClaudeManagedBinary.of({ status, install, installWithProgress });
});

export const layerClaudeManagedBinary = (options: ClaudeManagedBinaryOptions) =>
  Layer.effect(ClaudeManagedBinary, makeClaudeManagedBinary(options));

/**
 * Claude binary status via the managed-binary service when it is present in
 * context (the production server runtime); `Option.none` when absent (unit
 * tests, standalone flows), where callers keep their pre-managed behavior.
 */
export const claudeBinaryStatusOption = Effect.fn("claudeBinary.statusOption")(function* (
  binaryPath: string,
  environment: NodeJS.ProcessEnv,
): Effect.fn.Return<Option.Option<ClaudeBinaryStatus>> {
  const managedBinary = yield* Effect.serviceOption(ClaudeManagedBinary);
  if (Option.isNone(managedBinary)) return Option.none();
  return Option.some(yield* managedBinary.value.status(binaryPath, environment));
});

/**
 * Executable path spawn sites should use. Applies the managed precedence
 * (explicit → env override → managed download → PATH) when the service is
 * present; otherwise — and when nothing is installed — falls back to the
 * configured path so the caller's spawn failure keeps its existing
 * command-missing classification.
 */
export const resolveEffectiveClaudeExecutablePath = Effect.fn(
  "claudeBinary.resolveEffectiveExecutablePath",
)(function* (binaryPath: string, environment: NodeJS.ProcessEnv): Effect.fn.Return<string> {
  const binaryStatus = yield* claudeBinaryStatusOption(binaryPath, environment);
  if (Option.isSome(binaryStatus) && binaryStatus.value.status === "available") {
    return binaryStatus.value.executablePath;
  }
  return yield* resolveClaudeSdkExecutablePath(binaryPath, environment);
});
