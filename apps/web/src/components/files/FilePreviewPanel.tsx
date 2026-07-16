import type {
  EditorId,
  EnvironmentId,
  ResolvedKeybindingsConfig,
  ScopedThreadRef,
} from "@t3tools/contracts";
import { VirtualizedFile } from "@pierre/diffs";
import { File, type FileOptions, Virtualizer } from "@pierre/diffs/react";
import type { EditorView } from "@codemirror/view";
import {
  isAtomCommandInterrupted,
  squashAtomCommandFailure,
} from "@t3tools/client-runtime/state/runtime";
import { ChevronRight, Code2, Eye, FolderTree, Globe2, LoaderCircle } from "lucide-react";
import * as Option from "effect/Option";
import * as Schema from "effect/Schema";
import { AsyncResult } from "effect/unstable/reactivity";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { isBrowserPreviewFile, openFileInPreview } from "~/browser/openFileInPreview";
import ChatMarkdown from "~/components/ChatMarkdown";
import { OpenInPicker } from "~/components/chat/OpenInPicker";
import { useClientSettings } from "~/hooks/useSettings";
import { useTheme } from "~/hooks/useTheme";
import { getLocalStorageItem, setLocalStorageItem } from "~/hooks/useLocalStorage";
import { resolveDiffThemeName } from "~/lib/diffRendering";
import { cn } from "~/lib/utils";
import { isPreviewSupportedInRuntime } from "~/previewStateStore";
import { resolvePathLinkTarget } from "~/terminal-links";
import { ScrollArea } from "~/components/ui/scroll-area";
import { Toggle } from "~/components/ui/toggle";
import { Tooltip, TooltipPopup, TooltipTrigger } from "~/components/ui/tooltip";
import { stackedThreadToast, toastManager } from "~/components/ui/toast";
import { type DraftId, useComposerDraftStore } from "~/composerDraftStore";
import { buildFileReviewComment } from "~/reviewCommentContext";
import { assetEnvironment } from "~/state/assets";
import { useEnvironmentHttpBaseUrl, usePrimaryEnvironmentId } from "~/state/environments";
import { previewEnvironment } from "~/state/preview";
import { projectEnvironment } from "~/state/projects";
import { useAtomCommand } from "~/state/use-atom-command";
import { useAtomQueryRunner } from "~/state/use-atom-query-runner";

import FileBrowserPanel from "./FileBrowserPanel";
import { CodeMirrorFileEditor } from "./codemirror/CodeMirrorFileEditor";
import { useLspBridge } from "./codemirror/useLspBridge";
import {
  type ReviewAnnotationSpec,
  type ReviewLineRange,
  reviewCommentsExtension,
  setReviewAnnotations,
  setReviewSelection,
} from "./codemirror/reviewComments";
import {
  type FileCommentAnnotationEntry,
  type FileCommentLineAnnotation,
  fileCommentAnnotationGroupId,
  formatFileCommentRange,
  nextFileCommentId,
  normalizeFileCommentRange,
  remapFileCommentAnnotations,
} from "./fileCommentAnnotations";
import { LocalCommentAnnotation } from "./LocalCommentAnnotation";
import { projectFileCacheKey } from "./fileContentRevision";
import { fileBreadcrumbs } from "./filePath";
import { isMarkdownPreviewFile, setMarkdownTaskChecked } from "./filePreviewMode";
import {
  type FileBufferConflictReason,
  detectsExternalConflict,
  isStaleRevisionWriteFailure,
} from "./fileBufferConflict";
import { FileSaveCoordinator } from "./fileSaveCoordinator";
import {
  clearProjectFileQueryData,
  confirmProjectFileQueryData,
  getOptimisticProjectFileQueryData,
  setProjectFileQueryData,
  useProjectFileDiskRevision,
  useProjectFileQuery,
  useWorkspaceFileWatch,
} from "./projectFilesQueryState";

interface FilePreviewPanelProps {
  environmentId: EnvironmentId;
  cwd: string;
  projectName: string;
  relativePath: string | null;
  threadRef: ScopedThreadRef;
  composerDraftTarget: ScopedThreadRef | DraftId;
  keybindings: ResolvedKeybindingsConfig;
  availableEditors: ReadonlyArray<EditorId>;
  revealLine: number | null;
  revealRequestId: number;
  onOpenFile: (relativePath: string, line?: number) => void;
  onPendingChange: (relativePath: string, pending: boolean) => void;
}

