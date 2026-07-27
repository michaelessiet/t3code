import { describe, expect, it } from "@effect/vitest";

import {
  GRAPHIFY_PINNED_VERSION,
  graphifyProbes,
  isSupportedPython,
  parseGraphifyVersion,
  parsePythonVersion,
  pinnedRequirement,
  planGraphifyInstall,
  uvInstallEnvironment,
  venvExecutable,
} from "./graphifyDetection.ts";

const RUNTIME_DIR = "/state/graph-runtime";

describe("graphifyProbes", () => {
  it("tries PATH, the managed venv, then the python module", () => {
    const probes = graphifyProbes({
      graphifyPath: "",
      graphRuntimeDir: RUNTIME_DIR,
      uvToolBinDir: null,
      platform: "darwin",
    });
    expect(probes.map((probe) => probe.kind)).toEqual(["path", "managed", "module"]);
  });

  it("puts an explicit path first so a user override always wins", () => {
    const probes = graphifyProbes({
      graphifyPath: "  /opt/bin/graphify  ",
      graphRuntimeDir: RUNTIME_DIR,
      uvToolBinDir: null,
      platform: "darwin",
    });
    expect(probes[0]).toEqual({ kind: "setting", command: "/opt/bin/graphify", args: [] });
  });

  it("never probes `uv tool run`, which would install on miss", () => {
    const probes = graphifyProbes({
      graphifyPath: "",
      graphRuntimeDir: RUNTIME_DIR,
      uvToolBinDir: "/home/me/.local/bin",
      platform: "linux",
    });
    expect(probes.some((probe) => probe.command === "uv")).toBe(false);
  });

  // uv installs console scripts into its bin directory and only *warns* that
  // the directory is not on PATH, so a working `uv tool install graphifyy` is
  // invisible to the PATH probe. Without this candidate T3 reports "graphify is
  // not installed" at a machine that has it.
  it("looks in uv's bin directory, which uv does not put on PATH", () => {
    const probes = graphifyProbes({
      graphifyPath: "",
      graphRuntimeDir: RUNTIME_DIR,
      uvToolBinDir: "/home/me/.local/bin",
      platform: "linux",
    });
    expect(probes.map((probe) => probe.kind)).toEqual(["path", "managed", "uv-tool", "module"]);
    expect(probes.find((probe) => probe.kind === "uv-tool")?.command).toBe(
      "/home/me/.local/bin/graphify",
    );
  });

  it("omits the uv candidate when uv is not installed", () => {
    const probes = graphifyProbes({
      graphifyPath: "",
      graphRuntimeDir: RUNTIME_DIR,
      uvToolBinDir: "   ",
      platform: "linux",
    });
    expect(probes.some((probe) => probe.kind === "uv-tool")).toBe(false);
  });

  it("uses the Windows venv layout on win32", () => {
    const probes = graphifyProbes({
      graphifyPath: "",
      graphRuntimeDir: "C:\\state\\graph-runtime",
      uvToolBinDir: "C:\\Users\\me\\.local\\bin",
      platform: "win32",
    });
    expect(probes.find((probe) => probe.kind === "managed")?.command).toBe(
      "C:\\state\\graph-runtime\\Scripts\\graphify.exe",
    );
    expect(probes.find((probe) => probe.kind === "uv-tool")?.command).toBe(
      "C:\\Users\\me\\.local\\bin\\graphify.exe",
    );
  });
});

