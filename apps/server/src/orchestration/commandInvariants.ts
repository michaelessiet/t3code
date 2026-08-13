import {
  MAX_ADDITIONAL_WORKSPACE_ROOTS,
  type OrchestrationCommand,
  type OrchestrationProject,
  type OrchestrationReadModel,
  type OrchestrationThread,
  type ProjectId,
  type ThreadId,
  type WorkspaceRootRef,
} from "@t3tools/contracts";
import { normalizeProjectPathForComparison } from "@t3tools/shared/path";
import * as Effect from "effect/Effect";

import { OrchestrationCommandInvariantError } from "./Errors.ts";

function invariantError(commandType: string, detail: string): OrchestrationCommandInvariantError {
  return new OrchestrationCommandInvariantError({
    commandType,
    detail,
  });
}

export function findThreadById(
  readModel: OrchestrationReadModel,
  threadId: ThreadId,
): OrchestrationThread | undefined {
  return readModel.threads.find((thread) => thread.id === threadId);
}

export function findProjectById(
  readModel: OrchestrationReadModel,
  projectId: ProjectId,
): OrchestrationProject | undefined {
  return readModel.projects.find((project) => project.id === projectId);
}

export function listThreadsByProjectId(
  readModel: OrchestrationReadModel,
  projectId: ProjectId,
): ReadonlyArray<OrchestrationThread> {
  return readModel.threads.filter((thread) => thread.projectId === projectId);
}

export function requireProject(input: {
  readonly readModel: OrchestrationReadModel;
  readonly command: OrchestrationCommand;
  readonly projectId: ProjectId;
}): Effect.Effect<OrchestrationProject, OrchestrationCommandInvariantError> {
  const project = findProjectById(input.readModel, input.projectId);
  if (project) {
    return Effect.succeed(project);
  }
  return Effect.fail(
    invariantError(
      input.command.type,
      `Project '${input.projectId}' does not exist for command '${input.command.type}'.`,
    ),
  );
}

export function requireProjectAbsent(input: {
  readonly readModel: OrchestrationReadModel;
  readonly command: OrchestrationCommand;
  readonly projectId: ProjectId;
}): Effect.Effect<void, OrchestrationCommandInvariantError> {
  if (!findProjectById(input.readModel, input.projectId)) {
    return Effect.void;
  }
  return Effect.fail(
    invariantError(
      input.command.type,
      `Project '${input.projectId}' already exists and cannot be created twice.`,
    ),
  );
}

export function requireActiveProjectWorkspaceRootAbsent(input: {
  readonly readModel: OrchestrationReadModel;
  readonly command: OrchestrationCommand;
  readonly workspaceRoot: string;
  readonly exceptProjectId?: ProjectId;
}): Effect.Effect<void, OrchestrationCommandInvariantError> {
  const normalizedWorkspaceRoot = normalizeProjectPathForComparison(input.workspaceRoot);
  const existingProject = input.readModel.projects.find(
    (project) =>
      project.deletedAt === null &&
      normalizeProjectPathForComparison(project.workspaceRoot) === normalizedWorkspaceRoot &&
      project.id !== input.exceptProjectId,
  );
  if (existingProject === undefined) {
    return Effect.void;
  }
  return Effect.fail(
    invariantError(
      input.command.type,
      `Active project '${existingProject.id}' already exists for workspace root '${normalizedWorkspaceRoot}'.`,
    ),
  );
}

export function requireThread(input: {
  readonly readModel: OrchestrationReadModel;
  readonly command: OrchestrationCommand;
  readonly threadId: ThreadId;
}): Effect.Effect<OrchestrationThread, OrchestrationCommandInvariantError> {
  const thread = findThreadById(input.readModel, input.threadId);
  if (thread) {
    return Effect.succeed(thread);
  }
  return Effect.fail(
    invariantError(
      input.command.type,
      `Thread '${input.threadId}' does not exist for command '${input.command.type}'.`,
    ),
  );
}

export function requireThreadArchived(input: {
  readonly readModel: OrchestrationReadModel;
  readonly command: OrchestrationCommand;
  readonly threadId: ThreadId;
}): Effect.Effect<OrchestrationThread, OrchestrationCommandInvariantError> {
  return requireThread(input).pipe(
    Effect.flatMap((thread) =>
      thread.archivedAt !== null
        ? Effect.succeed(thread)
        : Effect.fail(
            invariantError(
              input.command.type,
              `Thread '${input.threadId}' is not archived for command '${input.command.type}'.`,
            ),
          ),
    ),
  );
}

export function requireThreadNotArchived(input: {
  readonly readModel: OrchestrationReadModel;
  readonly command: OrchestrationCommand;
  readonly threadId: ThreadId;
}): Effect.Effect<OrchestrationThread, OrchestrationCommandInvariantError> {
  return requireThread(input).pipe(
    Effect.flatMap((thread) =>
      thread.archivedAt === null
        ? Effect.succeed(thread)
        : Effect.fail(
            invariantError(
              input.command.type,
              `Thread '${input.threadId}' is already archived and cannot handle command '${input.command.type}'.`,
            ),
          ),
    ),
  );
}

