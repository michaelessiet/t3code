import type { RepositoryIdentity, ResolvedWorkspaceRoot } from "@t3tools/contracts";
import { describe, expect, it } from "vite-plus/test";

import {
  composeThreadRoots,
  normalizeRootPathForComparison,
  relativizeAgainstRoots,
  rootLabels,
  type ThreadRoot,
} from "./threadRoots";

const REPO_IDENTITY: RepositoryIdentity = {
  canonicalKey: "github.com/acme/tools",
  locator: {
    source: "git-remote",
    remoteName: "origin",
    remoteUrl: "https://github.com/acme/tools.git",
  },
};

function okRoot(path: string, overrides?: Partial<ResolvedWorkspaceRoot>): ResolvedWorkspaceRoot {
  return {
    ref: { kind: "path", path },
    path,
    status: "ok",
    ...overrides,
  };
}

function makeThreadRoot(path: string, overrides?: Partial<ThreadRoot>): ThreadRoot {
  return {
    path,
    label: path,
    isPrimary: false,
    source: "thread",
    ...overrides,
  };
}

describe("normalizeRootPathForComparison", () => {
  it("lowercases and trims surrounding whitespace", () => {
    expect(normalizeRootPathForComparison("  /Users/Alice/Repo ")).toBe("/users/alice/repo");
  });

  it("normalizes backslashes to forward slashes", () => {
    expect(normalizeRootPathForComparison("C:\\Users\\Alice\\repo")).toBe("c:/users/alice/repo");
  });

  it("strips trailing separators, including repeated and mixed ones", () => {
    expect(normalizeRootPathForComparison("/users/alice/repo/")).toBe("/users/alice/repo");
    expect(normalizeRootPathForComparison("/users/alice/repo///")).toBe("/users/alice/repo");
    expect(normalizeRootPathForComparison("C:\\repo\\")).toBe("c:/repo");
  });

  it("keeps the filesystem root instead of trimming it to an empty string", () => {
    expect(normalizeRootPathForComparison("/")).toBe("/");
  });
});

describe("rootLabels", () => {
  it("uses plain basenames when they are unique", () => {
    expect(rootLabels(["/Users/alice/tools", "/Users/alice/webapp"])).toEqual(["tools", "webapp"]);
  });

  it("ignores trailing separators when deriving basenames", () => {
    expect(rootLabels(["/Users/alice/tools/"])).toEqual(["tools"]);
  });

  it("derives basenames from windows-style paths", () => {
    expect(rootLabels(["C:\\Users\\alice\\tools"])).toEqual(["tools"]);
  });

  it("disambiguates every colliding basename with its parent directory", () => {
    expect(rootLabels(["/Users/alice/work/repo", "/Users/alice/play/repo"])).toEqual([
      "work/repo",
      "play/repo",
    ]);
  });

  it("detects collisions case-insensitively but preserves original casing", () => {
    expect(rootLabels(["/Users/alice/work/Repo", "/Users/alice/play/repo"])).toEqual([
      "work/Repo",
      "play/repo",
    ]);
  });

  it("only qualifies one parent level, even if the result still collides", () => {
    expect(rootLabels(["/a/shared/repo", "/b/shared/repo"])).toEqual([
      "shared/repo",
      "shared/repo",
    ]);
  });

  it("falls back to the lone segment when a colliding path has no parent", () => {
    expect(rootLabels(["repo", "/parent/repo"])).toEqual(["repo", "parent/repo"]);
  });
});

