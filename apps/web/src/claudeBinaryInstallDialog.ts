import {
  ClaudeBinaryInstallProgressStageSchema,
  type ClaudeBinaryInstallProgressEvent,
  type ClaudeBinaryInstallProgressStage,
} from "@t3tools/contracts";
import * as Schema from "effect/Schema";

export class ClaudeBinaryInstallConfirmationConflictError extends Schema.TaggedErrorClass<ClaudeBinaryInstallConfirmationConflictError>()(
  "ClaudeBinaryInstallConfirmationConflictError",
  {
    requestedVersion: Schema.String,
    activeVersion: Schema.String,
    activeDialogStatus: Schema.Literals(["confirming", "installing", "closing"]),
    activeInstallStage: Schema.optional(ClaudeBinaryInstallProgressStageSchema),
  },
) {
  override get message(): string {
    return `Cannot confirm Claude Code installation ${this.requestedVersion}; installation ${this.activeVersion} has dialog status ${this.activeDialogStatus}.`;
  }
}

interface InstallingView {
  readonly status: "installing";
  readonly version: string;
  readonly stage: ClaudeBinaryInstallProgressStage;
  readonly bytesDownloaded?: number;
  readonly totalBytes?: number;
}

export type ClaudeBinaryInstallDialogState =
  | { readonly status: "idle" }
  | { readonly status: "confirming"; readonly version: string; readonly binarySizeBytes: number }
  | InstallingView
  | {
      readonly status: "closing";
      readonly view:
        | {
            readonly status: "confirming";
            readonly version: string;
            readonly binarySizeBytes: number;
          }
        | InstallingView;
    };

const idleState: ClaudeBinaryInstallDialogState = { status: "idle" };
let state: ClaudeBinaryInstallDialogState = idleState;
let resolveConfirmation: ((confirmed: boolean) => void) | null = null;
const listeners = new Set<() => void>();

function publish(next: ClaudeBinaryInstallDialogState) {
  state = next;
  for (const listener of listeners) {
    listener();
  }
}

export function readClaudeBinaryInstallDialogState(): ClaudeBinaryInstallDialogState {
  return state;
}

export function subscribeClaudeBinaryInstallDialog(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function requestClaudeBinaryInstallConfirmation(input: {
  readonly version: string;
  readonly binarySizeBytes: number;
}): Promise<boolean> {
  if (state.status !== "idle") {
    const activeInstall = state.status === "closing" ? state.view : state;
    return Promise.reject(
      new ClaudeBinaryInstallConfirmationConflictError({
        requestedVersion: input.version,
        activeVersion: activeInstall.version,
        activeDialogStatus: state.status,
        ...(activeInstall.status === "installing"
          ? { activeInstallStage: activeInstall.stage }
          : {}),
      }),
    );
  }

  publish({ status: "confirming", version: input.version, binarySizeBytes: input.binarySizeBytes });
  return new Promise<boolean>((resolve) => {
    resolveConfirmation = resolve;
  });
}

export function respondToClaudeBinaryInstallConfirmation(confirmed: boolean): void {
  if (state.status !== "confirming" || !resolveConfirmation) {
    return;
  }

  const resolve = resolveConfirmation;
  resolveConfirmation = null;
  publish(
    confirmed
      ? { status: "installing", version: state.version, stage: "checking" }
      : { status: "closing", view: state },
  );
  resolve(confirmed);
}

export function reportClaudeBinaryInstallProgress(event: ClaudeBinaryInstallProgressEvent): void {
  if (state.status !== "installing" || event.type !== "progress") {
    return;
  }
  publish({
    status: "installing",
    version: state.version,
    stage: event.stage,
    ...(event.bytesDownloaded === undefined ? {} : { bytesDownloaded: event.bytesDownloaded }),
    ...(event.totalBytes === undefined ? {} : { totalBytes: event.totalBytes }),
  });
}

export function finishClaudeBinaryInstall(): void {
  resolveConfirmation?.(false);
  resolveConfirmation = null;
  if (state.status === "confirming" || state.status === "installing") {
    publish({ status: "closing", view: state });
  }
}

export function completeClaudeBinaryInstallDialogClose(): void {
  if (state.status === "closing") {
    publish(idleState);
  }
}

export function resetClaudeBinaryInstallDialogForTests(): void {
  resolveConfirmation?.(false);
  resolveConfirmation = null;
  publish(idleState);
  listeners.clear();
}
