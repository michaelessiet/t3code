import type { ScopedThreadRef } from "@t3tools/contracts";

import { useRightPanelStore } from "./rightPanelStore";
import { resolvePathLinkTarget } from "./terminal-links";

interface OpenDiffFilePrimaryActionInput {
  readonly threadRef: ScopedThreadRef | null;
  readonly filePath: string;
  readonly activeCwd: string | undefined;
  /** Workspace root the diff was produced in; null = the thread's primary. */
  readonly rootPath?: string | null;
  readonly openFilesInExternalEditor?: boolean;
  readonly openInEditor: (targetPath: string) => void;
}

export function openDiffFilePrimaryAction({
  threadRef,
  filePath,
  activeCwd,
  rootPath = null,
  openFilesInExternalEditor = false,
  openInEditor,
}: OpenDiffFilePrimaryActionInput): void {
  if (threadRef && !openFilesInExternalEditor) {
    useRightPanelStore
      .getState()
      .openFile(
        threadRef,
        filePath,
        undefined,
        undefined,
        rootPath !== null ? { rootPath } : undefined,
      );
    return;
  }

  openInEditor(activeCwd ? resolvePathLinkTarget(filePath, activeCwd) : filePath);
}
