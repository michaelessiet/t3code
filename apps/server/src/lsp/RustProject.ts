// @effect-diagnostics nodeBuiltinImport:off
/**
 * RustProject - Cargo manifest discovery for rust-analyzer.
 *
 * rust-analyzer finds crates by looking for a `Cargo.toml` at its rootUri and
 * gives up if there is none: it logs "failed to find any projects", reports
 * "Failed to load workspaces", publishes no diagnostics, and answers every
 * request with null. Because LspClient always roots a server at the project
 * cwd (there is no root-marker discovery), that is exactly what happens in a
 * polyglot monorepo whose crates live in subdirectories — the server looks
 * healthy while providing nothing.
 *
 * The remedy is rust-analyzer's own `linkedProjects` setting, naming the
 * manifests explicitly. It must travel in `initializationOptions`: crate
 * discovery runs during the initialize handshake, and rust-analyzer never
 * issues `workspace/configuration`, so the pull-based channel LspClient uses
 * for vtsls cannot carry it.
 *
 * @module RustProject
 */
import * as NodeFSP from "node:fs/promises";
import * as NodePath from "node:path";

/**
 * Directories never worth descending into. `target` matters most: a built
 * Cargo project vendors manifests under it, and registry sources there would
 * otherwise be linked as first-class projects.
 */
const PRUNED_DIRECTORIES: ReadonlySet<string> = new Set([
  ".build",
  ".git",
  ".jj",
  ".next",
  ".t3",
  ".turbo",
  ".venv",
  "build",
  "dist",
  "node_modules",
  "out",
  "target",
  "vendor",
]);

/**
 * How deep to look. Crates in a monorepo sit a few levels down
 * (`apps/desktop-tauri/src-tauri` is 3); an unbounded walk would make server
 * startup hostage to repository size.
 */
const MAX_DEPTH = 5;

/**
 * Ceiling on linked projects. rust-analyzer builds a crate graph per linked
 * project, so a pathological repository must not translate into an unbounded
 * indexing job.
 */
const MAX_MANIFESTS = 24;

export interface DiscoverCargoManifestsOptions {
  readonly maxDepth?: number;
  readonly maxManifests?: number;
}

async function hasCargoManifest(directory: string): Promise<boolean> {
  try {
    const stats = await NodeFSP.stat(NodePath.join(directory, "Cargo.toml"));
    return stats.isFile();
  } catch {
    return false;
  }
}

/**
 * Absolute paths of the top-most `Cargo.toml` files beneath `workspaceRoot`,
 * breadth-first so shallower crates win the manifest budget.
 *
 * Returns an empty array when the root itself is a Cargo project: rust-analyzer
 * discovers that natively, and native discovery understands `[workspace]`
 * members better than an explicit list would.
 *
 * Descent stops at any directory holding a manifest. Cargo already expands a
 * workspace to its members, so linking both a workspace root and its members
 * would load the same crates twice.
 */
export async function discoverCargoManifests(
  workspaceRoot: string,
  options: DiscoverCargoManifestsOptions = {},
): Promise<ReadonlyArray<string>> {
  const maxDepth = options.maxDepth ?? MAX_DEPTH;
  const maxManifests = options.maxManifests ?? MAX_MANIFESTS;

  if (await hasCargoManifest(workspaceRoot)) return [];

  const manifests: string[] = [];
  let frontier: ReadonlyArray<string> = [workspaceRoot];

  for (let depth = 0; depth < maxDepth && frontier.length > 0; depth += 1) {
    const next: string[] = [];
    for (const directory of frontier) {
      let entries;
      try {
        entries = await NodeFSP.readdir(directory, { withFileTypes: true });
      } catch {
        // Unreadable directories (permissions, races with a build) are skipped
        // rather than failing discovery for the whole workspace.
        continue;
      }
      for (const entry of entries) {
        if (!entry.isDirectory() || PRUNED_DIRECTORIES.has(entry.name)) continue;
        const child = NodePath.join(directory, entry.name);
        if (await hasCargoManifest(child)) {
          manifests.push(NodePath.join(child, "Cargo.toml"));
          if (manifests.length >= maxManifests) return manifests;
          // Prune: cargo expands this project's own workspace members.
          continue;
        }
        next.push(child);
      }
    }
    frontier = next;
  }

  return manifests;
}

/**
 * `initializationOptions` for rust-analyzer in `workspaceRoot`, or undefined
 * when the defaults already suffice (root is itself a Cargo project, or the
 * workspace holds no crates at all).
 */
export async function rustAnalyzerInitializationOptions(
  workspaceRoot: string,
): Promise<Record<string, unknown> | undefined> {
  const linkedProjects = await discoverCargoManifests(workspaceRoot);
  if (linkedProjects.length === 0) return undefined;
  return { linkedProjects };
}
