import type { EnvironmentId } from "@t3tools/contracts";
import { useState } from "react";

import {
  AlertDialog,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogPopup,
  AlertDialogTitle,
} from "~/components/ui/alert-dialog";
import { Button } from "~/components/ui/button";
import { stackedThreadToast, toastManager } from "~/components/ui/toast";
import { projectEnvironment } from "~/state/projects";
import { useAtomCommand } from "~/state/use-atom-command";

import { getActiveFileSave } from "./fileSaveBus";
import { persistUnsavedBuffer } from "./fileSaveState";
import { clearProjectFileQueryData } from "./projectFilesQueryState";

interface UnsavedChangesDialogProps {
  environmentId: EnvironmentId;
  cwd: string;
  /** Dirty files among the tabs being closed. */
  dirtyFilePaths: ReadonlyArray<string>;
  /** Clear a file's dirty-tab indicator after it was saved or discarded. */
  onResolvePending: (relativePath: string) => void;
  /** Run the close that was intercepted. */
  onProceed: () => void;
  onCancel: () => void;
}

/**
 * Save / Discard / Cancel prompt shown when closing file tabs with unsaved
 * edits while autosave is off. The mounted editor saves through its own
 * coordinator (single-flight with any in-flight write); background tabs save
 * straight from their surviving optimistic buffers.
 */
export function UnsavedChangesDialog({
  environmentId,
  cwd,
  dirtyFilePaths,
  onResolvePending,
  onProceed,
  onCancel,
}: UnsavedChangesDialogProps) {
  const writeFile = useAtomCommand(projectEnvironment.writeFile);
  const [busy, setBusy] = useState(false);

  const failClose = (relativePath: string, stale: boolean) => {
    toastManager.add(
      stackedThreadToast({
        type: "error",
        title: stale ? "File changed on disk" : "Unable to save file",
        description: stale
          ? `${relativePath} changed on disk since your edits. Resolve the conflict in the editor before closing.`
          : `${relativePath} could not be written; the tab stays open.`,
      }),
    );
    onCancel();
  };

  const handleSave = async () => {
    setBusy(true);
    try {
      for (const relativePath of dirtyFilePaths) {
        const mounted = getActiveFileSave();
        if (mounted?.relativePath === relativePath) {
          await mounted.flush();
          if (mounted.isDirty()) {
            failClose(relativePath, false);
            return;
          }
          continue;
        }
        const outcome = await persistUnsavedBuffer(environmentId, cwd, relativePath, writeFile);
        if (outcome === "stale" || outcome === "error") {
          failClose(relativePath, outcome === "stale");
          return;
        }
        onResolvePending(relativePath);
      }
      onProceed();
    } finally {
      setBusy(false);
    }
  };

  const handleDiscard = () => {
    for (const relativePath of dirtyFilePaths) {
      clearProjectFileQueryData(environmentId, cwd, relativePath);
      onResolvePending(relativePath);
    }
    onProceed();
  };

  return (
    <AlertDialog open onOpenChange={(open) => !open && onCancel()}>
      <AlertDialogPopup>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {dirtyFilePaths.length === 1 ? "Unsaved changes" : "Unsaved changes in multiple files"}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {dirtyFilePaths.length === 1 ? (
              <>
                <code className="font-medium">{dirtyFilePaths[0]}</code> has unsaved changes that
                will be lost if you close it without saving.
              </>
            ) : (
              <>
                These files have unsaved changes that will be lost if you close them without saving:
                <span className="mt-1.5 block space-y-0.5">
                  {dirtyFilePaths.map((relativePath) => (
                    <code key={relativePath} className="block font-medium">
                      {relativePath}
                    </code>
                  ))}
                </span>
              </>
            )}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <Button variant="outline" disabled={busy} onClick={onCancel}>
            Cancel
          </Button>
          <Button variant="destructive" disabled={busy} onClick={handleDiscard}>
            Discard
          </Button>
          <Button variant="default" disabled={busy} onClick={() => void handleSave()}>
            Save
          </Button>
        </AlertDialogFooter>
      </AlertDialogPopup>
    </AlertDialog>
  );
}
