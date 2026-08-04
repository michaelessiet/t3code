"use client";

import { Dialog as DialogPrimitive } from "@base-ui/react/dialog";
import { useAtomValue } from "@effect/atom-react";
import { useNavigate } from "@tanstack/react-router";
import type {
  OrchestrationMessageSearchMatch,
  ProjectSearchContentMatch,
} from "@t3tools/contracts";
import type { EnvironmentThreadShell } from "@t3tools/client-runtime/state/models";
import { scopeProjectRef, scopeThreadRef } from "@t3tools/client-runtime/environment";
import { GitBranch, MessageSquare, SearchIcon } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { isCommandPaletteOpen } from "../commandPaletteBus";
import { subscribeAppCommand } from "./appCommandBus";
import { isEditorFocused } from "../lib/editorFocus";
import { isFileTreeFocused } from "../lib/fileTreeFocus";
import { isPreviewFocused } from "../lib/previewFocus";
import { isTerminalFocused } from "../lib/terminalFocus";
import { useHandleNewThread } from "../hooks/useHandleNewThread";
import { useDebouncedValue } from "../hooks/useDebouncedValue";
import { resolveShortcutCommand, shortcutLabelForCommand } from "../keybindings";
import { useRightPanelStore } from "../rightPanelStore";
import { buildThreadRouteParams } from "../threadRoutes";
import { DialogBackdrop, DialogPortal, DialogViewport, Dialog } from "~/components/ui/dialog";
import { ScrollArea } from "~/components/ui/scroll-area";
import { cn } from "~/lib/utils";
import { useProject, useThreadShells } from "~/state/entities";
import { orchestrationEnvironment } from "~/state/orchestration";
import { projectEnvironment } from "~/state/projects";
import { useEnvironmentQuery } from "~/state/query";
import { primaryServerKeybindingsAtom } from "~/state/server";

import { CodeMirrorFileEditor } from "./files/codemirror/CodeMirrorFileEditor";
import { requestEditorFocus } from "./files/editorFocusRequest";
import { PierreFileIcon } from "./files/PierreFileIcon";
import { useProjectFileQuery } from "./files/projectFilesQueryState";
import { matchLineSegments, splitSearchResultPath } from "./SearchPanel.logic";

type QuickSearchMode = "open" | "content";

const QUERY_DEBOUNCE_MS = 200;
const THREAD_RESULT_LIMIT = 8;
const FILE_NAME_RESULT_LIMIT = 15;
const FILE_CONTENT_RESULT_LIMIT = 100;
const FILE_CONTENT_DISPLAY_LIMIT = 30;
const MESSAGE_RESULT_LIMIT = 15;

type QuickSearchItem =
  | { readonly key: string; readonly kind: "thread"; readonly thread: EnvironmentThreadShell }
  | {
      readonly key: string;
      readonly kind: "file";
      readonly path: string;
      readonly ignored: boolean;
    }
  | { readonly key: string; readonly kind: "file-match"; readonly match: ProjectSearchContentMatch }
  | {
      readonly key: string;
      readonly kind: "message";
      readonly match: OrchestrationMessageSearchMatch;
    };

interface QuickSearchGroup {
  readonly label: string;
  readonly items: ReadonlyArray<QuickSearchItem>;
}

function rankThreads(
  threads: ReadonlyArray<EnvironmentThreadShell>,
  query: string,
): ReadonlyArray<EnvironmentThreadShell> {
  const active = threads.filter((thread) => thread.archivedAt === null);
  const matching =
    query.length === 0
      ? active
      : active.filter((thread) => thread.title.toLowerCase().includes(query.toLowerCase()));
  return matching
    .toSorted((left, right) => right.updatedAt.localeCompare(left.updatedAt))
    .slice(0, THREAD_RESULT_LIMIT);
}