const FILE_EXPLORER_STORAGE_KEY = "t3code.fileExplorerOpen";
const FILE_SAVE_DEBOUNCE_MS = 500;
const FILE_LINK_REVEAL_ATTRIBUTE = "data-file-link-reveal";
const FILE_LINK_REVEAL_UNSAFE_CSS = `
  [${FILE_LINK_REVEAL_ATTRIBUTE}][data-line] {
    background-color: light-dark(
      color-mix(
        in lab,
        var(--diffs-computed-diff-line-bg) 82%,
        var(--diffs-bg-selection-override, var(--diffs-selection-base))
      ),
      color-mix(
        in lab,
        var(--diffs-computed-diff-line-bg) 75%,
        var(--diffs-bg-selection-override, var(--diffs-selection-base))
      )
    ) !important;
  }

  [${FILE_LINK_REVEAL_ATTRIBUTE}][data-column-number] {
    background-color: light-dark(
      color-mix(
        in lab,
        var(--diffs-computed-diff-line-bg) 75%,
        var(--diffs-bg-selection-number-override, var(--diffs-selection-base))
      ),
      color-mix(
        in lab,
        var(--diffs-computed-diff-line-bg) 60%,
        var(--diffs-bg-selection-number-override, var(--diffs-selection-base))
      )
    ) !important;
    color: var(--diffs-selection-number-fg) !important;
  }
`;
type FilePostRender = NonNullable<FileOptions<unknown>["onPostRender"]>;

function clampFileLine(contents: string, requestedLine: number): number {
  let lineCount = 1;
  for (let index = 0; index < contents.length; index += 1) {
    const character = contents.charCodeAt(index);
    if (character === 10) {
      lineCount += 1;
    } else if (character === 13) {
      lineCount += 1;
      if (contents.charCodeAt(index + 1) === 10) index += 1;
    }
  }
  return Math.min(Math.max(1, requestedLine), lineCount);
}

function updateFileLinkReveal(fileContainer: HTMLElement, line: number | null): void {
  const root = fileContainer.shadowRoot ?? fileContainer;
  for (const element of root.querySelectorAll<HTMLElement>(`[${FILE_LINK_REVEAL_ATTRIBUTE}]`)) {
    element.removeAttribute(FILE_LINK_REVEAL_ATTRIBUTE);
  }
  if (line === null) return;

  root
    .querySelector<HTMLElement>(`[data-line="${line}"]`)
    ?.setAttribute(FILE_LINK_REVEAL_ATTRIBUTE, "");
  root
    .querySelector<HTMLElement>(`[data-column-number="${line}"]`)
    ?.setAttribute(FILE_LINK_REVEAL_ATTRIBUTE, "");
}

function useFileLineReveal(
  relativePath: string | null,
  revealLine: number | null,
  revealRequestId: number,
): FilePostRender {
  const [handledRequestIdsByPath] = useState(() => new Map<string, number>());
  const [latestRequestIdsByPath] = useState(() => new Map<string, number>());
  const [pendingFramesByPath] = useState(() => new Map<string, number>());

  return useCallback<FilePostRender>(
    (fileContainer, instance, phase) => {
      if (relativePath === null) return;

      const cancelPendingReveal = () => {
        const frameId = pendingFramesByPath.get(relativePath);
        if (frameId !== undefined) {
          cancelAnimationFrame(frameId);
          pendingFramesByPath.delete(relativePath);
        }
      };

      if (phase === "unmount") {
        cancelPendingReveal();
        return;
      }

      const targetLine =
        revealLine === null ? null : clampFileLine(instance.file?.contents ?? "", revealLine);
      updateFileLinkReveal(fileContainer, targetLine);

      if (!(instance instanceof VirtualizedFile)) return;

      if (latestRequestIdsByPath.get(relativePath) !== revealRequestId) {
        cancelPendingReveal();
        latestRequestIdsByPath.set(relativePath, revealRequestId);
      }

      if (targetLine === null) {
        fileContainer.style.minHeight = "";
        return;
      }

      const scrollContainer = fileContainer.closest<HTMLElement>(".file-preview-virtualizer");
      if (!scrollContainer) return;
      fileContainer.style.minHeight = `${Math.ceil(
        Math.max(instance.height, scrollContainer.clientHeight),
      )}px`;

      if (
        handledRequestIdsByPath.get(relativePath) === revealRequestId ||
        pendingFramesByPath.has(relativePath)
      ) {
        return;
      }

      const reveal = () => {
        pendingFramesByPath.delete(relativePath);
        if (
          latestRequestIdsByPath.get(relativePath) !== revealRequestId ||
          !fileContainer.isConnected
        ) {
          return;
        }

        const linePosition = instance.getLinePosition(targetLine);
        if (!linePosition) return;

        const fileTop =
          scrollContainer.scrollTop +
          fileContainer.getBoundingClientRect().top -
          scrollContainer.getBoundingClientRect().top;
        const centeredTop = Math.max(
          0,
          fileTop +
            linePosition.top -
            Math.max(0, (scrollContainer.clientHeight - linePosition.height) / 2),
        );
        const maxScrollTop = Math.max(
          0,
          scrollContainer.scrollHeight - scrollContainer.clientHeight,
        );

        scrollContainer.scrollTop = Math.min(centeredTop, maxScrollTop);
        handledRequestIdsByPath.set(relativePath, revealRequestId);
      };

      pendingFramesByPath.set(relativePath, requestAnimationFrame(reveal));
    },
    [
      handledRequestIdsByPath,
      latestRequestIdsByPath,
      pendingFramesByPath,
      relativePath,
      revealLine,
      revealRequestId,
    ],
  );
}

