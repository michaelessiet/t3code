import { MultiFileDiff } from "@pierre/diffs/react";
import { useMemo } from "react";

import { useTheme } from "~/hooks/useTheme";
import { resolveDiffThemeName } from "~/lib/diffRendering";
import { Button } from "~/components/ui/button";
import {
  Dialog,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogPopup,
  DialogTitle,
} from "~/components/ui/dialog";

import { fileContentRevision } from "./fileContentRevision";

interface FileConflictCompareDialogProps {
  relativePath: string;
  /** The file as it exists on disk after the external change. */
  diskContents: string;
  /** The disk read is incomplete (file exceeds the preview size cap). */
  diskTruncated: boolean;
  diskRevision: string | undefined;
  /** The unsaved local buffer. */
  bufferContents: string;
  onReloadFromDisk: () => void;
  onKeepBuffer: () => void;
  onClose: () => void;
}

/**
 * Read-only diff of the unsaved buffer against the on-disk contents so
 * resolving a concurrent-edit conflict isn't a blind choice. "Reload from
 * disk" adopts the left side; "Keep my version" writes the right side over
 * the disk state.
 */
export function FileConflictCompareDialog({
  relativePath,
  diskContents,
  diskTruncated,
  diskRevision,
  bufferContents,
  onReloadFromDisk,
  onKeepBuffer,
  onClose,
}: FileConflictCompareDialogProps) {
  const { resolvedTheme } = useTheme();
  const oldFile = useMemo(
    () => ({
      name: relativePath,
      contents: diskContents,
      cacheKey: `conflict-disk:${relativePath}:${diskRevision ?? fileContentRevision(diskContents)}`,
    }),
    [diskContents, diskRevision, relativePath],
  );
  const newFile = useMemo(
    () => ({
      name: relativePath,
      contents: bufferContents,
      cacheKey: `conflict-buffer:${relativePath}:${fileContentRevision(bufferContents)}`,
    }),
    [bufferContents, relativePath],
  );
  const identical = diskContents === bufferContents;

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogPopup className="flex max-h-[85vh] w-[min(64rem,92vw)] max-w-none flex-col">
        <DialogHeader>
          <DialogTitle>Compare with disk</DialogTitle>
          <DialogDescription>
            <code className="font-medium">{relativePath}</code> changed on disk while you were
            editing. Removed lines show what's on disk; added lines show your unsaved edits.
          </DialogDescription>
        </DialogHeader>
        <div className="min-h-0 flex-1 overflow-auto rounded-md border border-border/60">
          {diskTruncated ? (
            <div className="px-4 py-6 text-center text-xs text-muted-foreground">
              The on-disk file is too large to read completely, so the comparison would be
              misleading. Resolve the conflict with the actions below.
            </div>
          ) : identical ? (
            <div className="px-4 py-6 text-center text-xs text-muted-foreground">
              Your buffer and the on-disk contents are identical; either choice keeps the same text.
            </div>
          ) : (
            <MultiFileDiff
              oldFile={oldFile}
              newFile={newFile}
              options={{
                collapsed: false,
                diffStyle: "unified",
                disableFileHeader: true,
                theme: resolveDiffThemeName(resolvedTheme),
                themeType: resolvedTheme,
              }}
            />
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="outline" onClick={onReloadFromDisk}>
            Reload from disk
          </Button>
          <Button variant="default" onClick={onKeepBuffer}>
            Keep my version
          </Button>
        </DialogFooter>
      </DialogPopup>
    </Dialog>
  );
}
