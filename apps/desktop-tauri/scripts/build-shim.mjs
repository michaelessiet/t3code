// Bundles the desktopBridge shim into a single self-contained IIFE that the
// Rust shell injects as a webview initialization script. All
// @t3tools/contracts imports are type-only, so the output has no runtime
// dependencies; esbuild is resolved from the repo root install.
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const packageDir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const repoRoot = path.resolve(packageDir, "../..");
const esbuildBin = path.join(repoRoot, "node_modules", ".bin", "esbuild");

const result = spawnSync(
  esbuildBin,
  [
    path.join(packageDir, "shim/src/index.ts"),
    "--bundle",
    "--format=iife",
    "--platform=browser",
    "--target=es2022",
    `--outfile=${path.join(packageDir, "shim/dist/shim.js")}`,
  ],
  { stdio: "inherit" },
);

if (result.status !== 0) {
  process.exit(result.status ?? 1);
}
