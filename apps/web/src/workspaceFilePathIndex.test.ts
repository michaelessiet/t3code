import { describe, expect, it } from "vite-plus/test";

import { resolveWorkspaceFilePath, workspaceFilePathSet } from "./workspaceFilePathIndex";

const CWD = "/Users/julius/project";

const WORKSPACE_FILES = new Set([
  "apps/web/src/markdown-links.ts",
  "package.json",
  "build/generated.js",
]);

describe("workspaceFilePathSet", () => {
  it("keeps files and drops directories", () => {
    const paths = workspaceFilePathSet({
      entries: [
        { path: "apps", kind: "directory" },
        { path: "apps/web/src/markdown-links.ts", kind: "file" },
      ],
      truncated: false,
    });

    expect([...paths]).toEqual(["apps/web/src/markdown-links.ts"]);
  });

  it("keeps gitignored files, which exist on disk and can be opened", () => {
    const paths = workspaceFilePathSet({
      entries: [{ path: "build/generated.js", kind: "file", ignored: true }],
      truncated: false,
    });

    expect(paths.has("build/generated.js")).toBe(true);
  });

  it("reuses the set built for the same entries result", () => {
    const result = {
      entries: [{ path: "package.json", kind: "file" }],
      truncated: false,
    } as const;

    expect(workspaceFilePathSet(result)).toBe(workspaceFilePathSet(result));
  });
});

describe("resolveWorkspaceFilePath", () => {
  it("resolves a workspace-relative path to an absolute target", () => {
    expect(resolveWorkspaceFilePath("apps/web/src/markdown-links.ts", WORKSPACE_FILES, CWD)).toBe(
      "/Users/julius/project/apps/web/src/markdown-links.ts",
    );
  });

  it("preserves a line suffix", () => {
    expect(
      resolveWorkspaceFilePath("apps/web/src/markdown-links.ts:42", WORKSPACE_FILES, CWD),
    ).toBe("/Users/julius/project/apps/web/src/markdown-links.ts:42");
  });

  it("preserves a line and column suffix", () => {
    expect(
      resolveWorkspaceFilePath("apps/web/src/markdown-links.ts:42:7", WORKSPACE_FILES, CWD),
    ).toBe("/Users/julius/project/apps/web/src/markdown-links.ts:42:7");
  });

  it("strips a leading ./ before checking membership", () => {
    expect(resolveWorkspaceFilePath("./package.json", WORKSPACE_FILES, CWD)).toBe(
      "/Users/julius/project/package.json",
    );
  });

  it("resolves an absolute path that lives inside the workspace", () => {
    expect(
      resolveWorkspaceFilePath("/Users/julius/project/package.json", WORKSPACE_FILES, CWD),
    ).toBe("/Users/julius/project/package.json");
  });

  it("resolves a home-relative path that lands inside the workspace", () => {
    expect(resolveWorkspaceFilePath("~/project/package.json", WORKSPACE_FILES, CWD)).toBe(
      "/Users/julius/project/package.json",
    );
  });

  it("rejects a path the workspace does not contain", () => {
    expect(resolveWorkspaceFilePath("apps/web/src/imaginary.ts", WORKSPACE_FILES, CWD)).toBeNull();
  });

  it("rejects an absolute path outside the workspace", () => {
    expect(resolveWorkspaceFilePath("/etc/hosts", WORKSPACE_FILES, CWD)).toBeNull();
  });

  it("rejects a parent-relative path, which the entry list cannot confirm", () => {
    expect(resolveWorkspaceFilePath("../sibling/package.json", WORKSPACE_FILES, CWD)).toBeNull();
  });

  it("rejects a directory that is not in the file set", () => {
    expect(resolveWorkspaceFilePath("apps/web/src", WORKSPACE_FILES, CWD)).toBeNull();
  });

  it("rejects an empty candidate", () => {
    expect(resolveWorkspaceFilePath("", WORKSPACE_FILES, CWD)).toBeNull();
  });

  it("resolves a gitignored file that exists on disk", () => {
    expect(resolveWorkspaceFilePath("build/generated.js", WORKSPACE_FILES, CWD)).toBe(
      "/Users/julius/project/build/generated.js",
    );
  });
});