describe("composeThreadRoots", () => {
  it("returns the empty result when the primary path is null", () => {
    const result = composeThreadRoots({ primaryPath: null });
    expect(result.primary).toBeNull();
    expect(result.additional).toEqual([]);
    expect(result.all).toEqual([]);
  });

  it("returns the empty result when the primary path is empty or whitespace", () => {
    expect(composeThreadRoots({ primaryPath: "" }).primary).toBeNull();
    expect(composeThreadRoots({ primaryPath: "   " }).all).toEqual([]);
  });

  it("composes a primary-only result", () => {
    const result = composeThreadRoots({ primaryPath: "/Users/alice/webapp" });
    expect(result.primary).toEqual({
      path: "/Users/alice/webapp",
      label: "webapp",
      isPrimary: true,
      source: "primary",
    });
    expect(result.additional).toEqual([]);
    expect(result.all).toEqual([result.primary]);
  });

  it("orders roots primary first, then project roots, then thread roots", () => {
    const result = composeThreadRoots({
      primaryPath: "/repos/webapp",
      projectRoots: [okRoot("/repos/project-a"), okRoot("/repos/project-b")],
      threadRoots: [okRoot("/repos/thread-a")],
    });
    expect(result.all.map((root) => root.path)).toEqual([
      "/repos/webapp",
      "/repos/project-a",
      "/repos/project-b",
      "/repos/thread-a",
    ]);
    expect(result.all.map((root) => root.source)).toEqual([
      "primary",
      "project",
      "project",
      "thread",
    ]);
    expect(result.all.map((root) => root.isPrimary)).toEqual([true, false, false, false]);
    expect(result.additional).toEqual(result.all.slice(1));
  });

  it("dedups attached roots against the primary despite trailing slashes and case", () => {
    const result = composeThreadRoots({
      primaryPath: "/Users/Alice/webapp",
      projectRoots: [okRoot("/users/alice/webapp/")],
      threadRoots: [okRoot("/USERS/ALICE/WEBAPP")],
    });
    expect(result.all).toHaveLength(1);
    expect(result.primary?.path).toBe("/Users/Alice/webapp");
  });

  it("dedups thread roots against project roots, keeping the project-sourced entry", () => {
    const result = composeThreadRoots({
      primaryPath: "/repos/webapp",
      projectRoots: [okRoot("/repos/tools")],
      threadRoots: [okRoot("/repos/tools/"), okRoot("/repos/extra")],
    });
    expect(result.all.map((root) => root.path)).toEqual([
      "/repos/webapp",
      "/repos/tools",
      "/repos/extra",
    ]);
    expect(result.all[1]?.source).toBe("project");
  });

  it("drops roots that are not status ok or that have no resolved path", () => {
    const result = composeThreadRoots({
      primaryPath: "/repos/webapp",
      projectRoots: [
        okRoot("/repos/dangling", { status: "missing-project" }),
        { ref: { kind: "path", path: "/repos/no-path" }, status: "ok" },
      ],
      threadRoots: [okRoot("/repos/kept")],
    });
    expect(result.all.map((root) => root.path)).toEqual(["/repos/webapp", "/repos/kept"]);
  });

  it("assigns basename labels and parent-qualifies collisions across sources", () => {
    const result = composeThreadRoots({
      primaryPath: "/Users/alice/work/repo",
      projectRoots: [okRoot("/Users/alice/play/repo")],
      threadRoots: [okRoot("/Users/alice/tools")],
    });
    expect(result.all.map((root) => root.label)).toEqual(["work/repo", "play/repo", "tools"]);
  });

  it("carries the repository identity of attached roots through", () => {
    const result = composeThreadRoots({
      primaryPath: "/repos/webapp",
      threadRoots: [okRoot("/repos/tools", { repositoryIdentity: REPO_IDENTITY })],
    });
    expect(result.additional[0]?.repositoryIdentity).toEqual(REPO_IDENTITY);
    expect(result.primary?.repositoryIdentity).toBeUndefined();
  });
});

describe("relativizeAgainstRoots", () => {
  const primary = makeThreadRoot("/repos/webapp", { isPrimary: true, source: "primary" });
  const attached = makeThreadRoot("/repos/tools");

  it("relativizes a path inside the primary root", () => {
    expect(relativizeAgainstRoots("/repos/webapp/src/main.ts", [primary, attached])).toEqual({
      root: primary,
      relativePath: "src/main.ts",
    });
  });

  it("relativizes a path inside an attached root", () => {
    expect(relativizeAgainstRoots("/repos/tools/bin/run.sh", [primary, attached])).toEqual({
      root: attached,
      relativePath: "bin/run.sh",
    });
  });

  it("lets the primary win when the path is inside both roots (first match by order)", () => {
    const nested = makeThreadRoot("/repos/webapp/packages/lib");
    const result = relativizeAgainstRoots("/repos/webapp/packages/lib/index.ts", [primary, nested]);
    expect(result?.root).toBe(primary);
    expect(result?.relativePath).toBe("packages/lib/index.ts");
  });

  it("returns null for a path outside every root", () => {
    expect(relativizeAgainstRoots("/elsewhere/file.ts", [primary, attached])).toBeNull();
  });

  it("does not treat a sibling directory sharing a prefix as inside a root", () => {
    expect(relativizeAgainstRoots("/repos/webapp-old/file.ts", [primary, attached])).toBeNull();
  });

  it("maps the exact root path to an empty relative path", () => {
    expect(relativizeAgainstRoots("/repos/webapp", [primary, attached])).toEqual({
      root: primary,
      relativePath: "",
    });
  });

  it("matches case-insensitively while preserving the input casing in the result", () => {
    expect(relativizeAgainstRoots("/Repos/WebApp/Src/App.TSX", [primary, attached])).toEqual({
      root: primary,
      relativePath: "Src/App.TSX",
    });
  });

  it("normalizes backslash separators in the queried path and root", () => {
    const windowsRoot = makeThreadRoot("C:\\repos\\webapp", {
      isPrimary: true,
      source: "primary",
    });
    expect(relativizeAgainstRoots("C:\\repos\\webapp\\src\\main.ts", [windowsRoot])).toEqual({
      root: windowsRoot,
      relativePath: "src/main.ts",
    });
  });

  it("trims surrounding whitespace from the queried path", () => {
    expect(relativizeAgainstRoots("  /repos/webapp/src/main.ts  ", [primary])).toEqual({
      root: primary,
      relativePath: "src/main.ts",
    });
  });

  it("returns null when there are no roots", () => {
    expect(relativizeAgainstRoots("/repos/webapp/src/main.ts", [])).toBeNull();
  });
});