function formatRelativeTime(iso: string): string {
  const elapsedMs = Date.now() - new Date(iso).getTime();
  const minutes = Math.round(elapsedMs / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

/** Case-insensitive first-occurrence split for name-match highlighting. */
function nameSegments(text: string, query: string): [string, string, string] {
  if (query.length === 0) return [text, "", ""];
  const index = text.toLowerCase().indexOf(query.toLowerCase());
  if (index === -1) return [text, "", ""];
  return [
    text.slice(0, index),
    text.slice(index, index + query.length),
    text.slice(index + query.length),
  ];
}

function HighlightedName({ text, query }: { text: string; query: string }) {
  const [before, matched, after] = nameSegments(text, query);
  return (
    <span className="truncate">
      {before}
      {matched.length > 0 ? (
        <span className="rounded-xs bg-amber-400/25 text-foreground">{matched}</span>
      ) : null}
      {after}
    </span>
  );
}

function FilePreview({
  environmentId,
  cwd,
  relativePath,
  revealLine,
}: {
  environmentId: Parameters<typeof useProjectFileQuery>[0];
  cwd: string;
  relativePath: string;
  revealLine: number | null;
}) {
  const file = useProjectFileQuery(environmentId, cwd, relativePath);

  if (file.error && file.data === null) {
    return <div className="p-4 text-xs text-destructive">{file.error}</div>;
  }
  if (file.data === null) {
    return <div className="p-4 text-xs text-muted-foreground">Loading…</div>;
  }

  return (
    <CodeMirrorFileEditor
      relativePath={relativePath}
      contents={file.data.contents}
      wordWrap={false}
      readOnly
      revealLine={revealLine}
      revealRequestId={1}
      className="quick-search-preview min-h-0 flex-1 overflow-hidden [&_.cm-editor]:h-full [&_.cm-editor]:text-[11px]"
    />
  );
}

export function QuickSearch() {
  const keybindings = useAtomValue(primaryServerKeybindingsAtom);
  const navigate = useNavigate();
  const threads = useThreadShells();
  const { activeDraftThread, activeThread, defaultProjectRef, routeThreadRef } =
    useHandleNewThread();
  // Search scope: the open thread, else the draft being composed, else the
  // first project — search must keep working from the home and new-chat
  // pages, not only inside a thread.
  const scopeRef = activeThread
    ? scopeProjectRef(activeThread.environmentId, activeThread.projectId)
    : activeDraftThread
      ? scopeProjectRef(activeDraftThread.environmentId, activeDraftThread.projectId)
      : defaultProjectRef;
  const project = useProject(scopeRef);
  const environmentId = scopeRef?.environmentId ?? null;
  const cwd = activeThread
    ? (activeThread.worktreePath ?? project?.workspaceRoot ?? null)
    : (activeDraftThread?.worktreePath ?? project?.workspaceRoot ?? null);

  const [mode, setMode] = useState<QuickSearchMode | null>(null);
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const open = mode !== null;
  /** Set when closing because a file was opened: skip dialog focus restore. */
  const activatedFileRef = useRef(false);

  const close = useCallback(() => {
    setMode(null);
    setQuery("");
    setSelectedIndex(0);
  }, []);

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      const command = resolveShortcutCommand(event, keybindings, {
        context: {
          terminalFocus: isTerminalFocused(),
          previewFocus: isPreviewFocused(),
          editorFocus: isEditorFocused(),
          fileTreeFocus: isFileTreeFocused(),
        },
      });
      if (command !== "quickSearch.open" && command !== "quickSearch.content") return;
      if (!open && isCommandPaletteOpen()) return;
      event.preventDefault();
      event.stopPropagation();
      const nextMode: QuickSearchMode = command === "quickSearch.open" ? "open" : "content";
      if (mode === nextMode) {
        close();
        return;
      }
      activatedFileRef.current = false;
      setMode(nextMode);
      setSelectedIndex(0);
    };
    window.addEventListener("keydown", handler, true);
    // Command-palette entry points dispatch the same commands over the bus.
    const unsubscribe = subscribeAppCommand((command) => {
      if (command !== "quickSearch.open" && command !== "quickSearch.content") return;
      activatedFileRef.current = false;
      setMode(command === "quickSearch.open" ? "open" : "content");
      setSelectedIndex(0);
    });
    return () => {
      window.removeEventListener("keydown", handler, true);
      unsubscribe();
    };
  }, [close, keybindings, mode, open]);

  // Keep the input focused through the open transition: when launched from
  // the command palette, the palette's closing dialog restores focus to the
  // previously-focused element a beat after our autoFocus fires.
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!open) return;
    let attempts = 0;
    let frame = 0;
    const tick = () => {
      const input = inputRef.current;
      if (input !== null && document.activeElement !== input) input.focus();
      attempts += 1;
      if (attempts < 30) frame = window.requestAnimationFrame(tick);
    };
    frame = window.requestAnimationFrame(tick);
    return () => window.cancelAnimationFrame(frame);
  }, [open]);

  const trimmedQuery = query.trim();
  const debouncedQuery = useDebouncedValue(trimmedQuery, QUERY_DEBOUNCE_MS);

  const fileNameResults = useEnvironmentQuery(
    mode === "open" && environmentId !== null && cwd !== null && debouncedQuery.length > 0
      ? projectEnvironment.searchEntries({
          environmentId,
          input: { cwd, query: debouncedQuery, limit: FILE_NAME_RESULT_LIMIT },
        })
      : null,
  );
  const fileContentResults = useEnvironmentQuery(
    mode === "content" && environmentId !== null && cwd !== null && debouncedQuery.length >= 2
      ? projectEnvironment.searchContent({
          environmentId,
          input: { cwd, query: debouncedQuery, maxResults: FILE_CONTENT_RESULT_LIMIT },
        })
      : null,
  );
  const messageResults = useEnvironmentQuery(
    mode === "content" && environmentId !== null && debouncedQuery.length >= 2
      ? orchestrationEnvironment.searchMessages({
          environmentId,
          input: { query: debouncedQuery, limit: MESSAGE_RESULT_LIMIT },
        })
      : null,
  );

  const groups = useMemo((): ReadonlyArray<QuickSearchGroup> => {
    if (mode === null) return [];
    if (mode === "open") {
      const threadItems = rankThreads(threads, debouncedQuery).map(
        (thread): QuickSearchItem => ({
          key: `thread:${thread.environmentId}:${thread.id}`,
          kind: "thread",
          thread,
        }),
      );
      const fileItems = (fileNameResults.data?.entries ?? [])
        .filter((entry) => entry.kind === "file")
        .map(
          (entry): QuickSearchItem => ({
            key: `file:${entry.path}`,
            kind: "file",
            path: entry.path,
            ignored: entry.ignored === true,
          }),
        );
      return [
        { label: "Chats", items: threadItems },
        { label: "Files", items: fileItems },
      ].filter((group) => group.items.length > 0);
    }
    const messageItems = (messageResults.data?.matches ?? []).map(
      (match): QuickSearchItem => ({
        key: `message:${match.threadId}:${match.messageId}`,
        kind: "message",
        match,
      }),
    );
    const fileMatchItems = (fileContentResults.data?.matches ?? [])
      .slice(0, FILE_CONTENT_DISPLAY_LIMIT)
      .map(
        (match): QuickSearchItem => ({
          key: `file-match:${match.path}:${match.line}:${match.matchStart}`,
          kind: "file-match",
          match,
        }),
      );
    return [
      { label: "Chats", items: messageItems },
      { label: "Files", items: fileMatchItems },
    ].filter((group) => group.items.length > 0);
  }, [
    debouncedQuery,
    fileContentResults.data,
    fileNameResults.data,
    messageResults.data,
    mode,
    threads,
  ]);

  const flatItems = useMemo(() => groups.flatMap((group) => group.items), [groups]);
  const selectedItem = flatItems[Math.min(selectedIndex, flatItems.length - 1)] ?? null;

  useEffect(() => {
    setSelectedIndex(0);
  }, [debouncedQuery, mode]);

  useEffect(() => {
    document
      .querySelector(`[data-quick-search-index="${selectedIndex}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  // Files open into the right panel, which is keyed by thread: the open
  // thread, else the draft page's pre-created thread (ChatView keys the
  // panel the same way on draft routes).
  const fileThreadRef =
    routeThreadRef ??
    (activeDraftThread
      ? scopeThreadRef(activeDraftThread.environmentId, activeDraftThread.threadId)
      : null);

  const activate = useCallback(
    (item: QuickSearchItem) => {
      if (item.kind === "thread") {
        close();
        void navigate({
          to: "/$environmentId/$threadId",
          params: buildThreadRouteParams(scopeThreadRef(item.thread.environmentId, item.thread.id)),
        });
        return;
      }
      if (item.kind === "message") {
        close();
        if (environmentId === null) return;
        void navigate({
          to: "/$environmentId/$threadId",
          params: buildThreadRouteParams(scopeThreadRef(environmentId, item.match.threadId)),
        });
        return;
      }
      // No thread surface to open the file into (home page): keep the dialog
      // open — its preview pane is the only viewer available.
      if (fileThreadRef === null) return;
      close();
      activatedFileRef.current = true;
      requestEditorFocus("quick-search");
      if (item.kind === "file") {
        useRightPanelStore.getState().openFile(fileThreadRef, item.path);
        return;
      }
      useRightPanelStore.getState().openFile(fileThreadRef, item.match.path, item.match.line);
    },
    [close, environmentId, fileThreadRef, navigate],
  );

  const onInputKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLInputElement>) => {
      if (event.key === "ArrowDown") {
        event.preventDefault();
        setSelectedIndex((index) => Math.min(index + 1, Math.max(0, flatItems.length - 1)));
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        setSelectedIndex((index) => Math.max(0, index - 1));
        return;
      }
      if (event.key === "Enter" && selectedItem !== null) {
        event.preventDefault();
        activate(selectedItem);
      }
    },
    [activate, flatItems.length, selectedItem],
  );

  const previewFile =
    selectedItem === null
      ? null
      : selectedItem.kind === "file"
        ? { path: selectedItem.path, line: null }
        : selectedItem.kind === "file-match"
          ? { path: selectedItem.match.path, line: selectedItem.match.line }
          : null;

  // Surface query failures instead of letting them masquerade as an empty
  // result set (a failed ripgrep spawn used to look identical to "No results").
  const searchError =
    debouncedQuery.length === 0
      ? null
      : mode === "content"
        ? (fileContentResults.error ?? messageResults.error)
        : fileNameResults.error;

  const searchPlaceholder =
    mode === "content" ? "Search chat and file contents…" : "Jump to a chat or file…";
  const openShortcutLabel = shortcutLabelForCommand(keybindings, "quickSearch.open");
  const contentShortcutLabel = shortcutLabelForCommand(keybindings, "quickSearch.content");

  // The dialog and preview pane size to what is being previewed: wide for
  // code excerpts, compact for metadata cards, list-only when there is
  // nothing to show.
  const previewKind: "file" | "card" | "none" =
    selectedItem === null
      ? "none"
      : selectedItem.kind === "thread" || selectedItem.kind === "message"
        ? "card"
        : previewFile !== null && environmentId !== null && cwd !== null
          ? "file"
          : "card";

  let itemIndex = -1;

  return (
    <Dialog open={open} onOpenChange={(nextOpen) => (nextOpen ? undefined : close())}>
      <DialogPortal>
        <DialogBackdrop />
        <DialogViewport className="pt-[12vh]">
          <DialogPrimitive.Popup
            className={cn(
              "row-start-2 flex h-[26rem] w-full min-w-0 flex-col overflow-hidden rounded-2xl border bg-popover text-popover-foreground shadow-lg transition-[max-width,scale,opacity] duration-200 data-ending-style:scale-98 data-starting-style:scale-98 data-ending-style:opacity-0 data-starting-style:opacity-0",
              previewKind === "file"
                ? "max-w-3xl"
                : previewKind === "card"
                  ? "max-w-2xl"
                  : "max-w-xl",
            )}
            data-quick-search="true"
            data-command-palette="true"
            aria-label={mode === "content" ? "Search chats and files" : "Quick open"}
            // When a file was opened, focus belongs to the editor; returning
            // false stops the dialog from restoring focus to the old element.
            finalFocus={() => (activatedFileRef.current ? false : undefined)}
          >
            <div className="flex items-center gap-2 border-b px-3">
              <SearchIcon className="size-4 shrink-0 text-muted-foreground" />
              <input
                ref={inputRef}
                autoFocus
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                onKeyDown={onInputKeyDown}
                placeholder={searchPlaceholder}
                className="h-11 min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
                spellCheck={false}
              />
              <div className="flex shrink-0 items-center gap-1 text-[10px] text-muted-foreground">
                <button
                  type="button"
                  className={cn(
                    "rounded px-1.5 py-0.5",
                    mode === "open" ? "bg-accent text-accent-foreground" : "hover:bg-accent/60",
                  )}
                  onClick={() => setMode("open")}
                >
                  Open{openShortcutLabel ? ` ${openShortcutLabel}` : ""}
                </button>
                <button
                  type="button"
                  className={cn(
                    "rounded px-1.5 py-0.5",
                    mode === "content" ? "bg-accent text-accent-foreground" : "hover:bg-accent/60",
                  )}
                  onClick={() => setMode("content")}
                >
                  Content{contentShortcutLabel ? ` ${contentShortcutLabel}` : ""}
                </button>
              </div>
            </div>
            <div className="flex min-h-0 flex-1">
              <ScrollArea className="min-w-0 flex-1">
                <div className="p-1.5">
                  {searchError !== null ? (
                    <div className="px-2 py-1.5 text-xs text-destructive">{searchError}</div>
                  ) : null}
                  {flatItems.length === 0 ? (
                    searchError !== null ? null : (
                      <div className="px-2 py-6 text-center text-xs text-muted-foreground">
                        {mode === "content" && trimmedQuery.length < 2
                          ? "Type at least 2 characters to search."
                          : environmentId === null
                            ? "Add a project to search."
                            : "No results."}
                      </div>
                    )
                  ) : (
                    groups.map((group) => (
                      <div key={group.label} className="mb-1">
                        <div className="px-2 py-1 text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                          {group.label}
                        </div>
                        {group.items.map((item) => {
                          itemIndex += 1;
                          const index = itemIndex;
                          const isSelected = index === selectedIndex;
                          return (
                            <button
                              key={item.key}
                              type="button"
                              data-quick-search-index={index}
                              className={cn(
                                "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs",
                                isSelected
                                  ? "bg-accent text-accent-foreground"
                                  : "hover:bg-accent/50",
                              )}
                              onMouseMove={() => setSelectedIndex(index)}
                              onClick={() => activate(item)}
                            >
                              {item.kind === "thread" || item.kind === "message" ? (
                                <MessageSquare className="size-3.5 shrink-0 text-muted-foreground" />
                              ) : (
                                <PierreFileIcon
                                  path={item.kind === "file" ? item.path : item.match.path}
                                  className="size-3.5 shrink-0 text-muted-foreground"
                                />
                              )}
                              {item.kind === "thread" ? (
                                <span className="flex min-w-0 flex-1 items-center gap-2">
                                  <HighlightedName
                                    text={item.thread.title}
                                    query={debouncedQuery}
                                  />
                                  <span className="ml-auto shrink-0 text-[10px] text-muted-foreground">
                                    {formatRelativeTime(item.thread.updatedAt)}
                                  </span>
                                </span>
                              ) : item.kind === "file" ? (
                                <FileRowContent
                                  path={item.path}
                                  query={debouncedQuery}
                                  ignored={item.ignored}
                                />
                              ) : item.kind === "file-match" ? (
                                <FileMatchRowContent match={item.match} />
                              ) : (
                                <span className="flex min-w-0 flex-1 flex-col">
                                  <span className="truncate font-medium">
                                    {item.match.threadTitle}
                                  </span>
                                  <span className="truncate text-muted-foreground">
                                    {item.match.snippet}
                                  </span>
                                </span>
                              )}
                            </button>
                          );
                        })}
                      </div>
                    ))
                  )}
                </div>
              </ScrollArea>
              <div
                className={cn(
                  "flex min-h-0 shrink-0 flex-col overflow-hidden bg-muted/30 transition-[width] duration-200",
                  previewKind === "file"
                    ? "w-[55%] border-l"
                    : previewKind === "card"
                      ? "w-72 border-l"
                      : "w-0",
                )}
              >
                {selectedItem === null ? null : selectedItem.kind === "thread" ? (
                  <ThreadPreview thread={selectedItem.thread} projectTitle={project?.title} />
                ) : selectedItem.kind === "message" ? (
                  <MessagePreview match={selectedItem.match} query={debouncedQuery} />
                ) : previewFile !== null && environmentId !== null && cwd !== null ? (
                  <FilePreview
                    environmentId={environmentId}
                    cwd={cwd}
                    relativePath={previewFile.path}
                    revealLine={previewFile.line}
                  />
                ) : (
                  <div className="flex flex-1 items-center justify-center p-3 text-center text-xs text-muted-foreground">
                    Open a chat to preview project files.
                  </div>
                )}
              </div>
            </div>
          </DialogPrimitive.Popup>
        </DialogViewport>
      </DialogPortal>
    </Dialog>
  );
}

function FileRowContent({
  path,
  query,
  ignored = false,
}: {
  path: string;
  query: string;
  ignored?: boolean;
}) {
  const { name, directory } = splitSearchResultPath(path);
  return (
    <span className={cn("flex min-w-0 flex-1 items-baseline gap-2", ignored ? "opacity-60" : null)}>
      <HighlightedName text={name} query={query} />
      {directory.length > 0 ? (
        <span className="truncate text-[10px] text-muted-foreground">{directory}</span>
      ) : null}
    </span>
  );
}

function FileMatchRowContent({ match }: { match: ProjectSearchContentMatch }) {
  const segments = matchLineSegments(match);
  const { name } = splitSearchResultPath(match.path);
  return (
    <span className="flex min-w-0 flex-1 flex-col">
      <span className="truncate font-medium">
        {name}
        <span className="ml-1 text-[10px] font-normal text-muted-foreground">:{match.line}</span>
      </span>
      <span className="truncate text-muted-foreground">
        {segments.beforeClipped ? "…" : ""}
        {segments.before}
        <span className="rounded-xs bg-amber-400/25 text-foreground">{segments.matched}</span>
        {segments.after}
      </span>
    </span>
  );
}

function ThreadPreview({
  thread,
  projectTitle,
}: {
  thread: EnvironmentThreadShell;
  projectTitle: string | undefined;
}) {
  return (
    <div className="flex flex-col gap-3 p-4 text-xs">
      <div className="text-sm font-medium">{thread.title}</div>
      <div className="flex flex-col gap-1.5 text-muted-foreground">
        {projectTitle !== undefined ? <div>Project: {projectTitle}</div> : null}
        {thread.branch !== null ? (
          <div className="flex items-center gap-1">
            <GitBranch className="size-3" />
            {thread.branch}
          </div>
        ) : null}
        <div>Updated {formatRelativeTime(thread.updatedAt)}</div>
        {thread.latestUserMessageAt !== null ? (
          <div>Last message {formatRelativeTime(thread.latestUserMessageAt)}</div>
        ) : null}
      </div>
    </div>
  );
}

function MessagePreview({
  match,
  query,
}: {
  match: OrchestrationMessageSearchMatch;
  query: string;
}) {
  const [before, matched, after] = nameSegments(match.snippet, query);
  return (
    <div className="flex flex-col gap-3 p-4 text-xs">
      <div className="text-sm font-medium">{match.threadTitle}</div>
      <div className="text-[10px] uppercase tracking-wide text-muted-foreground">
        {match.role} · {formatRelativeTime(match.updatedAt)}
      </div>
      <div className="whitespace-pre-wrap leading-5 text-muted-foreground">
        {before}
        {matched.length > 0 ? (
          <span className="rounded-xs bg-amber-400/25 text-foreground">{matched}</span>
        ) : null}
        {after}
      </div>
    </div>
  );
}
