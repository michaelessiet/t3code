import { DownloadIcon } from "lucide-react";
import { useSyncExternalStore } from "react";
import type { ClaudeBinaryInstallProgressStage } from "@t3tools/contracts";

import {
  completeClaudeBinaryInstallDialogClose,
  readClaudeBinaryInstallDialogState,
  respondToClaudeBinaryInstallConfirmation,
  subscribeClaudeBinaryInstallDialog,
} from "../../claudeBinaryInstallDialog";
import { Button } from "../ui/button";
import {
  Dialog,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogPanel,
  DialogPopup,
  DialogTitle,
} from "../ui/dialog";

const installSteps: ReadonlyArray<{
  readonly stage: ClaudeBinaryInstallProgressStage;
  readonly label: string;
}> = [
  { stage: "checking", label: "Checking current installation" },
  { stage: "waiting_for_lock", label: "Waiting for installer" },
  { stage: "downloading", label: "Downloading Claude Code" },
  { stage: "verifying", label: "Verifying download" },
  { stage: "installing", label: "Installing Claude Code" },
  { stage: "validating", label: "Validating executable" },
  { stage: "activating", label: "Activating installation" },
];

function formatMegabytes(bytes: number): string {
  return `${Math.max(1, Math.round(bytes / (1024 * 1024)))} MB`;
}

export function ClaudeBinaryInstallDialog() {
  const state = useSyncExternalStore(
    subscribeClaudeBinaryInstallDialog,
    readClaudeBinaryInstallDialogState,
    readClaudeBinaryInstallDialogState,
  );
  const view = state.status === "closing" ? state.view : state;
  const isConfirming = view.status === "confirming";
  const isInstalling = view.status === "installing";
  const activeStepIndex = isInstalling
    ? installSteps.findIndex(({ stage }) => stage === view.stage)
    : -1;
  const activeStep = installSteps[activeStepIndex];
  const downloadDetail =
    isInstalling && view.stage === "downloading" && view.bytesDownloaded !== undefined
      ? view.totalBytes !== undefined
        ? `${formatMegabytes(view.bytesDownloaded)} of ${formatMegabytes(view.totalBytes)}`
        : formatMegabytes(view.bytesDownloaded)
      : null;

  return (
    <Dialog
      open={state.status === "confirming" || state.status === "installing"}
      onOpenChange={(open) => {
        if (!open && isConfirming) {
          respondToClaudeBinaryInstallConfirmation(false);
        }
      }}
      onOpenChangeComplete={(open) => {
        if (!open) {
          completeClaudeBinaryInstallDialogClose();
        }
      }}
    >
      <DialogPopup className="max-w-md" showCloseButton={isConfirming}>
        <DialogHeader>
          <div className="flex size-9 items-center justify-center rounded-lg border border-border/70 bg-muted/60">
            <DownloadIcon aria-hidden className="size-4.5 text-muted-foreground" />
          </div>
          <DialogTitle>
            {isInstalling ? "Installing Claude Code" : "Install Claude Code?"}
          </DialogTitle>
          <DialogDescription>
            {isInstalling
              ? "T3 Code is downloading the Claude Code CLI so Claude threads can run in this environment."
              : "T3 Code needs the Claude Code CLI to run Claude threads, and it was not found on this machine."}
          </DialogDescription>
        </DialogHeader>
        <DialogPanel scrollFade={false}>
          {isInstalling ? (
            <div className="space-y-2.5">
              <div className="flex items-center justify-between gap-3 text-sm">
                <p aria-live="polite" className="font-medium text-foreground">
                  {activeStep?.label}
                </p>
                <p className="shrink-0 tabular-nums text-muted-foreground">
                  {downloadDetail ?? `${activeStepIndex + 1} of ${installSteps.length}`}
                </p>
              </div>
              <progress
                aria-label="Claude Code installation progress"
                className="h-2 w-full appearance-none overflow-hidden rounded-full bg-muted [&::-moz-progress-bar]:rounded-full [&::-moz-progress-bar]:bg-primary [&::-webkit-progress-bar]:rounded-full [&::-webkit-progress-bar]:bg-muted [&::-webkit-progress-value]:rounded-full [&::-webkit-progress-value]:bg-primary"
                {...(isInstalling &&
                view.stage === "downloading" &&
                view.bytesDownloaded !== undefined &&
                view.totalBytes !== undefined
                  ? { max: view.totalBytes, value: view.bytesDownloaded }
                  : { max: installSteps.length, value: activeStepIndex + 1 })}
              />
              <p className="text-xs leading-relaxed text-muted-foreground">
                Keep T3 Code open while Claude Code is installed.
              </p>
            </div>
          ) : (
            <div className="rounded-xl border border-border/70 bg-muted/35 p-3">
              <p className="text-sm font-medium text-foreground">Managed Claude Code CLI</p>
              <p className="mt-1 text-sm leading-relaxed text-muted-foreground">
                T3 Code will download and install Claude Code{" "}
                {view.status === "confirming" ? `v${view.version}` : ""} locally
                {view.status === "confirming" && view.binarySizeBytes > 0
                  ? ` (${formatMegabytes(view.binarySizeBytes)} on disk)`
                  : ""}
                . If you already have it, install it from claude.com/claude-code or set a binary
                path in the Claude provider settings instead.
              </p>
            </div>
          )}
        </DialogPanel>
        {isConfirming ? (
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => respondToClaudeBinaryInstallConfirmation(false)}
            >
              Cancel
            </Button>
            <Button onClick={() => respondToClaudeBinaryInstallConfirmation(true)}>
              Download and install
            </Button>
          </DialogFooter>
        ) : null}
      </DialogPopup>
    </Dialog>
  );
}
