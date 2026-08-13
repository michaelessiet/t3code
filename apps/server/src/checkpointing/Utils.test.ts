import { expect, it } from "@effect/vitest";
import { ProjectId, type ResolvedWorkspaceRoot } from "@t3tools/contracts";

import { resolveThreadWorkspaceRoots } from "./Utils.ts";

const projectId = ProjectId.make("project-main");
const otherProjectId = ProjectId.make("project-other");

const okPathRoot = (path: string): ResolvedWorkspaceRoot => ({
  ref: { kind: "path", path },
  path,
  status: "ok",
});

const okProjectRoot = (id: ProjectId, path: string): ResolvedWorkspaceRoot => ({
  ref: { kind: "project", projectId: id },
  path,
  status: "ok",
});

const danglingProjectRoot = (id: ProjectId): ResolvedWorkspaceRoot => ({
  ref: { kind: "project", projectId: id },
  status: "missing-project",
});

it("uses the worktree path as primary when present", () => {
  const result = resolveThreadWorkspaceRoots({
    thread: { projectId, worktreePath: "/tmp/worktrees/thread-1" },
    projects: [{ id: projectId, workspaceRoot: "/tmp/main" }],
  });

  expect(result.primary).toBe("/tmp/worktrees/thread-1");
  expect(result.additional).toEqual([]);
});

it("falls back to the owning project's workspace root as primary", () => {
  const result = resolveThreadWorkspaceRoots({
    thread: { projectId, worktreePath: null },
    projects: [{ id: projectId, workspaceRoot: "/tmp/main" }],
  });

  expect(result.primary).toBe("/tmp/main");
  expect(result.additional).toEqual([]);
});

it("returns undefined primary when the owning project is missing", () => {
  const result = resolveThreadWorkspaceRoots({
    thread: { projectId, worktreePath: null },
    projects: [],
  });

  expect(result.primary).toBeUndefined();
  expect(result.additional).toEqual([]);
});

it("orders project-level roots before thread-level roots", () => {
  const result = resolveThreadWorkspaceRoots({
    thread: {
      projectId,
      worktreePath: null,
      resolvedAdditionalRoots: [okPathRoot("/tmp/thread-root")],
    },
    projects: [
      {
        id: projectId,
        workspaceRoot: "/tmp/main",
        resolvedAdditionalRoots: [okPathRoot("/tmp/project-root")],
      },
    ],
  });

  expect(result.additional).toEqual(["/tmp/project-root", "/tmp/thread-root"]);
});

it("dedupes roots repeated across project and thread level", () => {
  const result = resolveThreadWorkspaceRoots({
    thread: {
      projectId,
      worktreePath: null,
      resolvedAdditionalRoots: [okPathRoot("/tmp/shared"), okPathRoot("/tmp/thread-only")],
    },
    projects: [
      {
        id: projectId,
        workspaceRoot: "/tmp/main",
        resolvedAdditionalRoots: [okPathRoot("/tmp/shared/")],
      },
    ],
  });

  expect(result.additional).toEqual(["/tmp/shared/", "/tmp/thread-only"]);
});

it("excludes the primary root from additional roots", () => {
  const result = resolveThreadWorkspaceRoots({
    thread: {
      projectId,
      worktreePath: null,
      resolvedAdditionalRoots: [okPathRoot("/tmp/main/"), okPathRoot("/tmp/extra")],
    },
    projects: [{ id: projectId, workspaceRoot: "/tmp/main" }],
  });

  expect(result.primary).toBe("/tmp/main");
  expect(result.additional).toEqual(["/tmp/extra"]);
});

it("drops dangling project refs", () => {
  const result = resolveThreadWorkspaceRoots({
    thread: {
      projectId,
      worktreePath: null,
      resolvedAdditionalRoots: [danglingProjectRoot(otherProjectId), okPathRoot("/tmp/extra")],
    },
    projects: [
      {
        id: projectId,
        workspaceRoot: "/tmp/main",
        resolvedAdditionalRoots: [danglingProjectRoot(ProjectId.make("project-deleted"))],
      },
    ],
  });

  expect(result.additional).toEqual(["/tmp/extra"]);
});

it("passes path refs through untouched", () => {
  const result = resolveThreadWorkspaceRoots({
    thread: {
      projectId,
      worktreePath: null,
      resolvedAdditionalRoots: [okPathRoot("/tmp/pinned-a"), okPathRoot("/tmp/pinned-b")],
    },
    projects: [{ id: projectId, workspaceRoot: "/tmp/main" }],
  });

  expect(result.additional).toEqual(["/tmp/pinned-a", "/tmp/pinned-b"]);
});

it("keeps attached project roots at their workspace root for worktree-isolated threads", () => {
  const result = resolveThreadWorkspaceRoots({
    thread: {
      projectId,
      worktreePath: "/tmp/worktrees/thread-1",
      resolvedAdditionalRoots: [okProjectRoot(otherProjectId, "/tmp/other")],
    },
    projects: [
      {
        id: projectId,
        workspaceRoot: "/tmp/main",
        resolvedAdditionalRoots: [okProjectRoot(otherProjectId, "/tmp/other")],
      },
      { id: otherProjectId, workspaceRoot: "/tmp/other" },
    ],
  });

  expect(result.primary).toBe("/tmp/worktrees/thread-1");
  expect(result.additional).toEqual(["/tmp/other"]);
});
