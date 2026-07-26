// @effect-diagnostics nodeBuiltinImport:off
import * as NodeChildProcess from "node:child_process";
import * as NodeFSP from "node:fs/promises";
import * as NodeOS from "node:os";
import * as NodePath from "node:path";
import * as NodeUtil from "node:util";

import { afterAll, beforeAll, expect, it } from "@effect/vitest";
import * as Effect from "effect/Effect";

import * as WorkspaceIgnoredEntries from "./WorkspaceIgnoredEntries.ts";

const execFile = NodeUtil.promisify(NodeChildProcess.execFile);

let gitFixture: string;
let plainFixture: string;

beforeAll(async () => {
  gitFixture = await NodeFSP.mkdtemp(NodePath.join(NodeOS.tmpdir(), "t3-ignored-entries-git-"));
  plainFixture = await NodeFSP.mkdtemp(NodePath.join(NodeOS.tmpdir(), "t3-ignored-entries-plain-"));

  await execFile("git", ["-C", gitFixture, "init", "--quiet"]);
  await NodeFSP.writeFile(NodePath.join(gitFixture, ".gitignore"), ".env\ndist/\nnode_modules/\n");
  await NodeFSP.writeFile(NodePath.join(gitFixture, "tracked.ts"), "export {};\n");
  await NodeFSP.writeFile(NodePath.join(gitFixture, ".env"), "SECRET=1\n");
  await NodeFSP.mkdir(NodePath.join(gitFixture, "dist", "sub"), { recursive: true });
  await NodeFSP.writeFile(NodePath.join(gitFixture, "dist", "a.js"), "1\n");
  await NodeFSP.writeFile(NodePath.join(gitFixture, "dist", "sub", "b.js"), "2\n");
  await NodeFSP.mkdir(NodePath.join(gitFixture, "node_modules", "pkg"), { recursive: true });
  await NodeFSP.writeFile(NodePath.join(gitFixture, "node_modules", "pkg", "index.js"), "3\n");
});

afterAll(async () => {
  await NodeFSP.rm(gitFixture, { recursive: true, force: true });
  await NodeFSP.rm(plainFixture, { recursive: true, force: true });
});

it.effect("enumerates gitignored files and expands non-vendor ignored directories", () =>
  Effect.gen(function* () {
    const supplement = yield* WorkspaceIgnoredEntries.listIgnoredEntries(gitFixture);

    const byPath = new Map(supplement.entries.map((entry) => [entry.path, entry]));
    expect(byPath.get(".env")).toEqual({ path: ".env", kind: "file", ignored: true });
    expect(byPath.get("dist")).toEqual({ path: "dist", kind: "directory", ignored: true });
    expect(byPath.get("dist/a.js")).toEqual({ path: "dist/a.js", kind: "file", ignored: true });
    expect(byPath.get("dist/sub")).toEqual({ path: "dist/sub", kind: "directory", ignored: true });
    expect(byPath.get("dist/sub/b.js")).toEqual({
      path: "dist/sub/b.js",
      kind: "file",
      ignored: true,
    });
    expect(supplement.truncated).toBe(false);
    // Tracked and untracked-but-not-ignored files never enter the supplement.
    expect(byPath.has("tracked.ts")).toBe(false);
    expect(byPath.has(".gitignore")).toBe(false);
  }),
);

it.effect("lists dependency stores as collapsed directories without expanding them", () =>
  Effect.gen(function* () {
    const supplement = yield* WorkspaceIgnoredEntries.listIgnoredEntries(gitFixture);

    const paths = new Set(supplement.entries.map((entry) => entry.path));
    expect(paths.has("node_modules")).toBe(true);
    expect(paths.has("node_modules/pkg")).toBe(false);
    expect(paths.has("node_modules/pkg/index.js")).toBe(false);
  }),
);

it.effect("degrades to an empty supplement outside a git repository", () =>
  Effect.gen(function* () {
    const supplement = yield* WorkspaceIgnoredEntries.listIgnoredEntries(plainFixture);
    expect(supplement).toEqual(WorkspaceIgnoredEntries.emptyIgnoredEntriesSupplement);
  }),
);

it.effect("degrades to an empty supplement when the workspace does not exist", () =>
  Effect.gen(function* () {
    const supplement = yield* WorkspaceIgnoredEntries.listIgnoredEntries(
      NodePath.join(plainFixture, "missing"),
    );
    expect(supplement).toEqual(WorkspaceIgnoredEntries.emptyIgnoredEntriesSupplement);
  }),
);
