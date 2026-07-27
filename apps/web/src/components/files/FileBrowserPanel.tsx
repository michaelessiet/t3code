import type {
  ContextMenuAnchorRect,
  ContextMenuItem,
  ContextMenuOpenContext,
  FileTreeRenameEvent,
} from "@pierre/trees";
import type {
  EnvironmentId,
  ProjectEntry,
  ProjectMutateEntryInput,
  ScopedThreadRef,
} from "@t3tools/contracts";
import { FileTree, useFileTree } from "@pierre/trees/react";
import {
  isAtomCommandInterrupted,
  squashAtomCommandFailure,
} from "@t3tools/client-runtime/state/runtime";
import { serializeComposerFileLink } from "@t3tools/shared/composerTrigger";
import {
  AtSign,
  ClipboardPaste,
  Copy,
  FilePlus2,
  FolderInput,
  FolderPlus,
  type LucideIcon,
  MessageSquarePlus,
  PencilLine,
  RefreshCw,
  Search,
  Trash2,
} from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { stackedThreadToast, toastManager } from "~/components/ui/toast";
import { useComposerHandleContext } from "~/composerHandleContext";
import { writeTextToClipboard } from "~/hooks/useCopyToClipboard";
import { useTheme } from "~/hooks/useTheme";
import { cn } from "~/lib/utils";
import { T3_PIERRE_ICONS } from "~/pierre-icons";
import { useThreadShell } from "~/state/entities";
import { type FileClipboardEntry, useFileClipboardStore } from "~/state/fileClipboardStore";
import { projectEnvironment } from "~/state/projects";
import { useAtomCommand } from "~/state/use-atom-command";

import { useCopyEntryAcrossThreads } from "./copyEntryAcrossThreads";
import { createFileTreeDragMentionController } from "./fileTreeDragMention";
import { subscribeFileTreeAction } from "./fileTreeActionBus";
import { useProjectEntriesQuery } from "./projectFilesQueryState";
import ThreadDestinationPicker, { type ThreadDestination } from "./ThreadDestinationPicker";

interface FileBrowserPanelProps {
  environmentId: EnvironmentId;
  cwd: string;
  projectName: string;
  threadRef: ScopedThreadRef;
  onOpenFile: (relativePath: string) => void;
}

const TREE_UNSAFE_CSS = `
  :host {
    --trees-bg-override: transparent;
    --trees-selected-bg-override: color-mix(in srgb, currentColor 12%, transparent);
    --trees-hover-bg-override: color-mix(in srgb, currentColor 7%, transparent);
    --trees-border-color-override: color-mix(in srgb, currentColor 14%, transparent);
    --trees-font-family-override: var(--font-sans);
    --trees-font-size-override: 12px;
  }
  button[data-type='item'] { border-radius: 5px; }
`;

function treePath(entry: ProjectEntry): string {
  return entry.kind === "directory" ? `${entry.path}/` : entry.path;
}

function stripTrailingSlash(path: string): string {
  return path.replace(/\/$/, "");
}

function parentDirectory(path: string): string {
  const normalized = stripTrailingSlash(path);
  const slashIndex = normalized.lastIndexOf("/");
  return slashIndex === -1 ? "" : normalized.slice(0, slashIndex);
}

function entryName(path: string): string {
  const normalized = stripTrailingSlash(path);
  const slashIndex = normalized.lastIndexOf("/");
  return slashIndex === -1 ? normalized : normalized.slice(slashIndex + 1);
}

function joinChild(directory: string, name: string): string {
  return directory.length > 0 ? `${directory}/${name}` : name;
}

/**
 * Vim-style navigation for the tree: remapped onto the arrow/Home/End keys
 * the tree already understands, re-dispatched at the original (shadow DOM)
 * target so the tree's own handlers pick them up.
 */
const VIM_TREE_KEYS: Readonly<Record<string, string>> = {
  j: "ArrowDown",
  k: "ArrowUp",
  h: "ArrowLeft",
  l: "ArrowRight",
  G: "End",
};

function isTextEntryTarget(target: EventTarget | undefined): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target.isContentEditable
  );
}

interface TreeContextMenuState {
  readonly item: ContextMenuItem;
  readonly anchorRect: ContextMenuAnchorRect;
  readonly close: (options?: { restoreFocus?: boolean }) => void;
}

