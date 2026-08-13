import type { EnvironmentId } from "@t3tools/contracts";
import { ChevronDownIcon, FolderGit2Icon } from "lucide-react";

import { cn } from "~/lib/utils";
import { useMultiRootGitStatusSummaries } from "~/state/queries";
import type { ThreadRoot } from "~/state/threadRoots";

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "./ui/menu";
import { Toggle, ToggleGroup } from "./ui/toggle-group";

/** Sentinel ToggleGroup value for the primary root (selection is null). */
const PRIMARY_ROOT_VALUE = "__primary__";

interface GitRootSwitcherProps {
  environmentId: EnvironmentId;
  /** Effective thread roots, primary first (`ThreadRoots.all`). */
  roots: ReadonlyArray<ThreadRoot>;
  /** Absolute path of the selected root; null = primary. */
  selectedRootPath: string | null;
  onSelect: (rootPath: string | null) => void;
}

function changeCountBadge(count: number | null) {
  if (count === null || count === 0) return null;
  return (
    <span className="ml-1 rounded-full bg-foreground/10 px-1 text-[9px] tabular-nums leading-3.5">
      {count}
    </span>
  );
}

/**
 * Picks which workspace root the git surfaces operate on. Renders nothing for
 * single-root threads; segmented control up to three roots, dropdown beyond.
 * Each option shows the root's live changed-file count.
 */
export function GitRootSwitcher({
  environmentId,
  roots,
  selectedRootPath,
  onSelect,
}: GitRootSwitcherProps) {
  const summaries = useMultiRootGitStatusSummaries(environmentId, roots);
  if (roots.length <= 1) return null;

  const selectedRoot =
    roots.find((root) => !root.isPrimary && root.path === selectedRootPath) ?? roots[0];

  if (roots.length <= 3) {
    return (
      <ToggleGroup
        className="shrink-0"
        variant="outline"
        size="xs"
        aria-label="Git repository"
        value={[selectedRootPath ?? PRIMARY_ROOT_VALUE]}
        onValueChange={(value) => {
          const next = value[0];
          if (typeof next !== "string") return;
          onSelect(next === PRIMARY_ROOT_VALUE ? null : next);
        }}
      >
        {summaries.map(({ root, changedFileCount }) => (
          <Toggle
            key={root.path}
            aria-label={`Show git changes for ${root.label}`}
            title={root.path}
            value={root.isPrimary ? PRIMARY_ROOT_VALUE : root.path}
          >
            <span className="max-w-24 truncate">{root.label}</span>
            {changeCountBadge(changedFileCount)}
          </Toggle>
        ))}
      </ToggleGroup>
    );
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        className="inline-flex h-6 max-w-40 shrink-0 items-center gap-1 rounded-md bg-muted/70 px-2 text-xs font-medium text-foreground outline-none transition-colors hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring"
        aria-label={`Git repository: ${selectedRoot?.label ?? "primary"}`}
      >
        <FolderGit2Icon className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="truncate">{selectedRoot?.label}</span>
        <ChevronDownIcon className="size-3.5 shrink-0 text-muted-foreground" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-64">
        {summaries.map(({ root, changedFileCount }) => {
          const isSelected = root.isPrimary
            ? selectedRootPath === null
            : root.path === selectedRootPath;
          return (
            <DropdownMenuItem
              key={root.path}
              className={cn(isSelected ? "bg-foreground/[0.08]" : undefined)}
              onClick={() => onSelect(root.isPrimary ? null : root.path)}
            >
              <span className="min-w-0 truncate" title={root.path}>
                {root.label}
              </span>
              {changedFileCount !== null && changedFileCount > 0 ? (
                <span className="ml-auto text-xs tabular-nums text-muted-foreground">
                  {changedFileCount}
                </span>
              ) : null}
            </DropdownMenuItem>
          );
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