interface EditableFileSurfaceProps {
  environmentId: EnvironmentId;
  cwd: string;
  relativePath: string;
  composerDraftTarget: ScopedThreadRef | DraftId;
  contents: string;
  diskRevision: string | undefined;
  revealLine: number | null;
  revealRequestId: number;
  wordWrap: boolean;
  vimMode: boolean;
  onPendingChange: (relativePath: string, pending: boolean) => void;
  onRefreshFile: () => void;
  onOpenFile: (relativePath: string, line?: number) => void;
}

interface FileSelectionOverride {
  revealRequestId: number;
  range: ReviewLineRange | null;
}

interface FileSaveCoordination {
  readonly coordinator: FileSaveCoordinator;
  /** True while local edits are unsaved or a save is in flight. */
  readonly isDirty: () => boolean;
  /** Base revision the buffer's edits are relative to; null when unknown. */
  readonly baseRevision: () => string | null;
  /** Forget the buffer's base revision (e.g. before reloading from disk). */
  readonly resetBaseRevision: () => void;
  /** Write unconditionally (no base-revision guard), e.g. "keep my version". */
  readonly forcePersist: (contents: string) => Promise<boolean>;
}

function useFileSaveCoordinator({
  environmentId,
  cwd,
  relativePath,
  diskRevision,
  onPendingChange,
  onStaleSave,
}: Pick<EditableFileSurfaceProps, "environmentId" | "cwd" | "relativePath" | "onPendingChange"> & {
  diskRevision: string | undefined;
  onStaleSave?: () => void;
}): FileSaveCoordination {
  const writeFile = useAtomCommand(projectEnvironment.writeFile);
  const baseRevisionRef = useRef<string | null>(null);
  const pendingRef = useRef(false);
  const onStaleSaveRef = useRef(onStaleSave);
  useEffect(() => {
    onStaleSaveRef.current = onStaleSave;
  }, [onStaleSave]);

  // While the buffer is clean it follows the disk: whatever revision the
  // query last read is what future edits are based on.
  useEffect(() => {
    if (!pendingRef.current && diskRevision !== undefined) {
      baseRevisionRef.current = diskRevision;
    }
  }, [diskRevision]);

  const coordinator = useMemo(
    () =>
      new FileSaveCoordinator({
        debounceMs: FILE_SAVE_DEBOUNCE_MS,
        onPendingChange: (pending) => {
          pendingRef.current = pending;
          onPendingChange(relativePath, pending);
        },
        persist: async (nextContents) => {
          const baseRevision = baseRevisionRef.current;
          const result = await writeFile({
            environmentId,
            input: {
              cwd,
              relativePath,
              contents: nextContents,
              ...(baseRevision === null ? {} : { baseRevision }),
            },
          });
          if (result._tag === "Success") {
            baseRevisionRef.current = Option.getOrNull(AsyncResult.value(result))?.revision ?? null;
          } else if (isStaleRevisionWriteFailure(result)) {
            onStaleSaveRef.current?.();
          }
          return result;
        },
        onConfirmed: (confirmedContents) => {
          confirmProjectFileQueryData(environmentId, cwd, relativePath, confirmedContents);
        },
      }),
    [cwd, environmentId, onPendingChange, relativePath, writeFile],
  );

  const forcePersist = useCallback(
    async (nextContents: string) => {
      const result = await writeFile({
        environmentId,
        input: { cwd, relativePath, contents: nextContents },
      });
      if (result._tag !== "Success") return false;
      baseRevisionRef.current = Option.getOrNull(AsyncResult.value(result))?.revision ?? null;
      coordinator.reset();
      confirmProjectFileQueryData(environmentId, cwd, relativePath, nextContents);
      return true;
    },
    [coordinator, cwd, environmentId, relativePath, writeFile],
  );

  useEffect(() => () => coordinator.dispose(), [coordinator]);
  return useMemo(
    () => ({
      coordinator,
      isDirty: () => pendingRef.current,
      baseRevision: () => baseRevisionRef.current,
      resetBaseRevision: () => {
        baseRevisionRef.current = null;
      },
      forcePersist,
    }),
    [coordinator, forcePersist],
  );
}

