// Dev runner for the Tauri shell. Run everything with one command from the
// repo root:
//
//   pnpm dev:vitre
//
// (scripts/dev-runner.ts starts the web dev server and this script in
// parallel with pinned HOST/ports, like dev:desktop does for Electron.)
//
// Standalone prerequisites, if running `pnpm dev` in this package directly:
//   1. The web dev server, bound to IPv4 with a pinned HMR host:
//        cd apps/web && HOST=127.0.0.1 ../../node_modules/.bin/vp dev
//      (`pnpm dev:web` won't work — scripts/dev-runner.ts deletes HOST for
//      non-desktop modes, and HOST=localhost binds IPv6-only on macOS while
//      the Rust proxy dials 127.0.0.1.)
//   2. A built server bundle: apps/server/dist/bin.mjs (pnpm build:bundle)
//
// This script builds the shim, waits for the prerequisites, and launches
// `cargo run` with the environment the Rust shell expects.
import { spawn, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const packageDir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const repoRoot = path.resolve(packageDir, "../..");

const devServerUrl = process.env.VITE_DEV_SERVER_URL ?? "http://127.0.0.1:5733";
const serverEntry =
  process.env.VITRE_SERVER_ENTRY ?? path.join(repoRoot, "apps/server/dist/bin.mjs");
const shimPath = path.join(packageDir, "shim/dist/shim.js");

// Under `pnpm dev:vitre` the web dev server starts concurrently with
// this script, so both prerequisites are awaited rather than asserted.
const WAIT_TIMEOUT_MS = 120_000;
const WAIT_POLL_MS = 500;

async function waitFor(check, description, hint) {
  const deadline = Date.now() + WAIT_TIMEOUT_MS;
  let announced = false;
  for (;;) {
    if (await check()) {
      return;
    }
    if (Date.now() >= deadline) {
      console.error(`Timed out waiting for ${description}. ${hint}`);
      process.exit(1);
    }
    if (!announced) {
      announced = true;
      console.log(`[vitre] waiting for ${description}…`);
    }
    await new Promise((resolve) => setTimeout(resolve, WAIT_POLL_MS));
  }
}

await waitFor(
  () => existsSync(serverEntry),
  `the server bundle at ${serverEntry}`,
  "Run `pnpm build:bundle` first.",
);
await waitFor(
  () =>
    fetch(devServerUrl, { method: "HEAD" }).then(
      () => true,
      () => false,
    ),
  `the web dev server at ${devServerUrl}`,
  "Start it with `cd apps/web && HOST=127.0.0.1 ../../node_modules/.bin/vp dev` (or set VITE_DEV_SERVER_URL).",
);

for (const script of ["scripts/build-shim.mjs", "scripts/build-preview-runtime.mjs"]) {
  const build = spawnSync(process.execPath, [path.join(packageDir, script)], {
    stdio: "inherit",
  });
  if (build.status !== 0) {
    process.exit(build.status ?? 1);
  }
}

// Build first, run the binary directly (not `cargo run`): cargo does not
// forward signals to its child, which would strand the shell — and the Node
// backend it supervises — when the dev runner tears this task down.
const srcTauriDir = path.join(packageDir, "src-tauri");
const cargoBuild = spawnSync("cargo", ["build"], { cwd: srcTauriDir, stdio: "inherit" });
if (cargoBuild.status !== 0) {
  process.exit(cargoBuild.status ?? 1);
}

const child = spawn(path.join(srcTauriDir, "target/debug/vitre"), [], {
  cwd: srcTauriDir,
  stdio: "inherit",
  env: {
    ...process.env,
    VITE_DEV_SERVER_URL: devServerUrl,
    VITRE_SERVER_ENTRY: serverEntry,
    VITRE_SHIM_PATH: shimPath,
    VITRE_PREVIEW_RUNTIME_PATH: path.join(packageDir, "shim/dist/preview-runtime.js"),
  },
});
child.on("exit", (code) => process.exit(code ?? 0));
// Hand teardown signals to the shell so its handler can reap the Node backend.
for (const signal of ["SIGTERM", "SIGINT"]) {
  process.on(signal, () => {
    child.kill(signal);
  });
}
// If the task runner above dies without signaling us (single-PID kill rather
// than a terminal's process-group Ctrl+C), this process reparents to init;
// tear the shell down instead of leaving an orphaned app + backend.
setInterval(() => {
  if (process.ppid === 1) {
    child.kill("SIGTERM");
    setTimeout(() => process.exit(0), 1_000);
  }
}, 2_000).unref();
