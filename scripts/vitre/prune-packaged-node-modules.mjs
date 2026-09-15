import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

const input = process.argv[2];
if (!input) throw new Error("usage: prune-packaged-node-modules.mjs <node_modules>");
const root = await fs.realpath(input);
if (
  path.basename(root) !== "node_modules" ||
  !root.split(path.sep).some((part) => part.startsWith("vitre-dmg."))
) {
  throw new Error(`refusing to prune non-Vitre temporary path: ${root}`);
}

const currentOs = process.platform;
const currentCpu = os.arch();
let removedPackages = 0;
let removedLinks = 0;

function supports(values, current) {
  if (!Array.isArray(values) || values.length === 0) return true;
  if (values.includes(`!${current}`)) return false;
  const positive = values.filter((value) => !value.startsWith("!"));
  return positive.length === 0 || positive.includes(current);
}

async function packageRoots(wrapper) {
  const modules = path.join(wrapper, "node_modules");
  let entries;
  try {
    entries = await fs.readdir(modules, { withFileTypes: true });
  } catch {
    return [];
  }
  const roots = [];
  for (const entry of entries) {
    if (!entry.isDirectory()) continue;
    const candidate = path.join(modules, entry.name);
    if (!entry.name.startsWith("@")) {
      roots.push(candidate);
      continue;
    }
    for (const scoped of await fs.readdir(candidate, { withFileTypes: true })) {
      if (scoped.isDirectory()) roots.push(path.join(candidate, scoped.name));
    }
  }
  return roots;
}

const virtualStore = path.join(root, ".pnpm");
for (const wrapper of await fs.readdir(virtualStore, { withFileTypes: true })) {
  if (!wrapper.isDirectory() || wrapper.name === "node_modules") continue;
  for (const packageRoot of await packageRoots(path.join(virtualStore, wrapper.name))) {
    let manifest;
    try {
      manifest = JSON.parse(await fs.readFile(path.join(packageRoot, "package.json"), "utf8"));
    } catch {
      continue;
    }
    if (!supports(manifest.os, currentOs) || !supports(manifest.cpu, currentCpu)) {
      await fs.rm(packageRoot, { recursive: true, force: true });
      removedPackages += 1;
    }
  }
}

// Legacy pnpm deploy adds a workspace self-link that escapes the application
// bundle. The built server entry is copied directly and never resolves it.
await fs.rm(path.join(virtualStore, "node_modules", "t3"), { force: true });

async function removeBrokenLinks(directory) {
  for (const entry of await fs.readdir(directory, { withFileTypes: true })) {
    const candidate = path.join(directory, entry.name);
    const stat = await fs.lstat(candidate);
    if (stat.isSymbolicLink()) {
      try {
        await fs.stat(candidate);
      } catch (error) {
        if (error?.code !== "ENOENT") throw error;
        await fs.unlink(candidate);
        removedLinks += 1;
      }
    } else if (stat.isDirectory()) {
      await removeBrokenLinks(candidate);
    }
  }
}

await removeBrokenLinks(root);
console.log(
  `[vitre-dmg] pruned ${removedPackages} incompatible packages and ${removedLinks} broken links`,
);
