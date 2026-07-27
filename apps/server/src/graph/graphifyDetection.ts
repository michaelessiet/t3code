/**
 * Pure detection logic for the graphify toolchain.
 *
 * Split out from the service so the interesting decisions — which candidates
 * to probe and in what order, what counts as a usable Python, how to read a
 * version out of whatever the tool printed — are testable without spawning
 * anything. The service in `GraphifyRuntime.ts` supplies the I/O.
 *
 * @module graphifyDetection
 */

/**
 * graphify is 0.x and moving fast (the published 0.9.27 is eight minor
 * versions ahead of the public repo), so the version is pinned exactly rather
 * than floated. A mismatch invalidates stored graphs instead of decoding a
 * `graph.json` whose shape may have changed.
 */
export const GRAPHIFY_PINNED_VERSION = "0.9.27";

/** The PyPI distribution name. The CLI it installs is `graphify`. */
export const GRAPHIFY_PACKAGE = "graphifyy";

/** graphify requires Python 3.10 or newer. */
export const MIN_PYTHON_MAJOR = 3;
export const MIN_PYTHON_MINOR = 10;

/**
 * Where a resolved graphify came from.
 *
 * - `setting`   — the explicit `knowledgeGraph.graphifyPath` override.
 * - `path`      — a `graphify` executable on PATH.
 * - `managed`   — the venv T3 Code created under its own data directory.
 * - `uv-tool`   — a `uv tool install graphifyy` the user did themselves, found
 *                 in uv's own bin directory.
 * - `module`    — `python3 -m graphify`, for a graphify installed into the
 *                 user's Python without a console script on PATH.
 */
export type GraphifyProbeKind = "setting" | "path" | "managed" | "uv-tool" | "module";

export interface GraphifyProbe {
  readonly kind: GraphifyProbeKind;
  readonly command: string;
  /** Prefix arguments that turn `command` into a graphify invocation. */
  readonly args: ReadonlyArray<string>;
}

export interface GraphifyProbeInput {
  /** `knowledgeGraph.graphifyPath`; empty means auto-detect. */
  readonly graphifyPath: string;
  /** `ServerConfig.graphRuntimeDir` — the managed venv root. */
  readonly graphRuntimeDir: string;
  /**
   * `uv tool dir --bin`, or null when uv is absent or did not answer.
   *
   * uv installs console scripts here — `~/.local/bin` by default — and does
   * *not* put the directory on PATH; it prints a "not on your PATH" warning and
   * leaves that to the user. So a perfectly good `uv tool install graphifyy` is
   * invisible to every other probe, which is exactly the gap this closes.
   */
  readonly uvToolBinDir: string | null;
  readonly platform: NodeJS.Platform;
}

/** Executable directory inside a venv: `Scripts` on Windows, `bin` elsewhere. */
export function venvBinDirectory(venvRoot: string, platform: NodeJS.Platform): string {
  const separator = platform === "win32" ? "\\" : "/";
  return `${venvRoot}${separator}${platform === "win32" ? "Scripts" : "bin"}`;
}

/** Path to an executable inside a venv, with the Windows `.exe` suffix. */
export function venvExecutable(venvRoot: string, name: string, platform: NodeJS.Platform): string {
  const separator = platform === "win32" ? "\\" : "/";
  const suffix = platform === "win32" ? ".exe" : "";
  return `${venvBinDirectory(venvRoot, platform)}${separator}${name}${suffix}`;
}

/**
 * Environment that pins `uv tool install` to T3 Code's own directory.
 *
 * Without this, uv installs into *its* defaults — the tool environment under
 * `~/.local/share/uv/tools` and the console script into `~/.local/bin` — and
 * then warns that the bin directory is not on PATH. T3 would have installed
 * graphify successfully and been unable to find it a second later, because the
 * `managed` probe looks under `graphRuntimeDir` and nothing put it there.
 *
 * Pointing `UV_TOOL_BIN_DIR` at exactly the path {@link venvExecutable} probes
 * makes the installer and the detector agree by construction rather than by
 * coincidence. It also keeps the install inside the directory T3 owns, so
 * removing T3's data directory removes graphify with it, and nothing is written
 * into the user's `~/.local/bin` behind their back. uv creates both directories
 * itself, so neither needs to exist first.
 */
export function uvInstallEnvironment(
  graphRuntimeDir: string,
  platform: NodeJS.Platform,
): Record<string, string> {
  const separator = platform === "win32" ? "\\" : "/";
  return {
    UV_TOOL_BIN_DIR: venvBinDirectory(graphRuntimeDir, platform),
    UV_TOOL_DIR: `${graphRuntimeDir}${separator}uv-tools`,
  };
}

