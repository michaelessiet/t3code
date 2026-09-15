#!/usr/bin/env node
// Dev runner for the native Vitre shell: ensure the server bundle exists,
// then `cargo run -p vitre-app` against it. The Rust shell spawns and
// supervises the sidecar itself (same model as the packaged app), so unlike
// `dev:desktop` there is no separate server dev process here — rebuild the
// bundle (`pnpm --filter t3 build:bundle` or `--rebuild`) to pick up server
// changes.

import { spawn, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const serverEntry = path.join(repoRoot, "apps/server/dist/bin.mjs");
const rebuild = process.argv.includes("--rebuild");

if (rebuild || !existsSync(serverEntry)) {
  console.log("[dev:vitre] building server bundle (vp pack)…");
  const result = spawnSync("pnpm", ["--filter", "t3", "run", "build:bundle"], {
    cwd: repoRoot,
    stdio: "inherit",
  });
  if (result.status !== 0) process.exit(result.status ?? 1);
}

const cargoArgs = ["run", "-p", "vitre-app"];
const extra = process.argv.slice(2).filter((arg) => arg !== "--rebuild");
if (extra.length > 0) cargoArgs.push(...extra);

console.log(`[dev:vitre] cargo ${cargoArgs.join(" ")}`);
const child = spawn("cargo", cargoArgs, {
  cwd: repoRoot,
  stdio: "inherit",
  env: {
    ...process.env,
    VITRE_SERVER_ENTRY: serverEntry,
  },
});
child.on("exit", (code) => process.exit(code ?? 0));
for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => child.kill(signal));
}