export function requireThreadAbsent(input: {
  readonly readModel: OrchestrationReadModel;
  readonly command: OrchestrationCommand;
  readonly threadId: ThreadId;
}): Effect.Effect<void, OrchestrationCommandInvariantError> {
  if (!findThreadById(input.readModel, input.threadId)) {
    return Effect.void;
  }
  return Effect.fail(
    invariantError(
      input.command.type,
      `Thread '${input.threadId}' already exists and cannot be created twice.`,
    ),
  );
}

function isNestedPath(childNormalized: string, parentNormalized: string): boolean {
  return (
    childNormalized.startsWith(`${parentNormalized}/`) ||
    childNormalized.startsWith(`${parentNormalized}\\`)
  );
}

/**
 * Validates a full-replacement `additionalRoots` list for a project or
 * thread. Project refs must point at existing, non-deleted projects and may
 * not reference the owner itself; effective paths (path refs plus
 * dereferenced project roots) must be unique, must not equal the primary
 * workspace root, and must not nest inside one another or the primary.
 *
 * Dangling project refs are rejected here (on attach) but tolerated at read
 * and session time when the referenced project is deleted later.
 */
export function requireValidAdditionalRoots(input: {
  readonly readModel: OrchestrationReadModel;
  readonly command: OrchestrationCommand;
  readonly additionalRoots: ReadonlyArray<WorkspaceRootRef>;
  readonly primaryWorkspaceRoot: string;
  readonly ownerProjectId?: ProjectId;
}): Effect.Effect<void, OrchestrationCommandInvariantError> {
  const fail = (detail: string) => Effect.fail(invariantError(input.command.type, detail));

  if (input.additionalRoots.length > MAX_ADDITIONAL_WORKSPACE_ROOTS) {
    return fail(
      `At most ${MAX_ADDITIONAL_WORKSPACE_ROOTS} additional workspace roots are allowed (received ${input.additionalRoots.length}).`,
    );
  }

  const primaryNormalized = normalizeProjectPathForComparison(input.primaryWorkspaceRoot);
  const seenProjectIds = new Set<ProjectId>();
  const effectivePaths: Array<{ readonly normalized: string; readonly label: string }> = [];

  for (const root of input.additionalRoots) {
    if (root.kind === "project") {
      if (input.ownerProjectId !== undefined && root.projectId === input.ownerProjectId) {
        return fail(
          `Additional roots cannot reference the project '${root.projectId}' they belong to.`,
        );
      }
      if (seenProjectIds.has(root.projectId)) {
        return fail(`Duplicate additional root for project '${root.projectId}'.`);
      }
      seenProjectIds.add(root.projectId);
      const referenced = findProjectById(input.readModel, root.projectId);
      if (referenced === undefined || referenced.deletedAt !== null) {
        return fail(`Additional root references missing or deleted project '${root.projectId}'.`);
      }
      effectivePaths.push({
        normalized: normalizeProjectPathForComparison(referenced.workspaceRoot),
        label: `project '${root.projectId}'`,
      });
      continue;
    }
    effectivePaths.push({
      normalized: normalizeProjectPathForComparison(root.path),
      label: `path '${root.path}'`,
    });
  }

  for (const [index, entry] of effectivePaths.entries()) {
    if (entry.normalized === primaryNormalized) {
      return fail(`Additional root ${entry.label} duplicates the primary workspace root.`);
    }
    if (
      isNestedPath(entry.normalized, primaryNormalized) ||
      isNestedPath(primaryNormalized, entry.normalized)
    ) {
      return fail(`Additional root ${entry.label} is nested with the primary workspace root.`);
    }
    for (const other of effectivePaths.slice(index + 1)) {
      if (entry.normalized === other.normalized) {
        return fail(
          `Additional roots ${entry.label} and ${other.label} resolve to the same directory.`,
        );
      }
      if (
        isNestedPath(entry.normalized, other.normalized) ||
        isNestedPath(other.normalized, entry.normalized)
      ) {
        return fail(
          `Additional roots ${entry.label} and ${other.label} are nested within each other.`,
        );
      }
    }
  }

  return Effect.void;
}

export function requireNonNegativeInteger(input: {
  readonly commandType: OrchestrationCommand["type"];
  readonly field: string;
  readonly value: number;
}): Effect.Effect<void, OrchestrationCommandInvariantError> {
  if (Number.isInteger(input.value) && input.value >= 0) {
    return Effect.void;
  }
  return Effect.fail(
    invariantError(
      input.commandType,
      `${input.field} must be an integer greater than or equal to 0.`,
    ),
  );
}
