import { describe, expect, it } from "@effect/vitest";

import { graphifyArguments, graphifyEnv } from "./GraphifyCli.ts";

const invocation = {
  workspaceRoot: "/repos/t3code",
  outDir: "/t3/caches/graph/p/main-0a1b2c3d/graphify-out",
  mode: "structural" as const,
  force: false,
  incremental: false,
};

describe("graphifyArguments", () => {
  it("uses --code-only for a structural build, which needs no API key", () => {
    expect(graphifyArguments(invocation)).toEqual(["extract", "/repos/t3code", "--code-only"]);
  });

  it("names the keyless backend for a semantic build instead of --code-only", () => {
    // Without a `--backend`, graphify exits 1 with "no LLM API key found"
    // rather than choosing one, so the flag is what makes semantic mode run
    // at all on a machine with no provider key set.
    expect(graphifyArguments({ ...invocation, mode: "semantic" })).toEqual([
      "extract",
      "/repos/t3code",
      "--backend",
      "claude-cli",
    ]);
  });

  it("passes --force through", () => {
    expect(graphifyArguments({ ...invocation, force: true })).toEqual([
      "extract",
      "/repos/t3code",
      "--code-only",
      "--force",
    ]);
  });

  it("uses `update` for an incremental refresh", () => {
    expect(graphifyArguments({ ...invocation, incremental: true })).toEqual([
      "update",
      "/repos/t3code",
    ]);
  });

  it("never passes --code-only to `update`, which rejects unknown options", () => {
    // `cli.py:1794` exits 2 on any unrecognised flag, so a stray --code-only
    // here would turn every incremental rebuild into a hard failure.
    const args = graphifyArguments({ ...invocation, incremental: true, force: true });
    expect(args).toEqual(["update", "/repos/t3code", "--force"]);
    expect(args).not.toContain("--code-only");
  });
});

describe("graphifyEnv", () => {
  it("points GRAPHIFY_OUT at the store", () => {
    expect(graphifyEnv({ outDir: invocation.outDir }).GRAPHIFY_OUT).toBe(invocation.outDir);
  });

  // The guarantee the whole store design rests on. `cli.py:2640` computes
  // `target / $GRAPHIFY_OUT`, and pathlib only discards `target` when the
  // right-hand side is absolute — a relative value writes into the user's repo.
  it.each(["graphify-out", "./graphify-out", "../shared/graphify-out", ""])(
    "refuses the relative path %j rather than writing into the repo",
    (outDir) => {
      expect(() => graphifyEnv({ outDir })).toThrow(/absolute/);
    },
  );

  it("accepts a Windows absolute path", () => {
    const outDir = "C:\\Users\\me\\t3\\graph\\main-0a1b2c3d\\graphify-out";
    expect(graphifyEnv({ outDir }).GRAPHIFY_OUT).toBe(outDir);
  });

  // `detect.py` excludes its own output by basename, so a renamed leaf would
  // both index the store and blank every same-named directory in the repo.
  it("refuses a leaf that is not named graphify-out", () => {
    expect(() => graphifyEnv({ outDir: "/t3/caches/graph/p/main-0a1b2c3d/out" })).toThrow(
      /graphify-out/,
    );
  });

  it("disables graphify's own backups, which T3's store would have to reclaim", () => {
    expect(graphifyEnv({ outDir: invocation.outDir }).GRAPHIFY_NO_BACKUP).toBe("1");
  });

  it("returns only the keys it means to layer onto the inherited environment", () => {
    expect(Object.keys(graphifyEnv({ outDir: invocation.outDir })).sort()).toEqual([
      "GRAPHIFY_NO_BACKUP",
      "GRAPHIFY_NO_TIPS",
      "GRAPHIFY_OUT",
    ]);
  });
});
