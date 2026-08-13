import { scopeProjectRef, scopeThreadRef } from "@t3tools/client-runtime/environment";
import {
  MAX_ADDITIONAL_WORKSPACE_ROOTS,
  type EnvironmentId,
  type ProjectId,
  type ThreadId,
  type WorkspaceRootRef,
} from "@t3tools/contracts";
import {
  isAtomCommandInterrupted,
  squashAtomCommandFailure,
  type AtomCommandResult,
} from "@t3tools/client-runtime/state/runtime";
import { ChevronDownIcon, FolderPlusIcon, FolderTreeIcon, PinIcon, XIcon } from "lucide-react";
import { memo, useCallback, useEffect, useMemo, useState } from "react";

import { subscribeAppCommand } from "./appCommandBus";
import { type DraftId } from "../composerDraftStore";
import { useAtomCommand } from "../state/use-atom-command";
import { projectEnvironment } from "../state/projects";
import { threadEnvironment } from "../state/threads";
import { useProject, useProjects, useThread } from "../state/entities";
import { normalizeRootPathForComparison, useThreadRoots } from "../state/threadRoots";
import { readLocalApi } from "../localApi";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";
import { Collapsible, CollapsiblePanel, CollapsibleTrigger } from "./ui/collapsible";
import {
  Dialog,
  DialogDescription,
  DialogHeader,
  DialogPanel,
  DialogPopup,
  DialogTitle,
} from "./ui/dialog";
import { Input } from "./ui/input";
import { Separator } from "./ui/separator";
import { stackedThreadToast, toastManager } from "./ui/toast";
import { Tooltip, TooltipPopup, TooltipTrigger } from "./ui/tooltip";

interface WorkspaceRootsControlProps {
  environmentId: EnvironmentId;
  threadId: ThreadId;
  draftId?: DraftId;
}

function refKey(ref: WorkspaceRootRef): string {
  return ref.kind === "project"
    ? `project:${ref.projectId}`
    : `path:${normalizeRootPathForComparison(ref.path)}`;
}

/**
 * The tail of an absolute path is what distinguishes it in a narrow list;
 * the full path stays available via the row tooltip.
 */
function shortenRootPathForDisplay(path: string): string {
  const parts = path
    .replace(/\\/g, "/")
    .replace(/\/+$/, "")
    .split("/")
    .filter((part) => part.length > 0);
  return parts.length <= 3 ? path : `…/${parts.slice(-3).join("/")}`;
}

/**
 * Attach/manage the workspace roots of a conversation: the pinned primary
 * root plus additional repositories from project defaults and per-thread
 * attachments. Mutations are full-replace `additionalRoots` writes on
 * `thread.meta.update` / `project.meta.update`; the server validates
 * duplicates, nesting, and the cap and its projections feed the resolved
 * roots back into `useThreadRoots`.
 */