describe("uvInstallEnvironment", () => {
  // The regression that shipped: `uv tool install` wrote the script to
  // ~/.local/bin while the managed probe looked under graphRuntimeDir, so the
  // install succeeded and detection immediately said it was missing. These two
  // paths have to be the same string or that gap reopens.
  it.each(["darwin", "linux", "win32"] as const)(
    "installs to exactly the path the managed probe reads on %s",
    (platform) => {
      const root = platform === "win32" ? "C:\\state\\graph-runtime" : RUNTIME_DIR;
      const separator = platform === "win32" ? "\\" : "/";
      const binDir = uvInstallEnvironment(root, platform).UV_TOOL_BIN_DIR;
      const probed = venvExecutable(root, "graphify", platform);
      expect(probed.startsWith(`${binDir}${separator}`)).toBe(true);
      expect(
        graphifyProbes({
          graphifyPath: "",
          graphRuntimeDir: root,
          uvToolBinDir: null,
          platform,
        }).find((probe) => probe.kind === "managed")?.command,
      ).toBe(probed);
    },
  );

  it("keeps uv's tool environments inside T3's directory too", () => {
    // Otherwise the wheels live in ~/.local/share/uv/tools and survive deleting
    // T3's data directory, leaving the symlink T3 owns pointing at nothing.
    expect(uvInstallEnvironment(RUNTIME_DIR, "linux").UV_TOOL_DIR).toBe(
      "/state/graph-runtime/uv-tools",
    );
  });
});

describe("venvExecutable", () => {
  it("uses bin/ without a suffix on posix", () => {
    expect(venvExecutable(RUNTIME_DIR, "python", "linux")).toBe("/state/graph-runtime/bin/python");
  });
});

describe("parseGraphifyVersion", () => {
  it("reads a version out of whatever the tool printed", () => {
    expect(parseGraphifyVersion("graphify 0.9.27")).toBe("0.9.27");
    expect(parseGraphifyVersion("graphify, version 1.0")).toBe("1.0");
  });

  it("returns null when there is no version, so the probe counts as failed", () => {
    expect(parseGraphifyVersion("command not found: graphify")).toBeNull();
    expect(parseGraphifyVersion("")).toBeNull();
  });
});

describe("parsePythonVersion", () => {
  it("parses the standard banner from either stream", () => {
    expect(parsePythonVersion("Python 3.12.1")).toEqual({ major: 3, minor: 12 });
    expect(parsePythonVersion("\nPython 3.9.6\n")).toEqual({ major: 3, minor: 9 });
  });

  it("returns null for unrelated output", () => {
    expect(parsePythonVersion("zsh: command not found: python3")).toBeNull();
  });
});

describe("isSupportedPython", () => {
  it("requires 3.10 or newer", () => {
    expect(isSupportedPython({ major: 3, minor: 10 })).toBe(true);
    expect(isSupportedPython({ major: 3, minor: 12 })).toBe(true);
    expect(isSupportedPython({ major: 4, minor: 0 })).toBe(true);
    expect(isSupportedPython({ major: 3, minor: 9 })).toBe(false);
    expect(isSupportedPython({ major: 2, minor: 7 })).toBe(false);
  });
});

describe("planGraphifyInstall", () => {
  it("prefers uv when it is available, regardless of Python", () => {
    expect(planGraphifyInstall({ uvAvailable: true, python: null })).toEqual({ kind: "uv" });
  });

  it("falls back to a venv built from the found interpreter", () => {
    expect(
      planGraphifyInstall({
        uvAvailable: false,
        python: { command: "python3", version: { major: 3, minor: 11 } },
      }),
    ).toEqual({ kind: "venv", python: "python3" });
  });

  it("refuses rather than installing a language runtime when Python is absent", () => {
    const plan = planGraphifyInstall({ uvAvailable: false, python: null });
    expect(plan.kind).toBe("impossible");
    expect(plan.kind === "impossible" && plan.detail).toContain("uv");
  });

  it("names the version it found when Python is too old", () => {
    const plan = planGraphifyInstall({
      uvAvailable: false,
      python: { command: "python3", version: { major: 3, minor: 8 } },
    });
    expect(plan.kind).toBe("impossible");
    expect(plan.kind === "impossible" && plan.detail).toContain("3.8");
  });
});

describe("pinnedRequirement", () => {
  it("pins the distribution exactly", () => {
    expect(pinnedRequirement()).toBe(`graphifyy==${GRAPHIFY_PINNED_VERSION}`);
  });
});