function EditableFileSurface({
  environmentId,
  cwd,
  relativePath,
  composerDraftTarget,
  contents,
  diskRevision,
  revealLine,
  revealRequestId,
  wordWrap,
  vimMode,
  onPendingChange,
  onRefreshFile,
  onOpenFile,
}: EditableFileSurfaceProps) {
  const addReviewComment = useComposerDraftStore((store) => store.addReviewComment);
  const removeReviewComment = useComposerDraftStore((store) => store.removeReviewComment);
  const [lineAnnotations, setLineAnnotations] = useState<FileCommentLineAnnotation[]>([]);
  const [selectionOverride, setSelectionOverride] = useState<FileSelectionOverride | null>(null);
  const selectedRange =
    selectionOverride?.revealRequestId === revealRequestId ? selectionOverride.range : null;
  const revealRequestIdRef = useRef(revealRequestId);
  useLayoutEffect(() => {
    revealRequestIdRef.current = revealRequestId;
  });
  const setSelectedRange = useCallback((range: ReviewLineRange | null) => {
    setSelectionOverride({ revealRequestId: revealRequestIdRef.current, range });
  }, []);
  const surfaceRef = useRef<HTMLDivElement>(null);
  const [conflict, setConflict] = useState<FileBufferConflictReason | null>(null);
  const onStaleSave = useCallback(() => setConflict("stale-save"), []);
  const saveCoordination = useFileSaveCoordinator({
    environmentId,
    cwd,
    relativePath,
    diskRevision,
    onPendingChange,
    onStaleSave,
  });
  const saveCoordinator = saveCoordination.coordinator;

  // The workspace watcher refreshed the file query underneath a dirty
  // buffer: the disk no longer matches what these edits are based on.
  useEffect(() => {
    if (
      detectsExternalConflict({
        dirty: saveCoordination.isDirty(),
        baseRevision: saveCoordination.baseRevision(),
        diskRevision,
      })
    ) {
      setConflict("external-change");
    }
  }, [diskRevision, saveCoordination]);

  const resolveConflictByReloading = useCallback(() => {
    saveCoordinator.reset();
    saveCoordination.resetBaseRevision();
    clearProjectFileQueryData(environmentId, cwd, relativePath);
    setConflict(null);
    onRefreshFile();
  }, [cwd, environmentId, onRefreshFile, relativePath, saveCoordination, saveCoordinator]);

  const resolveConflictByKeepingBuffer = useCallback(() => {
    const bufferContents =
      getOptimisticProjectFileQueryData(environmentId, cwd, relativePath)?.contents ?? contents;
    void saveCoordination.forcePersist(bufferContents).then((persisted) => {
      if (persisted) {
        setConflict(null);
        return;
      }
      toastManager.add(
        stackedThreadToast({
          type: "error",
          title: "Unable to save file",
          description: "Keeping your version failed; the file was not written.",
        }),
      );
    });
  }, [contents, cwd, environmentId, relativePath, saveCoordination]);

  const [editorView, setEditorView] = useState<EditorView | null>(null);
  const editorViewRef = useRef<EditorView | null>(null);
  const handleViewReady = useCallback((view: EditorView | null) => {
    editorViewRef.current = view;
    setEditorView(view);
  }, []);

  const handleContentsChange = useCallback(
    (nextContents: string) => {
      setProjectFileQueryData(environmentId, cwd, relativePath, nextContents);
      saveCoordinator.change(nextContents);
    },
    [cwd, environmentId, relativePath, saveCoordinator],
  );

  // Document edits move annotation anchors inside the editor; mirror the new
  // line numbers into React state and refresh the composer's review comments
  // so their ranges and code excerpts track the buffer (the Pierre editor's
  // onChange annotation remap did the same).
  const lineAnnotationsRef = useRef(lineAnnotations);
  useLayoutEffect(() => {
    lineAnnotationsRef.current = lineAnnotations;
  });
  const composerDraftTargetRef = useRef(composerDraftTarget);
  useLayoutEffect(() => {
    composerDraftTargetRef.current = composerDraftTarget;
  });
  const handleAnnotationLinesChanged = useCallback(
    (annotationLines: ReadonlyArray<ReviewAnnotationSpec>) => {
      const lineByGroupId = new Map(
        annotationLines.map((annotationLine) => [annotationLine.id, annotationLine.lineNumber]),
      );
      const moved = lineAnnotationsRef.current.map((annotation) => {
        const nextLineNumber =
          lineByGroupId.get(fileCommentAnnotationGroupId(annotation)) ?? annotation.lineNumber;
        return nextLineNumber === annotation.lineNumber
          ? annotation
          : { ...annotation, lineNumber: nextLineNumber };
      });
      const remapped = remapFileCommentAnnotations(moved);
      setLineAnnotations(remapped);
      const bufferContents = editorViewRef.current?.state.doc.toString();
      if (bufferContents === undefined) return;
      for (const annotation of remapped) {
        for (const entry of annotation.metadata.entries) {
          if (entry.kind !== "comment") continue;
          addReviewComment(
            composerDraftTargetRef.current,
            buildFileReviewComment({
              id: entry.id,
              filePath: relativePath,
              startLine: entry.startLine,
              endLine: entry.endLine,
              text: entry.text,
              contents: bufferContents,
            }),
          );
        }
      }
    },
    [addReviewComment, relativePath],
  );

  const removeAnnotationEntry = useCallback(
    (entryId: string) => {
      setSelectedRange(null);
      removeReviewComment(composerDraftTarget, entryId);
      setLineAnnotations((current) => {
        return current.flatMap((annotation) => {
          const entries = annotation.metadata.entries.filter((entry) => entry.id !== entryId);
          return entries.length > 0 ? [{ ...annotation, metadata: { entries } }] : [];
        });
      });
    },
    [composerDraftTarget, removeReviewComment, setSelectedRange],
  );

  const submitAnnotationEntry = useCallback(
    (entryId: string, text: string) => {
      setSelectedRange(null);
      const entry = lineAnnotations
        .flatMap((annotation) => annotation.metadata.entries)
        .find((candidate) => candidate.id === entryId);
      if (entry) {
        addReviewComment(
          composerDraftTarget,
          buildFileReviewComment({
            id: entry.id,
            filePath: relativePath,
            startLine: entry.startLine,
            endLine: entry.endLine,
            text,
            contents,
          }),
        );
      }
      setLineAnnotations((current) =>
        current.map((annotation) => ({
          ...annotation,
          metadata: {
            entries: annotation.metadata.entries.map((annotationEntry) =>
              annotationEntry.id === entryId
                ? { ...annotationEntry, kind: "comment", text }
                : annotationEntry,
            ),
          },
        })),
      );
    },
    [
      addReviewComment,
      composerDraftTarget,
      contents,
      lineAnnotations,
      relativePath,
      setSelectedRange,
    ],
  );

  const beginComment = useCallback((range: ReviewLineRange) => {
    const { startLine, endLine } = normalizeFileCommentRange(range);
    const draftEntry: FileCommentAnnotationEntry = {
      id: nextFileCommentId(),
      kind: "draft",
      startLine,
      endLine,
      text: "",
    };
    setLineAnnotations((current) => {
      const withoutDraft = current.flatMap((annotation) => {
        const entries = annotation.metadata.entries.filter((entry) => entry.kind !== "draft");
        return entries.length > 0 ? [{ ...annotation, metadata: { entries } }] : [];
      });
      const existingIndex = withoutDraft.findIndex(
        (annotation) => annotation.lineNumber === endLine,
      );
      if (existingIndex < 0) {
        return [
          ...withoutDraft,
          {
            lineNumber: endLine,
            metadata: { entries: [draftEntry] },
          },
        ];
      }
      return withoutDraft.map((annotation, index) =>
        index === existingIndex
          ? {
              ...annotation,
              metadata: { entries: [...annotation.metadata.entries, draftEntry] },
            }
          : annotation,
      );
    });
  }, []);
  const hasOpenCommentForm = lineAnnotations.some((annotation) =>
    annotation.metadata.entries.some((entry) => entry.kind === "draft"),
  );
  const hasOpenCommentFormRef = useRef(hasOpenCommentForm);
  useLayoutEffect(() => {
    hasOpenCommentFormRef.current = hasOpenCommentForm;
  });

  // Clicking outside the surface dismisses the current line selection unless
  // a draft comment form is open (matching the Pierre-based surface).
  useEffect(() => {
    const root = surfaceRef.current;
    if (!root) return;
    const handlePointerDown = (event: PointerEvent) => {
      if (hasOpenCommentFormRef.current || event.composedPath().includes(root)) return;
      setSelectedRange(null);
    };
    document.addEventListener("pointerdown", handlePointerDown, true);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown, true);
    };
  }, [setSelectedRange]);

  const handleLineSelectionEnd = useCallback(
    (range: ReviewLineRange | null) => {
      setSelectedRange(range);
      if (range) {
        beginComment(range);
      }
    },
    [beginComment, setSelectedRange],
  );

  // Detached containers the annotation UI renders into via portals; the
  // review-comments extension mounts them below their anchored lines.
  const annotationContainersRef = useRef(new Map<string, HTMLElement>());
  const getAnnotationContainer = useCallback((groupId: string) => {
    const existing = annotationContainersRef.current.get(groupId);
    if (existing) return existing;
    const container = document.createElement("div");
    annotationContainersRef.current.set(groupId, container);
    return container;
  }, []);

  const reviewExtension = useMemo(
    () =>
      reviewCommentsExtension({
        isSelectionEnabled: () => !hasOpenCommentFormRef.current,
        onSelectionChange: setSelectedRange,
        onSelectionEnd: handleLineSelectionEnd,
        getAnnotationContainer,
        onAnnotationLinesChanged: handleAnnotationLinesChanged,
      }),
    [
      getAnnotationContainer,
      handleAnnotationLinesChanged,
      handleLineSelectionEnd,
      setSelectedRange,
    ],
  );

  const onOpenFileAtLine = useCallback(
    (targetRelativePath: string, line: number) => onOpenFile(targetRelativePath, line),
    [onOpenFile],
  );
  const lspExtension = useLspBridge({
    environmentId,
    cwd,
    relativePath,
    view: editorView,
    onOpenFileAtLine,
  });
  const editorExtensions = useMemo(
    () => [reviewExtension, lspExtension ?? []],
    [lspExtension, reviewExtension],
  );

  useEffect(() => {
    if (editorView === null) return;
    setReviewSelection(editorView, selectedRange);
  }, [editorView, selectedRange]);

  useEffect(() => {
    if (editorView === null) return;
    const specs = lineAnnotations.map((annotation) => ({
      id: fileCommentAnnotationGroupId(annotation),
      lineNumber: annotation.lineNumber,
    }));
    setReviewAnnotations(editorView, specs);
    const liveGroupIds = new Set(specs.map((spec) => spec.id));
    for (const groupId of annotationContainersRef.current.keys()) {
      if (!liveGroupIds.has(groupId)) annotationContainersRef.current.delete(groupId);
    }
  }, [editorView, lineAnnotations]);

  return (
    <div ref={surfaceRef} className="flex min-h-0 flex-1 flex-col">
      {conflict !== null ? (
        <div
          className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-amber-500/20 bg-amber-500/8 px-3 py-1.5 text-[11px] text-amber-700 dark:text-amber-300"
          data-file-conflict-banner
        >
          <span className="min-w-0 flex-1">
            This file changed on disk while you were editing. Your edits are not being saved.
          </span>
          <button
            type="button"
            className="shrink-0 font-medium underline underline-offset-2 hover:opacity-80"
            onClick={resolveConflictByReloading}
          >
            Reload from disk
          </button>
          <button
            type="button"
            className="shrink-0 font-medium underline underline-offset-2 hover:opacity-80"
            onClick={resolveConflictByKeepingBuffer}
          >
            Keep my version
          </button>
        </div>
      ) : null}
      <CodeMirrorFileEditor
        className="min-h-0 flex-1 overflow-hidden"
        relativePath={relativePath}
        contents={contents}
        wordWrap={wordWrap}
        vimMode={vimMode}
        revealLine={revealLine}
        revealRequestId={revealRequestId}
        extensions={editorExtensions}
        onContentsChange={handleContentsChange}
        onViewReady={handleViewReady}
      />
      {lineAnnotations.map((annotation) => {
        const groupId = fileCommentAnnotationGroupId(annotation);
        return createPortal(
          <div className="py-1">
            {annotation.metadata.entries.map((entry) => (
              <LocalCommentAnnotation
                key={entry.id}
                kind={entry.kind}
                rangeLabel={formatFileCommentRange(entry.startLine, entry.endLine)}
                text={entry.text}
                onCancel={() => removeAnnotationEntry(entry.id)}
                onComment={(text) => submitAnnotationEntry(entry.id, text)}
                onDelete={() => removeAnnotationEntry(entry.id)}
              />
            ))}
          </div>,
          getAnnotationContainer(groupId),
          groupId,
        );
      })}
    </div>
  );
}