interface TreeContextMenuAction {
  readonly label: string;
  readonly icon: LucideIcon;
  readonly destructive?: boolean;
  readonly run: () => void;
}

export default function FileBrowserPanel({
  environmentId,
  cwd,
  projectName,
  threadRef,
  onOpenFile,
}: FileBrowserPanelProps) {
  const { resolvedTheme } = useTheme();
  const composerRef = useComposerHandleContext();
  const entriesQuery = useProjectEntriesQuery(environmentId, cwd);
  const entries = entriesQuery.data?.entries ?? [];
  const entryKinds = useMemo(
    () => new Map(entries.map((entry) => [entry.path, entry.kind] as const)),
    [entries],
  );
  const entryKindsRef = useRef<ReadonlyMap<string, ProjectEntry["kind"]>>(entryKinds);
  const treePaths = useMemo(() => entries.map(treePath), [entries]);
  const previousTreePathsRef = useRef<readonly string[]>([]);

  const mutateEntry = useAtomCommand(projectEnvironment.mutateEntry, { reportFailure: false });
  /** Placeholder paths added by New File / New Folder, awaiting their name. */
  const pendingCreatesRef = useRef(new Map<string, "file" | "directory">());
  const [contextMenu, setContextMenu] = useState<TreeContextMenuState | null>(null);

  const runMutation = useCallback(
    (input: ProjectMutateEntryInput, failureTitle: string, onSuccess?: (path: string) => void) => {
      void (async () => {
        const result = await mutateEntry({ environmentId, input });
        if (result._tag === "Success") {
          onSuccess?.(result.value.relativePath);
          entriesQuery.refresh();
          return;
        }
        if (!isAtomCommandInterrupted(result)) {
          const error = squashAtomCommandFailure(result);
          toastManager.add(
            stackedThreadToast({
              type: "error",
              title: failureTitle,
              description: error instanceof Error ? error.message : "An error occurred.",
            }),
          );
        }
        // Re-sync the tree with disk: the optimistic tree mutation (rename,
        // placeholder add) no longer reflects reality.
        entriesQuery.refresh();
      })();
    },
    [entriesQuery, environmentId, mutateEntry],
  );
  const runMutationRef = useRef(runMutation);
  useLayoutEffect(() => {
    runMutationRef.current = runMutation;
  });

  const onOpenFileRef = useRef(onOpenFile);
  useLayoutEffect(() => {
    onOpenFileRef.current = onOpenFile;
  });

  // useFileTree captures this closure once; that is safe because the
  // component remounts per project (keyed by environmentId:cwd), so `cwd`
  // never goes stale within a tree instance.
  const onRename = useCallback(
    (event: FileTreeRenameEvent) => {
      const source = stripTrailingSlash(event.sourcePath);
      const destination = stripTrailingSlash(event.destinationPath);
      const pendingKind = pendingCreatesRef.current.get(source);
      if (pendingKind !== undefined) {
        pendingCreatesRef.current.delete(source);
        runMutationRef.current(
          { _tag: "create", cwd, relativePath: destination, kind: pendingKind },
          pendingKind === "directory" ? "Could not create folder" : "Could not create file",
          (createdPath) => {
            if (pendingKind === "file") onOpenFileRef.current(createdPath);
          },
        );
        return;
      }
      if (source === destination) return;
      runMutationRef.current(
        { _tag: "rename", cwd, fromRelativePath: source, toRelativePath: destination },
        "Could not rename entry",
      );
    },
    [cwd],
  );

  // Mention helpers, offered from the tree's context menu: the composer
  // understands the serialized file link, so both actions share it.
  const copyMention = useCallback((path: string) => {
    const relativePath = stripTrailingSlash(path);
    void (async () => {
      try {
        await writeTextToClipboard(serializeComposerFileLink(relativePath));
        toastManager.add({ type: "success", title: "Mention copied", description: relativePath });
      } catch (error) {
        toastManager.add({
          type: "error",
          title: "Failed to copy mention",
          description: error instanceof Error ? error.message : "An error occurred.",
        });
      }
    })();
  }, []);

  const addMentionToChat = useCallback(
    (path: string) => {
      const composer = composerRef?.current;
      if (!composer) {
        toastManager.add({
          type: "error",
          title: "Unable to add to chat",
          description: "Open a chat for this project and try again.",
        });
        return;
      }
      const mention = serializeComposerFileLink(stripTrailingSlash(path));
      const inserted = composer.insertTextAtEnd(`${mention} `, { ensureLeadingBoundary: true });
      if (!inserted) {
        toastManager.add({
          type: "error",
          title: "Unable to add to chat",
          description: "The chat isn't ready to accept input right now.",
        });
      }
    },
    [composerRef],
  );

  const treeModelRef = useRef<ReturnType<typeof useFileTree>["model"] | null>(null);
  const dragMention = useMemo(
    () =>
      createFileTreeDragMentionController({
        deselect: (path) => treeModelRef.current?.getItem(path)?.deselect(),
      }),
    [],
  );

  const { model } = useFileTree({
    // Rows only need to be draggable so entries can be dropped into the chat
    // composer; rearranging files inside the tree stays off.
    dragAndDrop: { canDrop: () => false },
    density: "compact",
    fileTreeSearchMode: "hide-non-matches",
    flattenEmptyDirectories: true,
    initialExpansion: 1,
    icons: T3_PIERRE_ICONS,
    onSelectionChange: (selectedPaths) => {
      dragMention.handleSelectionChange(selectedPaths);
      // Starting a drag selects the dragged row; that selection is a side
      // effect of the gesture, not a request to open the file.
      if (dragMention.isDragInProgress()) {
        return;
      }
      const selectedPath = selectedPaths.at(-1)?.replace(/\/$/, "");
      if (selectedPath && entryKindsRef.current.get(selectedPath) === "file") {
        onOpenFile(selectedPath);
      }
    },
    paths: [],
    renaming: { onRename },
    composition: {
      contextMenu: {
        enabled: true,
        onOpen: (item, context: ContextMenuOpenContext) => {
          setContextMenu({ item, anchorRect: context.anchorRect, close: context.close });
        },
        onClose: () => setContextMenu(null),
        // The menu itself is portaled into the light DOM (see below) so it can
        // use the app's styles; returning null renders no shadow-DOM surface.
        render: () => null,
      },
    },
    search: true,
    unsafeCSS: TREE_UNSAFE_CSS,
  });

  useEffect(() => {
    if (previousTreePathsRef.current === treePaths) return;
    entryKindsRef.current = entryKinds;
    previousTreePathsRef.current = treePaths;
    pendingCreatesRef.current.clear();
    model.resetPaths(treePaths);
  }, [entryKinds, model, treePaths]);

  /**
   * Directory that New File / New Folder should target: the focused entry if
   * it is a directory, its parent when it is a file, the root otherwise.
   */
  const targetDirectory = useCallback(() => {
    const focused = model.getFocusedItem();
    if (!focused) return "";
    const focusedPath = stripTrailingSlash(focused.getPath());
    return focused.isDirectory() ? focusedPath : parentDirectory(focusedPath);
  }, [model]);

  const startCreate = useCallback(
    (kind: "file" | "directory", directory: string) => {
      const prefix = directory.length > 0 ? `${directory}/` : "";
      let name = "untitled";
      for (let suffix = 2; entryKindsRef.current.has(`${prefix}${name}`); suffix += 1) {
        name = `untitled-${suffix}`;
      }
      const placeholder = `${prefix}${name}`;
      pendingCreatesRef.current.set(placeholder, kind);
      model.add(kind === "directory" ? `${placeholder}/` : placeholder);
      window.requestAnimationFrame(() => {
        model.startRenaming(kind === "directory" ? `${placeholder}/` : placeholder, {
          removeIfCanceled: true,
        });
      });
    },
    [model],
  );

  const startRename = useCallback(
    (path: string) => {
      model.startRenaming(path);
    },
    [model],
  );

  const deleteEntry = useCallback(
    (path: string, kind: "file" | "directory") => {
      const normalized = stripTrailingSlash(path);
      const confirmed = window.confirm(
        kind === "directory"
          ? `Delete the folder '${entryName(normalized)}' and all of its contents?`
          : `Delete '${entryName(normalized)}'?`,
      );
      if (!confirmed) return;
      model.remove(path, { recursive: true });
      runMutationRef.current(
        { _tag: "delete", cwd, relativePath: normalized },
        "Could not delete entry",
      );
    },
    [cwd, model],
  );

  // --- Cross-thread copy / paste -------------------------------------------
  const threadShell = useThreadShell(threadRef);
  const threadTitle = threadShell?.title ?? projectName;
  const clipboardEntry = useFileClipboardStore((state) => state.entry);
  const setClipboardEntry = useFileClipboardStore((state) => state.copy);
  const copyAcrossThreads = useCopyEntryAcrossThreads();
  const [pickerSource, setPickerSource] = useState<FileClipboardEntry | null>(null);

  const buildClipboardEntry = useCallback(
    (path: string, kind: "file" | "directory"): FileClipboardEntry => {
      const relativePath = stripTrailingSlash(path);
      return {
        environmentId,
        cwd,
        relativePath,
        kind,
        name: entryName(relativePath),
        threadTitle,
        projectName,
      };
    },
    [cwd, environmentId, projectName, threadTitle],
  );

  // Latest values captured for the keyboard handler, which is registered once.
  const copyState = useRef({
    clipboardEntry,
    setClipboardEntry,
    buildClipboardEntry,
  });
  useLayoutEffect(() => {
    copyState.current = { clipboardEntry, setClipboardEntry, buildClipboardEntry };
  });

  const runCopy = useCallback(
    (
      source: FileClipboardEntry,
      destination: ThreadDestination,
      destinationRelativePath: string,
    ) => {
      void (async () => {
        const outcome = await copyAcrossThreads(
          {
            environmentId: source.environmentId,
            cwd: source.cwd,
            relativePath: source.relativePath,
            kind: source.kind,
          },
          {
            environmentId: destination.threadRef.environmentId,
            cwd: destination.cwd,
            relativePath: destinationRelativePath,
          },
        );
        if (outcome.status === "interrupted") return;
        if (outcome.status === "error") {
          toastManager.add(
            stackedThreadToast({
              type: "error",
              title: "Could not copy to thread",
              description: outcome.message,
            }),
          );
          return;
        }
        // Refresh the destination if it happens to be this panel (paste in place).
        if (destination.threadRef.environmentId === environmentId && destination.cwd === cwd) {
          entriesQuery.refresh();
        }
        toastManager.add(
          stackedThreadToast({
            type: "success",
            title: "Copied to thread",
            description: `Copied '${source.name}' to ${destination.threadTitle}.`,
          }),
        );
      })();
    },
    [copyAcrossThreads, cwd, entriesQuery, environmentId],
  );

  /** Paste the clipboard entry into a directory of the current thread. */
  const pasteInto = useCallback(
    (directory: string) => {
      const source = copyState.current.clipboardEntry;
      if (!source) return;
      const destination: ThreadDestination = {
        threadRef,
        cwd,
        threadTitle,
        projectName,
      };
      runCopy(source, destination, joinChild(stripTrailingSlash(directory), source.name));
    },
    [cwd, projectName, runCopy, threadRef, threadTitle],
  );

  const containerRef = useRef<HTMLDivElement>(null);

  // Vim movements while the tree has focus: j/k/h/l plus gg/G. The remapped
  // key is re-dispatched at the row inside the shadow DOM so the tree's own
  // keyboard handling (roving focus, expand/collapse) does the actual work.
  const pendingGRef = useRef(false);
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const handler = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      const innerTarget = event.composedPath()[0];
      if (isTextEntryTarget(innerTarget)) return;

      let mappedKey: string | undefined;
      if (event.key === "g" && !event.shiftKey) {
        if (pendingGRef.current) {
          pendingGRef.current = false;
          mappedKey = "Home";
        } else {
          pendingGRef.current = true;
          event.preventDefault();
          event.stopPropagation();
          return;
        }
      } else {
        pendingGRef.current = false;
        mappedKey = VIM_TREE_KEYS[event.key];
      }
      if (mappedKey === undefined || !(innerTarget instanceof HTMLElement)) return;

      event.preventDefault();
      event.stopPropagation();
      innerTarget.dispatchEvent(
        new KeyboardEvent("keydown", { key: mappedKey, bubbles: true, composed: true }),
      );
    };
    container.addEventListener("keydown", handler, true);
    return () => container.removeEventListener("keydown", handler, true);
  }, []);

  // Cmd/Ctrl+C copies the focused entry to the shared clipboard; Cmd/Ctrl+V
  // pastes it into the target directory of this thread.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const handler = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey) return;
      if (isTextEntryTarget(event.composedPath()[0])) return;
      const key = event.key.toLowerCase();
      if (key === "c") {
        const focused = model.getFocusedItem();
        if (!focused) return;
        event.preventDefault();
        event.stopPropagation();
        const { buildClipboardEntry, setClipboardEntry } = copyState.current;
        setClipboardEntry(
          buildClipboardEntry(focused.getPath(), focused.isDirectory() ? "directory" : "file"),
        );
        return;
      }
      if (key === "v") {
        if (!copyState.current.clipboardEntry) return;
        event.preventDefault();
        event.stopPropagation();
        pasteInto(targetDirectory());
      }
    };
    container.addEventListener("keydown", handler, true);
    return () => container.removeEventListener("keydown", handler, true);
  }, [model, pasteInto, targetDirectory]);

  const focusTree = useCallback(() => {
    // Give a row virtual focus so it becomes the roving-tabindex stop, then
    // move DOM focus onto it once the rows have re-rendered.
    model.focusNearestPath(null);
    window.requestAnimationFrame(() => {
      const host = containerRef.current?.querySelector("file-tree-container");
      const row = host?.shadowRoot?.querySelector<HTMLElement>(
        "button[data-type='item']:not([tabindex='-1'])",
      );
      row?.focus();
    });
  }, [model]);

  useEffect(
    () =>
      subscribeFileTreeAction((action) => {
        switch (action) {
          case "focus":
            focusTree();
            return;
          case "search":
            model.openSearch();
            return;
          case "new-file":
            startCreate("file", targetDirectory());
            return;
          case "new-directory":
            startCreate("directory", targetDirectory());
            return;
          case "rename": {
            const focused = model.getFocusedItem();
            if (focused) startRename(focused.getPath());
            return;
          }
        }
      }),
    [focusTree, model, startCreate, startRename, targetDirectory],
  );

  const fileCount = useMemo(
    () => entries.reduce((count, entry) => count + (entry.kind === "file" ? 1 : 0), 0),
    [entries],
  );

  // Tag tree drags with the composer mention payload. The row is read from
  // the composed event path (the tree's shadow root is open), so this does
  // not depend on running after the tree's own dragstart handler; the drag
  // data store is writable for every dragstart listener in the dispatch.
  // The capture phase runs before the tree's own dragstart handler selects
  // the dragged row, so the drag flag is up before that selection emits.
  useEffect(() => {
    treeModelRef.current = model;
  }, [model]);
  useEffect(() => {
    const panel = containerRef.current;
    if (panel === null) {
      return;
    }
    const handleDragStart = (event: DragEvent) => dragMention.handleDragStart(event);
    const handleDragEnd = () => dragMention.handleDragEnd();
    panel.addEventListener("dragstart", handleDragStart, true);
    panel.addEventListener("dragend", handleDragEnd);
    return () => {
      panel.removeEventListener("dragstart", handleDragStart, true);
      panel.removeEventListener("dragend", handleDragEnd);
    };
  }, [dragMention]);

  const contextMenuActions: ReadonlyArray<TreeContextMenuAction> | null =
    contextMenu === null
      ? null
      : [
          {
            label: "New File…",
            icon: FilePlus2,
            run: () => {
              const base = stripTrailingSlash(contextMenu.item.path);
              startCreate(
                "file",
                contextMenu.item.kind === "directory" ? base : parentDirectory(base),
              );
            },
          },
          {
            label: "New Folder…",
            icon: FolderPlus,
            run: () => {
              const base = stripTrailingSlash(contextMenu.item.path);
              startCreate(
                "directory",
                contextMenu.item.kind === "directory" ? base : parentDirectory(base),
              );
            },
          },
          {
            label: "Rename…",
            icon: PencilLine,
            run: () => startRename(contextMenu.item.path),
          },
          {
            label: "Copy",
            icon: Copy,
            run: () =>
              setClipboardEntry(buildClipboardEntry(contextMenu.item.path, contextMenu.item.kind)),
          },
          {
            label: "Copy Mention",
            icon: AtSign,
            run: () => copyMention(contextMenu.item.path),
          },
          {
            label: "Add to Chat",
            icon: MessageSquarePlus,
            run: () => addMentionToChat(contextMenu.item.path),
          },
          {
            label: "Copy to Thread…",
            icon: FolderInput,
            run: () =>
              setPickerSource(buildClipboardEntry(contextMenu.item.path, contextMenu.item.kind)),
          },
          ...(clipboardEntry
            ? [
                {
                  label: `Paste "${clipboardEntry.name}"`,
                  icon: ClipboardPaste,
                  run: () => {
                    const base = stripTrailingSlash(contextMenu.item.path);
                    pasteInto(contextMenu.item.kind === "directory" ? base : parentDirectory(base));
                  },
                } satisfies TreeContextMenuAction,
              ]
            : []),
          {
            label: "Delete",
            icon: Trash2,
            destructive: true,
            run: () => deleteEntry(contextMenu.item.path, contextMenu.item.kind),
          },
        ];

  return (
    <div
      ref={containerRef}
      className="flex min-h-0 flex-1 flex-col bg-background"
      data-file-browser-panel={`${environmentId}:${cwd}`}
    >
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-border/60 px-3">
        <div className="min-w-0 flex-1">
          <div className="truncate text-xs font-medium text-foreground">{projectName}</div>
          <div className="truncate text-[10px] leading-none text-muted-foreground">
            {entriesQuery.isPending && entriesQuery.data === null
              ? "Indexing…"
              : `${fileCount.toLocaleString()} files`}
            {entriesQuery.data?.truncated ? " · partial" : ""}
          </div>
        </div>
        <button
          type="button"
          className="rounded-md p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
          aria-label="New file"
          onClick={() => startCreate("file", targetDirectory())}
        >
          <FilePlus2 className="size-3.5" />
        </button>
        <button
          type="button"
          className="rounded-md p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
          aria-label="New folder"
          onClick={() => startCreate("directory", targetDirectory())}
        >
          <FolderPlus className="size-3.5" />
        </button>
        <button
          type="button"
          className="rounded-md p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
          aria-label="Search workspace files"
          onClick={() => model.openSearch()}
        >
          <Search className="size-3.5" />
        </button>
        <button
          type="button"
          className="rounded-md p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
          aria-label="Refresh workspace files"
          onClick={entriesQuery.refresh}
        >
          <RefreshCw className={cn("size-3.5", entriesQuery.isPending && "animate-spin")} />
        </button>
      </div>
      {entriesQuery.error && entriesQuery.data === null ? (
        <div className="p-4 text-xs leading-relaxed text-destructive">{entriesQuery.error}</div>
      ) : (
        <FileTree
          model={model}
          aria-label={`${projectName} files`}
          className="min-h-0 flex-1 overflow-hidden"
          style={{
            colorScheme: resolvedTheme,
            ["--trees-fg-override" as string]: "var(--foreground)",
          }}
        />
      )}
      {contextMenu !== null && contextMenuActions !== null
        ? createPortal(
            <div
              data-file-tree-context-menu-root="true"
              className="fixed z-50 min-w-44 rounded-md border border-border bg-popover p-1 text-popover-foreground shadow-md"
              style={{
                left: Math.min(contextMenu.anchorRect.left, window.innerWidth - 192),
                top: Math.min(contextMenu.anchorRect.bottom + 2, window.innerHeight - 160),
              }}
              role="menu"
            >
              {contextMenuActions.map((action) => (
                <button
                  key={action.label}
                  type="button"
                  role="menuitem"
                  className={cn(
                    "flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-xs hover:bg-accent hover:text-accent-foreground",
                    action.destructive === true && "text-destructive hover:text-destructive",
                  )}
                  onClick={() => {
                    contextMenu.close({ restoreFocus: false });
                    setContextMenu(null);
                    action.run();
                  }}
                >
                  <action.icon className="size-3.5" />
                  {action.label}
                </button>
              ))}
            </div>,
            document.body,
          )
        : null}
      {pickerSource !== null ? (
        <ThreadDestinationPicker
          sourceThreadRef={threadRef}
          entryName={pickerSource.name}
          onClose={() => setPickerSource(null)}
          onSelect={(destination) => {
            setPickerSource(null);
            runCopy(pickerSource, destination, pickerSource.name);
          }}
        />
      ) : null}
    </div>
  );
}