/**
 * Ordered candidates to probe, most specific first.
 *
 * Deliberately excludes `uv tool run graphifyy`: `uv tool run` downloads and
 * installs the package when it is absent, which would turn a detection probe
 * into a silent install. The feature is opt-in, so nothing may install without
 * the user pressing a button. `uv` is still the preferred *installer* — see
 * `GraphifyRuntime.installWithProgress`.
 */
export function graphifyProbes(input: GraphifyProbeInput): ReadonlyArray<GraphifyProbe> {
  const probes: Array<GraphifyProbe> = [];
  const explicit = input.graphifyPath.trim();
  if (explicit !== "") {
    probes.push({ kind: "setting", command: explicit, args: [] });
  }
  probes.push({ kind: "path", command: "graphify", args: [] });
  probes.push({
    kind: "managed",
    command: venvExecutable(input.graphRuntimeDir, "graphify", input.platform),
    args: [],
  });
  // After `managed`: T3's own install is the one whose version it controls, so
  // a user's separate `uv tool install` is a fallback rather than a preference.
  if (input.uvToolBinDir !== null && input.uvToolBinDir.trim() !== "") {
    const separator = input.platform === "win32" ? "\\" : "/";
    const suffix = input.platform === "win32" ? ".exe" : "";
    probes.push({
      kind: "uv-tool",
      command: `${input.uvToolBinDir.trim()}${separator}graphify${suffix}`,
      args: [],
    });
  }
  probes.push({ kind: "module", command: "python3", args: ["-m", "graphify"] });
  return probes;
}

/**
 * Extract a version from `graphify --version` output.
 *
 * The exact format is not contractual, so this accepts anything containing a
 * dotted numeric version and takes the first one. Returns null when the output
 * has no version in it at all, which is treated as a failed probe.
 */
export function parseGraphifyVersion(output: string): string | null {
  const match = /\b(\d+\.\d+(?:\.\d+)?(?:[.\-+][0-9A-Za-z.-]+)?)\b/.exec(output);
  return match?.[1] ?? null;
}

export interface PythonVersion {
  readonly major: number;
  readonly minor: number;
}

/** Parse `python3 --version` output, e.g. `Python 3.12.1`. */
export function parsePythonVersion(output: string): PythonVersion | null {
  const match = /Python\s+(\d+)\.(\d+)/i.exec(output);
  if (match?.[1] === undefined || match[2] === undefined) return null;
  const major = Number.parseInt(match[1], 10);
  const minor = Number.parseInt(match[2], 10);
  if (Number.isNaN(major) || Number.isNaN(minor)) return null;
  return { major, minor };
}

/** True when the interpreter is new enough to run graphify. */
export function isSupportedPython(version: PythonVersion): boolean {
  if (version.major !== MIN_PYTHON_MAJOR) return version.major > MIN_PYTHON_MAJOR;
  return version.minor >= MIN_PYTHON_MINOR;
}

/**
 * How graphify should be installed, given what is on the machine. Pure so the
 * "what do I tell the user to install" message is testable.
 */
export type GraphifyInstallPlan =
  | { readonly kind: "uv" }
  | { readonly kind: "venv"; readonly python: string }
  | { readonly kind: "impossible"; readonly detail: string };

export interface GraphifyInstallPlanInput {
  readonly uvAvailable: boolean;
  /** Interpreter that answered `--version`, or null when none did. */
  readonly python: { readonly command: string; readonly version: PythonVersion } | null;
}

export function planGraphifyInstall(input: GraphifyInstallPlanInput): GraphifyInstallPlan {
  if (input.uvAvailable) return { kind: "uv" };
  if (input.python === null) {
    return {
      kind: "impossible",
      detail:
        `No Python interpreter was found. Install Python ${MIN_PYTHON_MAJOR}.${MIN_PYTHON_MINOR} ` +
        "or newer, or install uv (https://docs.astral.sh/uv/), then try again.",
    };
  }
  if (!isSupportedPython(input.python.version)) {
    const { major, minor } = input.python.version;
    return {
      kind: "impossible",
      detail:
        `Found Python ${major}.${minor}, but graphify needs ` +
        `${MIN_PYTHON_MAJOR}.${MIN_PYTHON_MINOR} or newer. Upgrade Python, or install uv ` +
        "(https://docs.astral.sh/uv/), then try again.",
    };
  }
  return { kind: "venv", python: input.python.command };
}

/** Pinned install argument, e.g. `graphifyy==0.9.27`. */
export function pinnedRequirement(): string {
  return `${GRAPHIFY_PACKAGE}==${GRAPHIFY_PINNED_VERSION}`;
}
