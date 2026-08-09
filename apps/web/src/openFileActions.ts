import type { ScopedThreadRef } from "@t3tools/contracts";

import { workspaceRelativePath } from "./markdown-links";
import { useRightPanelStore } from "./rightPanelStore";
import { resolvePathLinkTarget, splitPathAndPosition } from "./terminal-links";

interface OpenWorkspaceFilePrimaryActionInput {
  readonly threadRef: ScopedThreadRef | null;
  /** Raw path as surfaced in the UI; may be relative and carry `:line[:col]`. */
  readonly filePath: string;
  readonly workspaceRoot: string | undefined;
  readonly openFilesInExternalEditor: boolean;
  readonly openInEditor: (targetPath: string) => void;
}

/**
 * Primary action for opening a file: the built-in editor unless the user
 * opted into a third-party editor in settings. Files the built-in editor
 * cannot show (no thread context, or outside the workspace root) still go
 * to the external editor.
 */
export function openWorkspaceFilePrimaryAction({
  threadRef,
  filePath,
  workspaceRoot,
  openFilesInExternalEditor,
  openInEditor,
}: OpenWorkspaceFilePrimaryActionInput): void {
  const targetPath = workspaceRoot ? resolvePathLinkTarget(filePath, workspaceRoot) : filePath;

  if (!openFilesInExternalEditor && threadRef && workspaceRoot) {
    const { path, line, endLine } = splitPathAndPosition(targetPath);
    const relativePath = workspaceRelativePath(path, workspaceRoot);
    if (relativePath) {
      const parsedLine = line === undefined ? Number.NaN : Number.parseInt(line, 10);
      const parsedEndLine = endLine === undefined ? Number.NaN : Number.parseInt(endLine, 10);
      useRightPanelStore
        .getState()
        .openFile(
          threadRef,
          relativePath,
          Number.isFinite(parsedLine) ? parsedLine : undefined,
          Number.isFinite(parsedEndLine) ? parsedEndLine : undefined,
        );
      return;
    }
  }

  openInEditor(targetPath);
}