export const WorkspaceRootsControl = memo(function WorkspaceRootsControl({
  environmentId,
  threadId,
  draftId,
}: WorkspaceRootsControlProps) {
  const threadRef = useMemo(
    () => scopeThreadRef(environmentId, threadId),
    [environmentId, threadId],
  );
  const serverThread = useThread(threadRef);
  const activeProjectRef = serverThread
    ? scopeProjectRef(serverThread.environmentId, serverThread.projectId)
    : null;
  const activeProject = useProject(activeProjectRef);
  const roots = useThreadRoots(threadRef, draftId);
  const allProjects = useProjects();

  const [open, setOpen] = useState(false);
  const [addingProject, setAddingProject] = useState(false);
  const [manualPath, setManualPath] = useState("");
  const [pending, setPending] = useState(false);

  const updateThreadMetadata = useAtomCommand(threadEnvironment.updateMetadata, {
    reportFailure: false,
  });
  const updateProject = useAtomCommand(projectEnvironment.update, { reportFailure: false });

  // Command-palette entry point: open the dialog as if clicked.
  useEffect(
    () =>
      subscribeAppCommand((command) => {
        if (command === "workspaceRoots.manage") setOpen(true);
      }),
    [],
  );

  const threadRootRefs = serverThread?.additionalRoots ?? [];
  const projectRootRefs = activeProject?.additionalRoots ?? [];
  const attachedKeys = useMemo(() => {
    const keys = new Set<string>([...threadRootRefs, ...projectRootRefs].map(refKey));
    if (activeProject) {
      keys.add(`project:${activeProject.id}`);
      keys.add(`path:${normalizeRootPathForComparison(activeProject.workspaceRoot)}`);
    }
    for (const root of roots.all) {
      keys.add(`path:${normalizeRootPathForComparison(root.path)}`);
    }
    return keys;
  }, [threadRootRefs, projectRootRefs, activeProject, roots]);

  const attachableProjects = useMemo(
    () =>
      allProjects.filter(
        (project) =>
          project.environmentId === environmentId &&
          !attachedKeys.has(`project:${project.id}`) &&
          !attachedKeys.has(`path:${normalizeRootPathForComparison(project.workspaceRoot)}`),
      ),
    [allProjects, environmentId, attachedKeys],
  );

  const totalAttached = threadRootRefs.length + projectRootRefs.length;
  const canAttachMore = totalAttached < MAX_ADDITIONAL_WORKSPACE_ROOTS;
  // Thread-level attachments need a server thread to write to; drafts only
  // surface the project defaults until the first message creates the thread.
  const canEditThreadRoots = serverThread !== null;

  const reportFailure = useCallback(
    (title: string, result: AtomCommandResult<unknown, unknown>) => {
      if (result._tag === "Failure" && !isAtomCommandInterrupted(result)) {
        const error = squashAtomCommandFailure(result);
        toastManager.add(
          stackedThreadToast({
            type: "error",
            title,
            description: error instanceof Error ? error.message : "An error occurred.",
          }),
        );
      }
    },
    [],
  );

  const writeThreadRoots = useCallback(
    async (nextRoots: ReadonlyArray<WorkspaceRootRef>) => {
      if (!serverThread) return;
      setPending(true);
      const result = await updateThreadMetadata({
        environmentId,
        input: { threadId, additionalRoots: nextRoots },
      });
      setPending(false);
      reportFailure("Failed to update conversation repositories", result);
    },
    [environmentId, reportFailure, serverThread, threadId, updateThreadMetadata],
  );

  const writeProjectRoots = useCallback(
    async (nextRoots: ReadonlyArray<WorkspaceRootRef>) => {
      if (!activeProject) return;
      setPending(true);
      const result = await updateProject({
        environmentId,
        input: { projectId: activeProject.id, additionalRoots: nextRoots },
      });
      setPending(false);
      reportFailure("Failed to update project repositories", result);
    },
    [activeProject, environmentId, reportFailure, updateProject],
  );

  const attachRef = useCallback(
    (ref: WorkspaceRootRef) => {
      if (attachedKeys.has(refKey(ref))) return;
      void writeThreadRoots([...threadRootRefs, ref]);
      setAddingProject(false);
      setManualPath("");
    },
    [attachedKeys, threadRootRefs, writeThreadRoots],
  );

  const attachProject = useCallback(
    (projectId: ProjectId) => attachRef({ kind: "project", projectId }),
    [attachRef],
  );

  const attachFolder = useCallback(async () => {
    const api = readLocalApi();
    const picked = await api?.dialogs.pickFolder().catch(() => null);
    if (picked) {
      attachRef({ kind: "path", path: picked });
      return;
    }
    // No native picker (browser client): fall back to the manual path input.
    setManualPath((value) => value);
  }, [attachRef]);

  const removeThreadRoot = useCallback(
    (ref: WorkspaceRootRef) => {
      void writeThreadRoots(threadRootRefs.filter((existing) => refKey(existing) !== refKey(ref)));
    },
    [threadRootRefs, writeThreadRoots],
  );

  const removeProjectRoot = useCallback(
    (ref: WorkspaceRootRef) => {
      void writeProjectRoots(
        projectRootRefs.filter((existing) => refKey(existing) !== refKey(ref)),
      );
    },
    [projectRootRefs, writeProjectRoots],
  );

  const makeDefaultForProject = useCallback(
    async (ref: WorkspaceRootRef) => {
      if (!activeProject) return;
      await writeProjectRoots([...projectRootRefs, ref]);
      await writeThreadRoots(threadRootRefs.filter((existing) => refKey(existing) !== refKey(ref)));
    },
    [activeProject, projectRootRefs, threadRootRefs, writeProjectRoots, writeThreadRoots],
  );

  if (!activeProject && !serverThread) return null;

  const additionalCount = roots.additional.length;
  const hasDesktopPicker = typeof window !== "undefined" && Boolean(window.desktopBridge);

  return (
    <>
      <Tooltip>
        <TooltipTrigger
          render={
            <Button
              size="xs"
              variant="outline"
              aria-label="Manage repositories"
              onClick={() => setOpen(true)}
            />
          }
        >
          <FolderTreeIcon className="size-3.5" />
          <span className="sr-only @3xl/header-actions:not-sr-only @3xl/header-actions:ml-0.5">
            Repos
          </span>
          {additionalCount > 0 ? (
            <Badge variant="secondary" className="px-1 py-0 text-[10px]">
              {additionalCount + 1}
            </Badge>
          ) : null}
        </TooltipTrigger>
        <TooltipPopup side="top">Manage repositories</TooltipPopup>
      </Tooltip>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogPopup className="max-w-md">
          <DialogHeader>
            <DialogTitle>Repositories</DialogTitle>
            <DialogDescription>
              Attached repositories are readable and editable by the agent in place — they get no
              worktrees and are not checkpointed.
            </DialogDescription>
          </DialogHeader>
          <DialogPanel>
            <div className="space-y-1.5">
              {roots.primary ? (
                <div className="flex items-center gap-3 rounded-md px-3 py-2.5">
                  <PinIcon className="size-4 shrink-0 text-muted-foreground" />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium">{roots.primary.label}</div>
                    <div
                      className="truncate text-xs text-muted-foreground"
                      title={roots.primary.path}
                    >
                      {shortenRootPathForDisplay(roots.primary.path)}
                    </div>
                  </div>
                  <Badge variant="secondary" className="shrink-0">
                    primary
                  </Badge>
                </div>
              ) : null}
              {projectRootRefs.map((ref) => {
                const resolved = activeProject?.resolvedAdditionalRoots?.find(
                  (candidate) => refKey(candidate.ref) === refKey(ref),
                );
                const path = resolved?.path ?? (ref.kind === "path" ? ref.path : null);
                const label = path
                  ? (roots.all.find((root) => root.path === path)?.label ?? path)
                  : null;
                return (
                  <div
                    key={refKey(ref)}
                    className="group flex items-center gap-3 rounded-md px-3 py-2.5 hover:bg-muted/50"
                  >
                    <FolderTreeIcon className="size-4 shrink-0 text-muted-foreground" />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-sm font-medium">
                        {label ?? "Repository unavailable"}
                      </div>
                      {path ? (
                        <div className="truncate text-xs text-muted-foreground" title={path}>
                          {shortenRootPathForDisplay(path)}
                        </div>
                      ) : null}
                    </div>
                    <Badge variant="outline" className="shrink-0">
                      project default
                    </Badge>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="size-7 shrink-0 opacity-0 group-hover:opacity-100"
                      title="Remove from project defaults"
                      disabled={pending}
                      onClick={() => removeProjectRoot(ref)}
                    >
                      <XIcon className="size-4" />
                    </Button>
                  </div>
                );
              })}
              {threadRootRefs.map((ref) => {
                const resolved = serverThread?.resolvedAdditionalRoots?.find(
                  (candidate) => refKey(candidate.ref) === refKey(ref),
                );
                const path = resolved?.path ?? (ref.kind === "path" ? ref.path : null);
                const label = path
                  ? (roots.all.find((root) => root.path === path)?.label ?? path)
                  : null;
                return (
                  <div
                    key={refKey(ref)}
                    className="group flex items-center gap-3 rounded-md px-3 py-2.5 hover:bg-muted/50"
                  >
                    <FolderTreeIcon className="size-4 shrink-0 text-muted-foreground" />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-sm font-medium">
                        {label ?? "Repository unavailable"}
                      </div>
                      {path ? (
                        <div className="truncate text-xs text-muted-foreground" title={path}>
                          {shortenRootPathForDisplay(path)}
                        </div>
                      ) : null}
                    </div>
                    <Button
                      variant="ghost"
                      size="sm"
                      className="h-7 shrink-0 px-2 text-xs opacity-0 group-hover:opacity-100"
                      title="Attach this repository to every conversation in this project"
                      disabled={pending}
                      onClick={() => void makeDefaultForProject(ref)}
                    >
                      Make default
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="size-7 shrink-0 opacity-0 group-hover:opacity-100"
                      title="Detach from this conversation"
                      disabled={pending}
                      onClick={() => removeThreadRoot(ref)}
                    >
                      <XIcon className="size-4" />
                    </Button>
                  </div>
                );
              })}
            </div>
            <Separator className="my-3" />
            <div className="space-y-1">
              {!canEditThreadRoots ? (
                <p className="text-sm text-muted-foreground">
                  Send the first message to attach repositories to this conversation.
                </p>
              ) : !canAttachMore ? (
                <p className="text-sm text-muted-foreground">
                  Root limit reached ({MAX_ADDITIONAL_WORKSPACE_ROOTS} attachments).
                </p>
              ) : (
                <>
                  <Collapsible open={addingProject} onOpenChange={setAddingProject}>
                    <CollapsibleTrigger
                      render={
                        <Button
                          variant="ghost"
                          size="sm"
                          className="w-full justify-start gap-3 px-2"
                          disabled={pending}
                        />
                      }
                    >
                      <FolderPlusIcon className="size-4" />
                      Add project…
                      <ChevronDownIcon className="ml-auto! size-4 text-muted-foreground transition-transform duration-200 in-data-panel-open:rotate-180" />
                    </CollapsibleTrigger>
                    <CollapsiblePanel>
                      <div className="max-h-72 overflow-y-auto py-2 pl-4">
                        {attachableProjects.length === 0 ? (
                          <p className="px-2 py-1 text-sm text-muted-foreground">
                            No other projects available in this environment.
                          </p>
                        ) : (
                          attachableProjects.map((project) => (
                            <Button
                              key={project.id}
                              variant="ghost"
                              className="w-full justify-start px-3 py-6"
                              disabled={pending}
                              title={project.workspaceRoot}
                              onClick={() => attachProject(project.id)}
                            >
                              <FolderTreeIcon className="size-4 shrink-0 text-muted-foreground" />
                              <span className="flex min-w-0 flex-1 flex-col items-start text-left">
                                <span className="max-w-full truncate text-sm font-medium">
                                  {project.title}
                                </span>
                                <span className="max-w-full truncate text-xs font-normal text-muted-foreground">
                                  {shortenRootPathForDisplay(project.workspaceRoot)}
                                </span>
                              </span>
                            </Button>
                          ))
                        )}
                      </div>
                    </CollapsiblePanel>
                  </Collapsible>
                  {hasDesktopPicker ? (
                    <Button
                      variant="ghost"
                      size="sm"
                      className="w-full justify-start gap-3 px-2"
                      disabled={pending}
                      onClick={() => void attachFolder()}
                    >
                      <FolderPlusIcon className="size-4" />
                      Add folder…
                    </Button>
                  ) : (
                    <form
                      className="flex items-center gap-2 pt-1"
                      onSubmit={(event) => {
                        event.preventDefault();
                        const path = manualPath.trim();
                        if (path.length === 0) return;
                        attachRef({ kind: "path", path });
                      }}
                    >
                      <Input
                        value={manualPath}
                        onChange={(event) => setManualPath(event.target.value)}
                        placeholder="Absolute folder path…"
                        className="h-8 flex-1 text-sm"
                      />
                      <Button
                        type="submit"
                        variant="outline"
                        size="sm"
                        className="h-8"
                        disabled={pending || manualPath.trim().length === 0}
                      >
                        Add
                      </Button>
                    </form>
                  )}
                </>
              )}
            </div>
          </DialogPanel>
        </DialogPopup>
      </Dialog>
    </>
  );
});
