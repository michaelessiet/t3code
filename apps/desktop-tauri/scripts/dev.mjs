// Dev runner for the Tauri shell. Prerequisites (started separately):
//   1. The web dev server:    HOST=localhost pnpm dev:web   (default port 5733)
//   2. A built server bundle: apps/server/dist/bin.mjs      (pnpm build:bundle)
//
// This script builds the shim, verifies the prerequisites, and launches
// `cargo run` with the environment the Rust shell expects.
import { spawn, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const packageDir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const repoRoot = path.resolve(packageDir, "../..");

const devServerUrl = process.env.VITE_DEV_SERVER_URL ?? "http://localhost:5733";
const serverEntry =
  process.env.T3CODE_TAURI_SERVER_ENTRY ?? path.join(repoRoot, "apps/server/dist/bin.mjs");
const shimPath = path.join(packageDir, "shim/dist/shim.js");

if (!existsSync(serverEntry)) {
  console.error(`Missing server bundle at ${serverEntry} — run \`pnpm build:bundle\` first.`);
  process.exit(1);
}

const devServerReachable = await fetch(devServerUrl, { method: "HEAD" }).then(
  () => true,
  () => false,
);
if (!devServerReachable) {
  console.error(
    `Web dev server is not reachable at ${devServerUrl} — start it with \`HOST=localhost pnpm dev:web\` (or set VITE_DEV_SERVER_URL).`,
  );
  process.exit(1);
}

const shimBuild = spawnSync(process.execPath, [path.join(packageDir, "scripts/build-shim.mjs")], {
  stdio: "inherit",
});
if (shimBuild.status !== 0) {
  process.exit(shimBuild.status ?? 1);
}

const child = spawn("cargo", ["run"], {
  cwd: path.join(packageDir, "src-tauri"),
  stdio: "inherit",
  env: {
    ...process.env,
    VITE_DEV_SERVER_URL: devServerUrl,
    T3CODE_TAURI_SERVER_ENTRY: serverEntry,
    T3CODE_TAURI_SHIM_PATH: shimPath,
  },
});
child.on("exit", (code) => process.exit(code ?? 0));
