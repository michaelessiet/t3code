import { afterEach, describe, expect, it } from "@effect/vitest";

import {
  clearGraphAvailability,
  normalizeWorkspaceRoot,
  readGraphAvailability,
  reconcileGraphAvailability,
  recordGraphAvailability,
} from "./GraphAvailability.ts";

const graph = (overrides?: { branch?: string | null; builtAt?: number }) => ({
  branch: overrides?.branch === undefined ? "main" : overrides.branch,
  nodeCount: 100,
  edgeCount: 250,
  builtAt: overrides?.builtAt ?? 1_000,
});

afterEach(() => {
  clearGraphAvailability();
});

describe("normalizeWorkspaceRoot", () => {
  it("ignores trailing separators, which a cwd and a stored root disagree on", () => {
    expect(normalizeWorkspaceRoot("/repo/app/")).toBe("/repo/app");
    expect(normalizeWorkspaceRoot("C:\\repo\\app\\")).toBe("C:\\repo\\app");
    expect(normalizeWorkspaceRoot("  /repo/app  ")).toBe("/repo/app");
  });

  // Stripping every separator off "/" would key the root directory as "",
  // which `recordGraphAvailability` drops — so a checkout at the filesystem
  // root would silently never get a note.
  it("does not reduce a root-only path to nothing", () => {
    expect(normalizeWorkspaceRoot("/")).toBe("/");
    recordGraphAvailability("/", graph());
    expect(readGraphAvailability("/")).toBeDefined();
  });
});

describe("recordGraphAvailability", () => {
  it("round-trips through a cwd that differs only in trailing separator", () => {
    recordGraphAvailability("/repo/app", graph());
    expect(readGraphAvailability("/repo/app/")?.nodeCount).toBe(100);
  });

  it("reports nothing for a checkout that has no graph", () => {
    recordGraphAvailability("/repo/app", graph());
    expect(readGraphAvailability("/repo/other")).toBeUndefined();
  });

  // Entries are keyed by `git rev-parse --show-toplevel`, so a thread running
  // in a subdirectory has to walk up to find the graph that covers it.
  it("finds an ancestor's graph from a subdirectory", () => {
    recordGraphAvailability("/repo/app", graph());
    expect(readGraphAvailability("/repo/app/packages/server/src")?.nodeCount).toBe(100);
  });

  it("does not let a string prefix stand in for a path segment", () => {
    recordGraphAvailability("/repo/app", graph());
    expect(readGraphAvailability("/repo/app-2")).toBeUndefined();
    expect(readGraphAvailability("/repo/app-2/src")).toBeUndefined();
  });

  it("stops at the filesystem root instead of looping", () => {
    recordGraphAvailability("/repo/app", graph());
    expect(readGraphAvailability("/elsewhere/deep/nested")).toBeUndefined();
    expect(readGraphAvailability("relative/path")).toBeUndefined();
  });

  it("walks up windows paths too", () => {
    recordGraphAvailability("C:\\repo\\app", graph());
    expect(readGraphAvailability("C:\\repo\\app\\src\\server")?.nodeCount).toBe(100);
    expect(readGraphAvailability("C:\\repo\\other")).toBeUndefined();
  });

  it("prefers the nearest ancestor when a repository sits inside another", () => {
    recordGraphAvailability("/repo", graph({ branch: "outer" }));
    recordGraphAvailability("/repo/vendor/inner", graph({ branch: "inner" }));
    expect(readGraphAvailability("/repo/vendor/inner/src")?.branch).toBe("inner");
    expect(readGraphAvailability("/repo/src")?.branch).toBe("outer");
  });

  // One checkout accumulates an entry per branch it has been built on, and the
  // reader cannot tell which is current, so the newest build is what it gets.
  it("keeps the most recent build when a checkout has several branches", () => {
    recordGraphAvailability("/repo/app", graph({ branch: "main", builtAt: 1_000 }));
    recordGraphAvailability("/repo/app", graph({ branch: "feat/x", builtAt: 2_000 }));
    expect(readGraphAvailability("/repo/app")?.branch).toBe("feat/x");
  });

  it("does not let an older build displace a newer one", () => {
    recordGraphAvailability("/repo/app", graph({ branch: "feat/x", builtAt: 2_000 }));
    recordGraphAvailability("/repo/app", graph({ branch: "main", builtAt: 1_000 }));
    expect(readGraphAvailability("/repo/app")?.branch).toBe("feat/x");
  });
});

describe("reconcileGraphAvailability", () => {
  // This is how eviction propagates: the sweep deletes a directory and re-lists
  // the store in the same pass, so a dropped entry stops being advertised
  // without eviction having to notify anyone.
  it("forgets an entry that is absent from a later full listing", () => {
    reconcileGraphAvailability([
      { workspaceRoot: "/repo/app", ...graph() },
      { workspaceRoot: "/repo/other", ...graph() },
    ]);
    reconcileGraphAvailability([{ workspaceRoot: "/repo/app", ...graph() }]);
    expect(readGraphAvailability("/repo/app")).toBeDefined();
    expect(readGraphAvailability("/repo/other")).toBeUndefined();
  });

  it("empties the map when the store has become empty", () => {
    recordGraphAvailability("/repo/app", graph());
    reconcileGraphAvailability([]);
    expect(readGraphAvailability("/repo/app")).toBeUndefined();
  });

  it("still resolves newest-wins within a single listing", () => {
    reconcileGraphAvailability([
      { workspaceRoot: "/repo/app", ...graph({ branch: "main", builtAt: 1_000 }) },
      { workspaceRoot: "/repo/app", ...graph({ branch: "feat/x", builtAt: 2_000 }) },
    ]);
    expect(readGraphAvailability("/repo/app")?.branch).toBe("feat/x");
  });
});