function RenderedMarkdownSurface({
  environmentId,
  cwd,
  relativePath,
  contents,
  diskRevision,
  threadRef,
  onPendingChange,
  onRefreshFile,
}: Omit<
  EditableFileSurfaceProps,
  "composerDraftTarget" | "revealLine" | "revealRequestId" | "wordWrap" | "vimMode" | "onOpenFile"
> & {
  threadRef: ScopedThreadRef;
}) {
  // Rendered-markdown edits are single checkbox toggles; on a concurrent-edit
  // conflict the safest resolution is reloading the disk state and letting
  // the user re-apply the toggle.
  const coordinationRef = useRef<FileSaveCoordination | null>(null);
  const onStaleSave = useCallback(() => {
    coordinationRef.current?.coordinator.reset();
    coordinationRef.current?.resetBaseRevision();
    clearProjectFileQueryData(environmentId, cwd, relativePath);
    onRefreshFile();
    toastManager.add(
      stackedThreadToast({
        type: "error",
        title: "File changed on disk",
        description: "Reloaded the latest contents; your last change was not saved.",
      }),
    );
  }, [cwd, environmentId, onRefreshFile, relativePath]);
  const saveCoordination = useFileSaveCoordinator({
    environmentId,
    cwd,
    relativePath,
    diskRevision,
    onPendingChange,
    onStaleSave,
  });
  useEffect(() => {
    coordinationRef.current = saveCoordination;
  }, [saveCoordination]);
  const saveCoordinator = saveCoordination.coordinator;

  return (
    <ScrollArea className="min-h-0 flex-1">
      <ChatMarkdown
        text={contents}
        cwd={cwd}
        threadRef={threadRef}
        className="mx-auto max-w-4xl px-6 py-5"
        onTaskListChange={({ markerOffset, checked }) => {
          const currentContents =
            getOptimisticProjectFileQueryData(environmentId, cwd, relativePath)?.contents ??
            contents;
          const nextContents = setMarkdownTaskChecked(currentContents, markerOffset, checked);
          if (nextContents === currentContents) return;
          setProjectFileQueryData(environmentId, cwd, relativePath, nextContents);
          saveCoordinator.change(nextContents);
        }}
      />
    </ScrollArea>
  );
}

