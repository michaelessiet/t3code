import type { EnvironmentId, ScopedThreadRef } from "@t3tools/contracts";
import { ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";

import { cn } from "~/lib/utils";
import type { ThreadRoot } from "~/state/threadRoots";

import FileBrowserPanel from "./FileBrowserPanel";

interface MultiRootFileBrowserProps {
  environmentId: EnvironmentId;
  /** Effective thread roots, primary first (`ThreadRoots.all`). */
  roots: ReadonlyArray<ThreadRoot>;
  projectName: string;
  threadRef: ScopedThreadRef;
  /** Relative path of the file open in the editor, revealed only in the
      tree whose root matches `activeRootPath`. */
  openRelativePath: string | null;
  /** Root of the open file; null means the primary root. */
  activeRootPath: string | null;
  revealRequestId: number;
  onOpenFile: (
    relativePath: string,
    line?: number,
    options?: { readonly rootPath?: string | null },
  ) => void;
}

/**
 * The file explorer for a thread's workspace roots: one collapsible tree per
 * root. With a single root it renders exactly the plain `FileBrowserPanel`.
 * Each tree is keyed by `${environmentId}:${root.path}` so per-cwd queries and
 * unsaved-buffer state stay isolated per root.
 */
export default function MultiRootFileBrowser({
  environmentId,
  roots,
  projectName,
  threadRef,
  openRelativePath,
  activeRootPath,
  revealRequestId,
  onOpenFile,
}: MultiRootFileBrowserProps) {
  const [collapsedRootPaths, setCollapsedRootPaths] = useState<ReadonlySet<string>>(
    () => new Set(),
  );

  const panelFor = (root: ThreadRoot) => {
    const isActiveRoot = root.isPrimary ? activeRootPath === null : root.path === activeRootPath;
    return (
      <FileBrowserPanel
        key={`${environmentId}:${root.path}`}
        environmentId={environmentId}
        cwd={root.path}
        projectName={root.isPrimary ? projectName : root.label}
        threadRef={threadRef}
        mentionRootPath={root.isPrimary ? null : root.path}
        openRelativePath={isActiveRoot ? openRelativePath : null}
        revealRequestId={revealRequestId}
        onOpenFile={(relativePath: string) =>
          onOpenFile(relativePath, undefined, root.isPrimary ? undefined : { rootPath: root.path })
        }
      />
    );
  };

  const primaryOnly = roots.length <= 1;
  if (primaryOnly) {
    const root = roots[0];
    return root === undefined ? null : panelFor(root);
  }

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      {roots.map((root) => {
        const collapsed = collapsedRootPaths.has(root.path);
        return (
          <section
            key={root.path}
            className={cn(
              "flex min-w-0 flex-col border-b border-border/60 last:border-b-0",
              collapsed ? "shrink-0" : "min-h-0 flex-1",
            )}
          >
            <button
              type="button"
              className="flex w-full shrink-0 items-center gap-1.5 px-2 py-1.5 text-left text-[11px] font-medium uppercase tracking-wide text-muted-foreground hover:text-foreground"
              aria-expanded={!collapsed}
              onClick={() =>
                setCollapsedRootPaths((current) => {
                  const next = new Set(current);
                  if (next.has(root.path)) {
                    next.delete(root.path);
                  } else {
                    next.add(root.path);
                  }
                  return next;
                })
              }
            >
              {collapsed ? (
                <ChevronRight className="size-3 shrink-0" aria-hidden />
              ) : (
                <ChevronDown className="size-3 shrink-0" aria-hidden />
              )}
              <span className="truncate" title={root.path}>
                {root.label}
              </span>
            </button>
            {collapsed ? null : <div className="flex min-h-0 flex-1">{panelFor(root)}</div>}
          </section>
        );
      })}
    </div>
  );
}
