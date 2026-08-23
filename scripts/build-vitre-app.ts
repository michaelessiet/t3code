#!/usr/bin/env node
// Builds the packaged Vitre macOS app (apps/desktop-tauri).
//
// Stages the Node backend (server dist + hoisted production node_modules) and
// the injection scripts under src-tauri/staged/ (bundle.resources), provides a
// real Node sidecar under src-tauri/binaries/ (bundle.externalBin — bin.mjs
// uses node:sqlite, so a real Node >= 24 is required), then runs `tauri build`
// with the Clerk publishable key exported for the protocol.rs option_env!
// bake. See apps/desktop-tauri/README.md "Packaging design (M4, macOS)".
//
// Usage: node scripts/build-vitre-app.ts [--skip-build]
//   --skip-build      reuse existing server/shim/preview-runtime dist outputs
//   VITRE_NODE_SIDECAR overrides the Node binary copied as the sidecar
//                      (defaults to the Node running this script).

import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

import * as NodeServices from "@effect/platform-node/NodeServices";
import * as Effect from "effect/Effect";
import { parse as parseYamlString, stringify as stringifyYamlString } from "yaml";

import rootPackageJson from "../package.json" with { type: "json" };
import serverPackageJson from "../apps/server/package.json" with { type: "json" };

import {
  createStagePatchedDependencies,
  createStageWorkspaceConfig,
  isClerkJsPrunableDistChunkPath,
  pruneClaudeSdkPlatformPackages,
  STAGE_INSTALL_ARGS,
  stripStagedSourcemaps,
} from "./build-desktop-artifact.ts";
import { loadRepoEnv } from "./lib/public-config.ts";
import { resolveCatalogDependencies } from "./lib/resolve-catalog.ts";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const vitreDir = path.join(repoRoot, "apps/desktop-tauri");
const srcTauriDir = path.join(vitreDir, "src-tauri");
const stagedDir = path.join(srcTauriDir, "staged");
const backendDir = path.join(stagedDir, "backend");
const serverDistDir = path.join(repoRoot, "apps/server/dist");

const skipBuild = process.argv.includes("--skip-build");

