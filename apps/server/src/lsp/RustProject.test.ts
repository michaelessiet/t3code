// @effect-diagnostics nodeBuiltinImport:off
import * as NodeFSP from "node:fs/promises";
import * as NodeOS from "node:os";
import * as NodePath from "node:path";

import { afterEach, beforeEach, describe, expect, it } from "vite-plus/test";

import { discoverCargoManifests, rustAnalyzerInitializationOptions } from "./RustProject.ts";

let root = "";

/** Create `<root>/<relativeDir>/Cargo.toml`, plus any missing parents. */
async function writeCrate(relativeDir: string): Promise<string> {
  const directory = NodePath.join(root, relativeDir);
  await NodeFSP.mkdir(directory, { recursive: true });
  const manifest = NodePath.join(directory, "Cargo.toml");
  await NodeFSP.writeFile(manifest, '[package]\nname = "probe"\n');
  return manifest;
}

beforeEach(async () => {
  root = await NodeFSP.mkdtemp(NodePath.join(NodeOS.tmpdir(), "t3-rust-project-"));
});

afterEach(async () => {
  await NodeFSP.rm(root, { recursive: true, force: true });
});

describe("discoverCargoManifests", () => {
  it("finds crates nested under a root that is not itself a Cargo project", async () => {
    const tauri = await writeCrate("apps/desktop-tauri/src-tauri");
    const probe = await writeCrate("experiments/preview-probe");

    expect(new Set(await discoverCargoManifests(root))).toEqual(new Set([tauri, probe]));
  });

  it("returns nothing when the root is itself a Cargo project", async () => {
    // rust-analyzer discovers this natively, and native discovery expands
    // [workspace] members more faithfully than an explicit list.
    await writeCrate(".");
    await writeCrate("crates/inner");

    expect(await discoverCargoManifests(root)).toEqual([]);
  });

  it("returns nothing for a workspace with no crates", async () => {
    await NodeFSP.mkdir(NodePath.join(root, "src"), { recursive: true });

    expect(await discoverCargoManifests(root)).toEqual([]);
  });

  it("stops at the top-most manifest instead of also linking workspace members", async () => {
    const workspace = await writeCrate("rust");
    await writeCrate("rust/crates/core");
    await writeCrate("rust/crates/cli");

    expect(await discoverCargoManifests(root)).toEqual([workspace]);
  });

  it("skips build output and dependency directories", async () => {
    await writeCrate("target/debug/build/vendored");
    await writeCrate("node_modules/some-pkg/native");
    const real = await writeCrate("crates/app");

    expect(await discoverCargoManifests(root)).toEqual([real]);
  });

  it("honours the depth limit", async () => {
    const deep = await writeCrate("a/b/c/crate");

    expect(await discoverCargoManifests(root, { maxDepth: 5 })).toEqual([deep]);
    expect(await discoverCargoManifests(root, { maxDepth: 2 })).toEqual([]);
  });

  it("honours the manifest budget", async () => {
    await writeCrate("crates/one");
    await writeCrate("crates/two");
    await writeCrate("crates/three");

    expect(await discoverCargoManifests(root, { maxManifests: 2 })).toHaveLength(2);
  });

  it("tolerates an unreadable workspace root", async () => {
    expect(await discoverCargoManifests(NodePath.join(root, "missing"))).toEqual([]);
  });
});

describe("rustAnalyzerInitializationOptions", () => {
  it("carries the discovered manifests as linkedProjects", async () => {
    const manifest = await writeCrate("apps/shell/src-tauri");

    expect(await rustAnalyzerInitializationOptions(root)).toEqual({
      linkedProjects: [manifest],
    });
  });

  it("is undefined when the defaults already suffice", async () => {
    await writeCrate(".");

    expect(await rustAnalyzerInitializationOptions(root)).toBeUndefined();
  });
});
