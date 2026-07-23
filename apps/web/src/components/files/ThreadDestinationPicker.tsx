/**
 * Modal picker for choosing a destination thread when copying a workspace entry
 * to another thread ("Copy to Thread…").
 *
 * Only the active thread's file explorer is mounted, so the destination panel
 * isn't available to drop into. This lists every open, non-archived thread
 * (across environments) and resolves each thread's workspace root
 * (`worktreePath ?? project.workspaceRoot`) so the copy can target it directly.
 */
import type { EnvironmentId, ScopedThreadRef } from "@t3tools/contracts";
import { CornerDownLeft, Folder } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { cn } from "~/lib/utils";
import { useProjects, useThreadShells } from "~/state/entities";

export interface ThreadDestination {
  readonly threadRef: ScopedThreadRef;
  /** Destination workspace root (thread worktree path or project root). */
  readonly cwd: string;
  readonly threadTitle: string;
  readonly projectName: string;
}

interface ThreadDestinationPickerProps {
  /** Source thread, excluded from the destination list. */
  readonly sourceThreadRef: ScopedThreadRef;
  /** Label of the entry being copied, shown in the header. */
  readonly entryName: string;
  readonly onSelect: (destination: ThreadDestination) => void;
  readonly onClose: () => void;
}

function projectKey(environmentId: EnvironmentId, projectId: string): string {
  return `${environmentId}:${projectId}`;
}

export default function ThreadDestinationPicker({
  sourceThreadRef,
  entryName,
  onSelect,
  onClose,
}: ThreadDestinationPickerProps) {
  const threadShells = useThreadShells();
  const projects = useProjects();
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const destinations = useMemo<ReadonlyArray<ThreadDestination>>(() => {
    const projectByKey = new Map(
      projects.map((project) => [projectKey(project.environmentId, project.id), project] as const),
    );
    return threadShells
      .filter((thread) => thread.archivedAt === null)
      .filter(
        (thread) =>
          !(
            thread.environmentId === sourceThreadRef.environmentId &&
            thread.id === sourceThreadRef.threadId
          ),
      )
      .flatMap((thread) => {
        const project = projectByKey.get(projectKey(thread.environmentId, thread.projectId));
        if (!project) return [];
        const cwd = thread.worktreePath ?? project.workspaceRoot;
        return [
          {
            threadRef: { environmentId: thread.environmentId, threadId: thread.id },
            cwd,
            threadTitle: thread.title,
            projectName: project.title,
          } satisfies ThreadDestination,
        ];
      });
  }, [projects, sourceThreadRef.environmentId, sourceThreadRef.threadId, threadShells]);

  const filtered = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (normalized.length === 0) return destinations;
    return destinations.filter(
      (destination) =>
        destination.threadTitle.toLowerCase().includes(normalized) ||
        destination.projectName.toLowerCase().includes(normalized),
    );
  }, [destinations, query]);

  useEffect(() => {
    setActiveIndex(0);
  }, [query]);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActiveIndex((index) => Math.min(index + 1, Math.max(filtered.length - 1, 0)));
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      setActiveIndex((index) => Math.max(index - 1, 0));
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      const destination = filtered[activeIndex];
      if (destination) onSelect(destination);
    }
  };

  return createPortal(
    <div
      className="fixed inset-0 z-50 flex flex-col items-center bg-background/60 px-4 py-[10vh] backdrop-blur-xs"
      role="presentation"
      onClick={onClose}
    >
      <div
        className="flex max-h-[70vh] w-full max-w-md flex-col overflow-hidden rounded-xl border border-border bg-popover text-popover-foreground shadow-lg"
        role="dialog"
        aria-label="Copy to thread"
        onClick={(event) => event.stopPropagation()}
        onKeyDown={handleKeyDown}
      >
        <div className="border-b border-border/60 px-3 py-2.5">
          <div className="truncate text-xs font-medium text-foreground">
            Copy <span className="text-muted-foreground">{entryName}</span> to thread…
          </div>
          <input
            ref={inputRef}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search threads…"
            className="mt-2 w-full rounded-md border border-border bg-background px-2 py-1.5 text-xs outline-none focus:border-ring"
          />
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto p-1">
          {filtered.length === 0 ? (
            <div className="px-3 py-6 text-center text-xs text-muted-foreground">
              No other threads available.
            </div>
          ) : (
            filtered.map((destination, index) => (
              <button
                key={`${destination.threadRef.environmentId}:${destination.threadRef.threadId}`}
                type="button"
                className={cn(
                  "flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-xs",
                  index === activeIndex ? "bg-accent text-accent-foreground" : "hover:bg-accent/60",
                )}
                onMouseEnter={() => setActiveIndex(index)}
                onClick={() => onSelect(destination)}
              >
                <Folder className="size-3.5 shrink-0 text-muted-foreground" />
                <span className="min-w-0 flex-1">
                  <span className="block truncate font-medium text-foreground">
                    {destination.threadTitle}
                  </span>
                  <span className="block truncate text-[10px] text-muted-foreground">
                    {destination.projectName}
                  </span>
                </span>
                {index === activeIndex ? (
                  <CornerDownLeft className="size-3 shrink-0 text-muted-foreground" />
                ) : null}
              </button>
            ))
          )}
        </div>
      </div>
    </div>,
    document.body,
  );
}
