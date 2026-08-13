import type { ScopedThreadRef } from "@t3tools/contracts";

import { workspaceRelativePath } from "./markdown-links";
import { useRightPanelStore } from "./rightPanelStore";
import { relativizeAgainstRoots, type ThreadRoot } from "./state/threadRoots";
import { resolvePathLinkTarget, splitPathAndPosition } from "./terminal-links";

interface OpenWorkspaceFilePrimaryActionInput {
  readonly threadRef: ScopedThreadRef | null;
  /** Raw path as surfaced in the UI; may be relative and carry `:line[:col]`. */
  readonly filePath: string;
  readonly workspaceRoot: string | undefined;
  /** Effective thread roots (primary first). Absolute paths inside an
      attached root open in the built-in editor scoped to that root. */
  readonly roots?: ReadonlyArray<ThreadRoot> | undefined;
  readonly openFilesInExternalEditor: boolean;
  readonly openInEditor: (targetPath: string) => void;
}

/**
 * Primary action for opening a file: the built-in editor unless the user
 * opted into a third-party editor in settings. Files the built-in editor
 * cannot show (no thread context, or outside every workspace root) still go
 * to the external editor.
 */
export function openWorkspaceFilePrimaryAction({
  threadRef,
  filePath,
  workspaceRoot,
  roots,
  openFilesInExternalEditor,
  openInEditor,
}: OpenWorkspaceFilePrimaryActionInput): void {
  const targetPath = workspaceRoot ? resolvePathLinkTarget(filePath, workspaceRoot) : filePath;

  if (!openFilesInExternalEditor && threadRef && workspaceRoot) {
    const { path, line, endLine } = splitPathAndPosition(targetPath);
    const parsedLine = line === undefined ? Number.NaN : Number.parseInt(line, 10);
    const parsedEndLine = endLine === undefined ? Number.NaN : Number.parseInt(endLine, 10);
    const revealLine = Number.isFinite(parsedLine) ? parsedLine : undefined;
    const revealEndLine = Number.isFinite(parsedEndLine) ? parsedEndLine : undefined;

    const match =
      roots !== undefined && roots.length > 0 ? relativizeAgainstRoots(path, roots) : null;
    if (match !== null && match.relativePath.length > 0) {
      useRightPanelStore
        .getState()
        .openFile(
          threadRef,
          match.relativePath,
          revealLine,
          revealEndLine,
          match.root.isPrimary ? undefined : { rootPath: match.root.path },
        );
      return;
    }
    if (match === null) {
      const relativePath = workspaceRelativePath(path, workspaceRoot);
      if (relativePath) {
        useRightPanelStore.getState().openFile(threadRef, relativePath, revealLine, revealEndLine);
        return;
      }
    }
  }

  openInEditor(targetPath);
}