function run(
  command: string,
  args: ReadonlyArray<string>,
  options: { cwd?: string; env?: NodeJS.ProcessEnv } = {},
): void {
  console.log(`[vitre-app] $ ${command} ${args.join(" ")}`);
  const result = spawnSync(command, [...args], {
    stdio: "inherit",
    cwd: options.cwd ?? repoRoot,
    env: options.env ?? process.env,
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(`[vitre-app] ${command} ${args.join(" ")} exited with status ${result.status}`);
  }
}

const runStageEffect = <A, E>(effect: Effect.Effect<A, E, never>): Promise<A> =>
  Effect.runPromise(effect);
const provideNode = <A, E, R>(effect: Effect.Effect<A, E, R>) =>
  Effect.provide(effect, NodeServices.layer) as Effect.Effect<A, E, never>;

function assertBuildInput(artifactPath: string, hint: string): void {
  if (!fs.existsSync(artifactPath)) {
    throw new Error(`[vitre-app] Missing build input ${artifactPath} — ${hint}`);
  }
}

// 1. Build inputs: server bundle (+ bundled web client) and injection scripts.
if (!skipBuild) {
  run("vp", ["run", "--filter", "t3", "build"]);
  run("node", ["scripts/build-shim.mjs"], { cwd: vitreDir });
  run("node", ["scripts/build-preview-runtime.mjs"], { cwd: vitreDir });
}
assertBuildInput(path.join(serverDistDir, "bin.mjs"), "run `vp run --filter t3 build`");
assertBuildInput(path.join(serverDistDir, "client/index.html"), "run `vp run --filter t3 build`");
const shimJs = path.join(vitreDir, "shim/dist/shim.js");
const previewRuntimeJs = path.join(vitreDir, "shim/dist/preview-runtime.js");
assertBuildInput(shimJs, "run `node scripts/build-shim.mjs` in apps/desktop-tauri");
assertBuildInput(
  previewRuntimeJs,
  "run `node scripts/build-preview-runtime.mjs` in apps/desktop-tauri",
);

// 2. Stage the backend with the Electron artifact's layout (node_modules above
// bin.mjs so Node resolution finds the externalized deps, client/ beside it).
console.log(`[vitre-app] Staging backend into ${backendDir}`);
fs.rmSync(stagedDir, { recursive: true, force: true });
fs.mkdirSync(path.join(backendDir, "apps/server"), { recursive: true });
fs.cpSync(serverDistDir, path.join(backendDir, "apps/server/dist"), { recursive: true });
await runStageEffect(
  provideNode(stripStagedSourcemaps(path.join(backendDir, "apps/server/dist"), "apps/server/dist")),
);

const workspaceConfig = parseYamlString(
  fs.readFileSync(path.join(repoRoot, "pnpm-workspace.yaml"), "utf8"),
) as {
  catalog?: Record<string, string>;
  overrides?: Record<string, string>;
  patchedDependencies?: Record<string, string>;
  allowBuilds?: Record<string, boolean>;
};
const catalog = workspaceConfig.catalog ?? {};
const arch = process.arch === "arm64" ? "arm64" : "x64";
const targetTriple = arch === "arm64" ? "aarch64-apple-darwin" : "x86_64-apple-darwin";

const stageDependencies = resolveCatalogDependencies(
  serverPackageJson.dependencies,
  catalog,
  "apps/server",
);
const stagePatchedDependencies = createStagePatchedDependencies(
  workspaceConfig.patchedDependencies ?? {},
  stageDependencies,
);
fs.writeFileSync(
  path.join(backendDir, "package.json"),
  `${JSON.stringify(
    {
      name: "vitre-backend",
      version: serverPackageJson.version,
      private: true,
      packageManager: rootPackageJson.packageManager,
      dependencies: stageDependencies,
    },
    null,
    2,
  )}\n`,
);
const stageWorkspaceConfig = createStageWorkspaceConfig({
  platform: "mac",
  arch,
  allowBuilds: workspaceConfig.allowBuilds,
  patchedDependencies: stagePatchedDependencies,
  overrides: resolveCatalogDependencies(
    workspaceConfig.overrides ?? {},
    catalog,
    "pnpm-workspace.yaml",
  ),
});
fs.writeFileSync(
  path.join(backendDir, "pnpm-workspace.yaml"),
  stringifyYamlString({
    ...stageWorkspaceConfig,
    // Hoisted so the staged tree has no pnpm symlink farm — the Tauri bundler
    // copies resources as plain files.
    nodeLinker: "hoisted",
  }),
);
if (Object.keys(stagePatchedDependencies).length > 0) {
  fs.cpSync(path.join(repoRoot, "patches"), path.join(backendDir, "patches"), { recursive: true });
}

run("vp", [...STAGE_INSTALL_ARGS], { cwd: backendDir });
await runStageEffect(
  provideNode(stripStagedSourcemaps(path.join(backendDir, "node_modules"), "node_modules")),
);
await runStageEffect(provideNode(pruneClaudeSdkPlatformPackages(backendDir)));

// Best-effort: the Vitre backend stages server deps only, so the Electron
// prune targets (@clerk/clerk-js variants, playwright-core) are usually
// absent; drop the Clerk variant chunks when a transitive dep pulls them in.
const stagedNodeModules = path.join(backendDir, "node_modules");
const clerkChunks = fs
  .readdirSync(stagedNodeModules, { recursive: true })
  .map(String)
  .filter(isClerkJsPrunableDistChunkPath);
for (const chunk of clerkChunks) {
  fs.rmSync(path.join(stagedNodeModules, chunk), { force: true });
}
if (clerkChunks.length > 0) {
  console.log(`[vitre-app] Pruned ${clerkChunks.length} @clerk/clerk-js variant chunks.`);
}

// 3. Injection scripts.
fs.copyFileSync(shimJs, path.join(stagedDir, "shim.js"));
fs.copyFileSync(previewRuntimeJs, path.join(stagedDir, "preview-runtime.js"));

// 4. Node sidecar. Local builds copy the host Node (already the pinned major
// per engines); CI should download the official darwin build from nodejs.org.
const engines = (rootPackageJson as { engines?: { node?: string } }).engines?.node ?? "";
const nodeSource = process.env.VITRE_NODE_SIDECAR ?? process.execPath;
const binariesDir = path.join(srcTauriDir, "binaries");
fs.mkdirSync(binariesDir, { recursive: true });
const sidecarPath = path.join(binariesDir, `node-${targetTriple}`);
fs.copyFileSync(nodeSource, sidecarPath);
fs.chmodSync(sidecarPath, 0o755);
console.log(
  `[vitre-app] Node sidecar ${sidecarPath} <- ${nodeSource} (${process.version}, engines "${engines}")`,
);

// 5. tauri build (release cargo build + .app/.dmg bundling, ad-hoc signed).
const repoEnv = loadRepoEnv({ repoRoot });
const clerkKey = repoEnv.VITE_CLERK_PUBLISHABLE_KEY;
if (!clerkKey) {
  console.warn(
    "[vitre-app] No Clerk publishable key in the repo env — the packaged CSP will not admit Clerk.",
  );
}
// bundle.resources/externalBin live in a build-only overlay so plain dev
// `cargo build` (tauri-build validates them) doesn't require staged artifacts.
run("pnpm", ["exec", "tauri", "build", "--config", "src-tauri/tauri.bundle.conf.json"], {
  cwd: vitreDir,
  env: {
    ...process.env,
    ...(clerkKey ? { VITE_CLERK_PUBLISHABLE_KEY: clerkKey } : {}),
  },
});

const bundleDir = path.join(srcTauriDir, "target/release/bundle");
console.log(`[vitre-app] Done. Artifacts under ${bundleDir}`);
for (const entry of fs.existsSync(bundleDir)
  ? fs.readdirSync(bundleDir, { recursive: true }).map(String)
  : []) {
  if (entry.endsWith(".app") || entry.endsWith(".dmg")) {
    console.log(`[vitre-app]   ${path.join(bundleDir, entry)}`);
  }
}