function initialExplorerOpen(): boolean {
  try {
    return getLocalStorageItem(FILE_EXPLORER_STORAGE_KEY, Schema.Boolean) ?? true;
  } catch (error) {
    console.error(error);
    return true;
  }
}

export default function FilePreviewPanel({
  environmentId,
  cwd,
  projectName,
  relativePath,
  threadRef,
  composerDraftTarget,
  keybindings,
  availableEditors,
  revealLine,
  revealRequestId,
  onOpenFile,
  onPendingChange,
}: FilePreviewPanelProps) {
  const { resolvedTheme } = useTheme();
  const wordWrap = useClientSettings((settings) => settings.wordWrap);
  const vimMode = useClientSettings((settings) => settings.vimMode);
  const primaryEnvironmentId = usePrimaryEnvironmentId();
  const environmentHttpBaseUrl = useEnvironmentHttpBaseUrl(environmentId);
  const createAssetUrl = useAtomQueryRunner(assetEnvironment.createUrl, {
    reportFailure: false,
  });
  const openPreview = useAtomCommand(previewEnvironment.open, {
    reportFailure: false,
  });
  const file = useProjectFileQuery(environmentId, cwd, relativePath);
  const diskRevision = useProjectFileDiskRevision(environmentId, cwd, relativePath);
  useWorkspaceFileWatch(environmentId, cwd, relativePath, file.refresh);
  const [explorerOpen, setExplorerOpen] = useState(initialExplorerOpen);
  const [markdownView, setMarkdownView] = useState<{
    path: string | null;
    revealRequestId: number | null;
  }>({ path: null, revealRequestId: null });
  const breadcrumbRef = useRef<HTMLDivElement>(null);
  const isMarkdown = relativePath ? isMarkdownPreviewFile(relativePath) : false;
  const renderMarkdown =
    isMarkdown &&
    markdownView.path === relativePath &&
    (revealLine === null || markdownView.revealRequestId === revealRequestId);
  const canOpenInBrowser =
    relativePath !== null && isPreviewSupportedInRuntime() && isBrowserPreviewFile(relativePath);
  const absolutePath = relativePath ? resolvePathLinkTarget(relativePath, cwd) : null;
  const breadcrumbs = useMemo(
    () => (relativePath ? fileBreadcrumbs(projectName, relativePath) : []),
    [projectName, relativePath],
  );
  const onFilePostRender = useFileLineReveal(relativePath, revealLine, revealRequestId);

  useEffect(() => {
    const currentCrumb = breadcrumbRef.current?.querySelector<HTMLElement>(
      "[data-current-file-crumb='true']",
    );
    currentCrumb?.scrollIntoView({ block: "nearest", inline: "end" });
  }, [relativePath]);

  const toggleExplorer = () => {
    setExplorerOpen((current) => {
      const next = !current;
      try {
        setLocalStorageItem(FILE_EXPLORER_STORAGE_KEY, next, Schema.Boolean);
      } catch (error) {
        console.error(error);
      }
      return next;
    });
  };

  const handleOpenInBrowser = useCallback(() => {
    if (!absolutePath || !environmentHttpBaseUrl) return;
    void (async () => {
      const result = await openFileInPreview({
        threadRef,
        filePath: absolutePath,
        httpBaseUrl: environmentHttpBaseUrl,
        createAssetUrl,
        openPreview,
      });
      if (result._tag === "Success" || isAtomCommandInterrupted(result)) {
        return;
      }
      const error = squashAtomCommandFailure(result);
      toastManager.add(
        stackedThreadToast({
          type: "error",
          title: "Unable to open file in browser",
          description: error instanceof Error ? error.message : "An error occurred.",
        }),
      );
    })();
  }, [absolutePath, createAssetUrl, environmentHttpBaseUrl, openPreview, threadRef]);

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden bg-background">
      {relativePath ? (
        <div className="surface-subheader gap-2 px-3" data-surface-subheader>
          <ScrollArea
            ref={breadcrumbRef}
            hideScrollbars
            scrollFade
            className="min-w-0 flex-1 rounded-none"
            data-file-breadcrumbs
          >
            <div className="flex h-full w-max min-w-full items-center text-xs">
              {breadcrumbs.map((crumb, index) => (
                <div
                  key={crumb.path || "project"}
                  className="flex min-w-0 shrink-0 items-center"
                  data-current-file-crumb={crumb.kind === "file"}
                >
                  {index > 0 ? (
                    <ChevronRight className="mx-1 size-3.5 shrink-0 text-muted-foreground/60" />
                  ) : null}
                  <span
                    className={cn(
                      "max-w-40 truncate",
                      crumb.kind === "file"
                        ? "font-medium text-foreground"
                        : "text-muted-foreground",
                    )}
                    title={crumb.path || projectName}
                  >
                    {crumb.label}
                  </span>
                </div>
              ))}
            </div>
          </ScrollArea>
          {absolutePath && environmentId === primaryEnvironmentId ? (
            <OpenInPicker
              environmentId={environmentId}
              keybindings={keybindings}
              availableEditors={availableEditors}
              openInCwd={absolutePath}
              compact
              enableShortcut={false}
            />
          ) : null}
          {isMarkdown ? (
            <Tooltip>
              <TooltipTrigger
                render={
                  <Toggle
                    className="shrink-0"
                    pressed={renderMarkdown}
                    onPressedChange={(pressed) => {
                      setMarkdownView({
                        path: pressed ? relativePath : null,
                        revealRequestId: pressed ? revealRequestId : null,
                      });
                    }}
                    aria-label={renderMarkdown ? "Show markdown source" : "Show rendered markdown"}
                    variant="ghost"
                    size="sm"
                  >
                    {renderMarkdown ? <Code2 className="size-3.5" /> : <Eye className="size-3.5" />}
                  </Toggle>
                }
              />
              <TooltipPopup>
                {renderMarkdown ? "Show markdown source" : "Show rendered markdown"}
              </TooltipPopup>
            </Tooltip>
          ) : null}
          {canOpenInBrowser ? (
            <Tooltip>
              <TooltipTrigger
                render={
                  <Toggle
                    className="shrink-0"
                    pressed={false}
                    onPressedChange={handleOpenInBrowser}
                    aria-label="Open file in preview browser"
                    variant="ghost"
                    size="sm"
                  >
                    <Globe2 className="size-3.5" />
                  </Toggle>
                }
              />
              <TooltipPopup>Open file in preview browser</TooltipPopup>
            </Tooltip>
          ) : null}
          <Tooltip>
            <TooltipTrigger
              render={
                <Toggle
                  className="shrink-0"
                  pressed={explorerOpen}
                  onPressedChange={toggleExplorer}
                  aria-label={explorerOpen ? "Hide file explorer" : "Show file explorer"}
                  variant="ghost"
                  size="sm"
                >
                  <FolderTree className="size-3.5" />
                </Toggle>
              }
            />
            <TooltipPopup>
              {explorerOpen ? "Hide file explorer" : "Show file explorer"}
            </TooltipPopup>
          </Tooltip>
        </div>
      ) : null}
      {relativePath && file.data?.truncated ? (
        <div className="shrink-0 border-b border-amber-500/20 bg-amber-500/8 px-3 py-1.5 text-[11px] text-amber-700 dark:text-amber-300">
          Preview limited to the first 1 MB of a {file.data.byteLength.toLocaleString()} byte file.
        </div>
      ) : null}
      <div className="flex min-h-0 flex-1 overflow-hidden">
        <div
          className={cn(
            "min-w-0 flex-1 flex-col overflow-hidden",
            relativePath ? "flex" : "hidden",
          )}
        >
          {relativePath && file.error && file.data === null ? (
            <div className="flex min-h-0 flex-1 items-center justify-center px-6 text-center text-xs leading-relaxed text-destructive">
              {file.error}
            </div>
          ) : relativePath && file.data === null ? (
            <div className="flex min-h-0 flex-1 items-center justify-center text-muted-foreground">
              <LoaderCircle className="size-5 animate-spin" />
            </div>
          ) : relativePath && file.data ? (
            isMarkdown && renderMarkdown ? (
              <RenderedMarkdownSurface
                environmentId={environmentId}
                cwd={cwd}
                relativePath={relativePath}
                threadRef={threadRef}
                contents={file.data.contents}
                diskRevision={diskRevision}
                onPendingChange={onPendingChange}
                onRefreshFile={file.refresh}
              />
            ) : file.data.truncated ? (
              <Virtualizer
                key={`${relativePath}:${resolvedTheme}:${file.data.byteLength}`}
                className="file-preview-virtualizer min-h-0 flex-1 overflow-auto"
                config={{
                  overscrollSize: 600,
                  intersectionObserverMargin: 1200,
                }}
              >
                <File
                  file={{
                    name: relativePath,
                    contents: file.data.contents,
                    cacheKey: projectFileCacheKey(cwd, relativePath, file.data.contents),
                  }}
                  options={{
                    disableFileHeader: true,
                    overflow: wordWrap ? "wrap" : "scroll",
                    theme: resolveDiffThemeName(resolvedTheme),
                    themeType: resolvedTheme,
                    unsafeCSS: FILE_LINK_REVEAL_UNSAFE_CSS,
                    onPostRender: onFilePostRender,
                  }}
                  className="min-h-full"
                />
              </Virtualizer>
            ) : (
              <EditableFileSurface
                key={relativePath}
                environmentId={environmentId}
                cwd={cwd}
                relativePath={relativePath}
                composerDraftTarget={composerDraftTarget}
                contents={file.data.contents}
                diskRevision={diskRevision}
                revealLine={revealLine}
                revealRequestId={revealRequestId}
                wordWrap={wordWrap}
                vimMode={vimMode}
                onPendingChange={onPendingChange}
                onRefreshFile={file.refresh}
                onOpenFile={onOpenFile}
              />
            )
          ) : null}
        </div>
        {explorerOpen || relativePath === null ? (
          <aside
            className={cn(
              "flex min-h-0 shrink-0 bg-background",
              relativePath
                ? "w-[min(22rem,46%)] min-w-64 border-l border-border/60"
                : "min-w-0 flex-1",
            )}
          >
            <FileBrowserPanel
              key={`${environmentId}:${cwd}`}
              environmentId={environmentId}
              cwd={cwd}
              projectName={projectName}
              onOpenFile={onOpenFile}
            />
          </aside>
        ) : null}
      </div>
    </div>
  );
}
