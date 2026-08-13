import {
  CommandId,
  DEFAULT_PROVIDER_INTERACTION_MODE,
  EventId,
  ProjectId,
  ThreadId,
  ProviderInstanceId,
  type OrchestrationReadModel,
  type WorkspaceRootRef,
} from "@t3tools/contracts";
import { expect, it } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as NodeServices from "@effect/platform-node/NodeServices";

import { decideOrchestrationCommand } from "./decider.ts";
import { createEmptyReadModel, projectEvent } from "./projector.ts";

const asEventId = (value: string): EventId => EventId.make(value);
const asProjectId = (value: string): ProjectId => ProjectId.make(value);

const now = "2026-01-01T00:00:00.000Z";

const appendProjectCreated = (
  readModel: OrchestrationReadModel,
  input: {
    readonly sequence: number;
    readonly projectId: string;
    readonly workspaceRoot: string;
    readonly additionalRoots?: ReadonlyArray<WorkspaceRootRef>;
  },
) =>
  projectEvent(readModel, {
    sequence: input.sequence,
    eventId: asEventId(`evt-project-create-${input.projectId}`),
    aggregateKind: "project",
    aggregateId: asProjectId(input.projectId),
    type: "project.created",
    occurredAt: now,
    commandId: CommandId.make(`cmd-project-create-${input.projectId}`),
    causationEventId: null,
    correlationId: CommandId.make(`cmd-project-create-${input.projectId}`),
    metadata: {},
    payload: {
      projectId: asProjectId(input.projectId),
      title: input.projectId,
      workspaceRoot: input.workspaceRoot,
      additionalRoots: input.additionalRoots ?? [],
      defaultModelSelection: null,
      scripts: [],
      createdAt: now,
      updatedAt: now,
    },
  });

const appendThreadCreated = (
  readModel: OrchestrationReadModel,
  input: {
    readonly sequence: number;
    readonly threadId: string;
    readonly projectId: string;
    readonly worktreePath?: string | null;
    readonly additionalRoots?: ReadonlyArray<WorkspaceRootRef>;
  },
) =>
  projectEvent(readModel, {
    sequence: input.sequence,
    eventId: asEventId(`evt-thread-create-${input.threadId}`),
    aggregateKind: "thread",
    aggregateId: ThreadId.make(input.threadId),
    type: "thread.created",
    occurredAt: now,
    commandId: CommandId.make(`cmd-thread-create-${input.threadId}`),
    causationEventId: null,
    correlationId: CommandId.make(`cmd-thread-create-${input.threadId}`),
    metadata: {},
    payload: {
      threadId: ThreadId.make(input.threadId),
      projectId: asProjectId(input.projectId),
      title: input.threadId,
      additionalRoots: input.additionalRoots ?? [],
      modelSelection: {
        instanceId: ProviderInstanceId.make("codex"),
        model: "gpt-5-codex",
      },
      interactionMode: DEFAULT_PROVIDER_INTERACTION_MODE,
      runtimeMode: "full-access",
      branch: null,
      worktreePath: input.worktreePath ?? null,
      createdAt: now,
      updatedAt: now,
    },
  });

const makeBaseReadModel = Effect.gen(function* () {
  const initial = createEmptyReadModel(now);
  const withMain = yield* appendProjectCreated(initial, {
    sequence: 1,
    projectId: "project-main",
    workspaceRoot: "/tmp/main",
    additionalRoots: [{ kind: "path", path: "/tmp/previous" }],
  });
  const withOther = yield* appendProjectCreated(withMain, {
    sequence: 2,
    projectId: "project-other",
    workspaceRoot: "/tmp/other",
  });
  return yield* appendThreadCreated(withOther, {
    sequence: 3,
    threadId: "thread-1",
    projectId: "project-main",
    additionalRoots: [{ kind: "path", path: "/tmp/previous" }],
  });
});

it.layer(NodeServices.layer)("decider additional roots", (it) => {
  it.effect("carries additionalRoots in the project.created payload", () =>
    Effect.gen(function* () {
      const readModel = yield* makeBaseReadModel;
      const additionalRoots: ReadonlyArray<WorkspaceRootRef> = [
        { kind: "project", projectId: asProjectId("project-other") },
        { kind: "path", path: "/tmp/extra" },
      ];

      const result = yield* decideOrchestrationCommand({
        command: {
          type: "project.create",
          commandId: CommandId.make("cmd-project-create-roots"),
          projectId: asProjectId("project-roots"),
          title: "Roots",
          workspaceRoot: "/tmp/roots",
          additionalRoots: Array.from(additionalRoots),
          createdAt: now,
        },
        readModel,
      });

      const event = Array.isArray(result) ? result[0] : result;
      expect(event.type).toBe("project.created");
      expect((event.payload as { additionalRoots?: unknown }).additionalRoots).toEqual(
        additionalRoots,
      );
    }),
  );

  it.effect("defaults additionalRoots to [] when project.create omits them", () =>
    Effect.gen(function* () {
      const readModel = yield* makeBaseReadModel;

      const result = yield* decideOrchestrationCommand({
        command: {
          type: "project.create",
          commandId: CommandId.make("cmd-project-create-no-roots"),
          projectId: asProjectId("project-no-roots"),
          title: "No Roots",
          workspaceRoot: "/tmp/no-roots",
          createdAt: now,
        },
        readModel,
      });

      const event = Array.isArray(result) ? result[0] : result;
      expect(event.type).toBe("project.created");
      expect((event.payload as { additionalRoots?: unknown }).additionalRoots).toEqual([]);
    }),
  );

  it.effect("fully replaces additionalRoots in project.meta.update payload when present", () =>
    Effect.gen(function* () {
      const readModel = yield* makeBaseReadModel;
      const additionalRoots: ReadonlyArray<WorkspaceRootRef> = [
        { kind: "project", projectId: asProjectId("project-other") },
      ];

      const result = yield* decideOrchestrationCommand({
        command: {
          type: "project.meta.update",
          commandId: CommandId.make("cmd-project-update-roots"),
          projectId: asProjectId("project-main"),
          additionalRoots: Array.from(additionalRoots),
        },
        readModel,
      });

      const event = Array.isArray(result) ? result[0] : result;
      expect(event.type).toBe("project.meta-updated");
      expect((event.payload as { additionalRoots?: unknown }).additionalRoots).toEqual(
        additionalRoots,
      );
    }),
  );

  it.effect("omits additionalRoots from project.meta-updated payload when absent", () =>
    Effect.gen(function* () {
      const readModel = yield* makeBaseReadModel;

      const result = yield* decideOrchestrationCommand({
        command: {
          type: "project.meta.update",
          commandId: CommandId.make("cmd-project-update-title"),
          projectId: asProjectId("project-main"),
          title: "Renamed",
        },
        readModel,
      });

      const event = Array.isArray(result) ? result[0] : result;
      expect(event.type).toBe("project.meta-updated");
      expect("additionalRoots" in event.payload).toBe(false);
    }),
  );

  it.effect("clears additionalRoots via project.meta.update with an empty array", () =>
    Effect.gen(function* () {
      const readModel = yield* makeBaseReadModel;

      const result = yield* decideOrchestrationCommand({
        command: {
          type: "project.meta.update",
          commandId: CommandId.make("cmd-project-update-clear-roots"),
          projectId: asProjectId("project-main"),
          additionalRoots: [],
        },
        readModel,
      });

      const event = Array.isArray(result) ? result[0] : result;
      expect(event.type).toBe("project.meta-updated");
      expect((event.payload as { additionalRoots?: unknown }).additionalRoots).toEqual([]);
    }),
  );

  it.effect("carries additionalRoots in the thread.created payload", () =>
    Effect.gen(function* () {
      const readModel = yield* makeBaseReadModel;
      const additionalRoots: ReadonlyArray<WorkspaceRootRef> = [
        { kind: "project", projectId: asProjectId("project-other") },
        { kind: "path", path: "/tmp/extra" },
      ];

      const result = yield* decideOrchestrationCommand({
        command: {
          type: "thread.create",
          commandId: CommandId.make("cmd-thread-create-roots"),
          threadId: ThreadId.make("thread-roots"),
          projectId: asProjectId("project-main"),
          title: "Thread Roots",
          modelSelection: {
            instanceId: ProviderInstanceId.make("codex"),
            model: "gpt-5-codex",
          },
          interactionMode: DEFAULT_PROVIDER_INTERACTION_MODE,
          runtimeMode: "full-access",
          branch: null,
          worktreePath: null,
          additionalRoots: Array.from(additionalRoots),
          createdAt: now,
        },
        readModel,
      });

      const event = Array.isArray(result) ? result[0] : result;
      expect(event.type).toBe("thread.created");
      expect((event.payload as { additionalRoots?: unknown }).additionalRoots).toEqual(
        additionalRoots,
      );
    }),
  );

  it.effect("fully replaces additionalRoots in thread.meta.update payload when present", () =>
    Effect.gen(function* () {
      const readModel = yield* makeBaseReadModel;
      const additionalRoots: ReadonlyArray<WorkspaceRootRef> = [
        { kind: "project", projectId: asProjectId("project-other") },
      ];

      const result = yield* decideOrchestrationCommand({
        command: {
          type: "thread.meta.update",
          commandId: CommandId.make("cmd-thread-update-roots"),
          threadId: ThreadId.make("thread-1"),
          additionalRoots: Array.from(additionalRoots),
        },
        readModel,
      });

      const event = Array.isArray(result) ? result[0] : result;
      expect(event.type).toBe("thread.meta-updated");
      expect((event.payload as { additionalRoots?: unknown }).additionalRoots).toEqual(
        additionalRoots,
      );
    }),
  );

  it.effect("omits additionalRoots from thread.meta-updated payload when absent", () =>
    Effect.gen(function* () {
      const readModel = yield* makeBaseReadModel;

      const result = yield* decideOrchestrationCommand({
        command: {
          type: "thread.meta.update",
          commandId: CommandId.make("cmd-thread-update-title"),
          threadId: ThreadId.make("thread-1"),
          title: "Renamed Thread",
        },
        readModel,
      });

      const event = Array.isArray(result) ? result[0] : result;
      expect(event.type).toBe("thread.meta-updated");
      expect("additionalRoots" in event.payload).toBe(false);
    }),
  );

  it.effect("clears additionalRoots via thread.meta.update with an empty array", () =>
    Effect.gen(function* () {
      const readModel = yield* makeBaseReadModel;

      const result = yield* decideOrchestrationCommand({
        command: {
          type: "thread.meta.update",
          commandId: CommandId.make("cmd-thread-update-clear-roots"),
          threadId: ThreadId.make("thread-1"),
          additionalRoots: [],
        },
        readModel,
      });

      const event = Array.isArray(result) ? result[0] : result;
      expect(event.type).toBe("thread.meta-updated");
      expect((event.payload as { additionalRoots?: unknown }).additionalRoots).toEqual([]);
    }),
  );

  it.effect("rejects duplicate path refs that resolve to the same directory", () =>
    Effect.gen(function* () {
      const readModel = yield* makeBaseReadModel;

      const failure = yield* Effect.flip(
        decideOrchestrationCommand({
          command: {
            type: "project.meta.update",
            commandId: CommandId.make("cmd-duplicate-path-roots"),
            projectId: asProjectId("project-main"),
            additionalRoots: [
              { kind: "path", path: "/tmp/extra" },
              { kind: "path", path: "/tmp/extra/" },
            ],
          },
          readModel,
        }),
      );

      expect(failure.message).toContain("resolve to the same directory");
    }),
  );

  it.effect("rejects duplicate project refs", () =>
    Effect.gen(function* () {
      const readModel = yield* makeBaseReadModel;

      const failure = yield* Effect.flip(
        decideOrchestrationCommand({
          command: {
            type: "project.meta.update",
            commandId: CommandId.make("cmd-duplicate-project-roots"),
            projectId: asProjectId("project-main"),
            additionalRoots: [
              { kind: "project", projectId: asProjectId("project-other") },
              { kind: "project", projectId: asProjectId("project-other") },
            ],
          },
          readModel,
        }),
      );

      expect(failure.message).toContain("Duplicate additional root for project 'project-other'.");
    }),
  );

  it.effect("rejects a path ref that dereferences to the same path as a project ref", () =>
    Effect.gen(function* () {
      const readModel = yield* makeBaseReadModel;

      const failure = yield* Effect.flip(
        decideOrchestrationCommand({
          command: {
            type: "project.meta.update",
            commandId: CommandId.make("cmd-project-path-collision"),
            projectId: asProjectId("project-main"),
            additionalRoots: [
              { kind: "project", projectId: asProjectId("project-other") },
              { kind: "path", path: "/tmp/other" },
            ],
          },
          readModel,
        }),
      );

      expect(failure.message).toContain(
        "Additional roots project 'project-other' and path '/tmp/other' resolve to the same directory.",
      );
    }),
  );

  it.effect("rejects an additional root that duplicates the primary workspace root", () =>
    Effect.gen(function* () {
      const readModel = yield* makeBaseReadModel;

      const failure = yield* Effect.flip(
        decideOrchestrationCommand({
          command: {
            type: "project.create",
            commandId: CommandId.make("cmd-primary-duplicate-root"),
            projectId: asProjectId("project-primary-dup"),
            title: "Primary Dup",
            workspaceRoot: "/tmp/primary-dup",
            additionalRoots: [{ kind: "path", path: "/tmp/primary-dup/" }],
            createdAt: now,
          },
          readModel,
        }),
      );

      expect(failure.message).toContain(
        "Additional root path '/tmp/primary-dup/' duplicates the primary workspace root.",
      );
    }),
  );

  it.effect("rejects a thread additional root that duplicates the thread's primary root", () =>
    Effect.gen(function* () {
      const readModel = yield* makeBaseReadModel;

      const failure = yield* Effect.flip(
        decideOrchestrationCommand({
          command: {
            type: "thread.meta.update",
            commandId: CommandId.make("cmd-thread-primary-duplicate"),
            threadId: ThreadId.make("thread-1"),
            additionalRoots: [{ kind: "path", path: "/tmp/main" }],
          },
          readModel,
        }),
      );

      expect(failure.message).toContain(
        "Additional root path '/tmp/main' duplicates the primary workspace root.",
      );
    }),
  );

  it.effect("rejects a project attaching itself as an additional root", () =>
    Effect.gen(function* () {
      const readModel = yield* makeBaseReadModel;

      const failure = yield* Effect.flip(
        decideOrchestrationCommand({
          command: {
            type: "project.meta.update",
            commandId: CommandId.make("cmd-project-self-root"),
            projectId: asProjectId("project-main"),
            additionalRoots: [{ kind: "project", projectId: asProjectId("project-main") }],
          },
          readModel,
        }),
      );

      expect(failure.message).toContain(
        "Additional roots cannot reference the project 'project-main' they belong to.",
      );
    }),
  );

  it.effect("rejects a thread attaching its own project as an additional root", () =>
    Effect.gen(function* () {
      const readModel = yield* makeBaseReadModel;

      const failure = yield* Effect.flip(
        decideOrchestrationCommand({
          command: {
            type: "thread.create",
            commandId: CommandId.make("cmd-thread-self-project-root"),
            threadId: ThreadId.make("thread-self"),
            projectId: asProjectId("project-main"),
            title: "Thread Self",
            modelSelection: {
              instanceId: ProviderInstanceId.make("codex"),
              model: "gpt-5-codex",
            },
            interactionMode: DEFAULT_PROVIDER_INTERACTION_MODE,
            runtimeMode: "full-access",
            branch: null,
            worktreePath: null,
            additionalRoots: [{ kind: "project", projectId: asProjectId("project-main") }],
            createdAt: now,
          },
          readModel,
        }),
      );

      expect(failure.message).toContain(
        "Additional roots cannot reference the project 'project-main' they belong to.",
      );
    }),
  );

  it.effect("rejects an additional root nested inside the primary workspace root", () =>
    Effect.gen(function* () {
      const readModel = yield* makeBaseReadModel;

      const failure = yield* Effect.flip(
        decideOrchestrationCommand({
          command: {
            type: "project.meta.update",
            commandId: CommandId.make("cmd-primary-nested-root"),
            projectId: asProjectId("project-main"),
            additionalRoots: [{ kind: "path", path: "/tmp/main/nested" }],
          },
          readModel,
        }),
      );

      expect(failure.message).toContain(
        "Additional root path '/tmp/main/nested' is nested with the primary workspace root.",
      );
    }),
  );

  it.effect("rejects additional roots nested inside another effective root", () =>
    Effect.gen(function* () {
      const readModel = yield* makeBaseReadModel;

      const failure = yield* Effect.flip(
        decideOrchestrationCommand({
          command: {
            type: "project.meta.update",
            commandId: CommandId.make("cmd-nested-roots"),
            projectId: asProjectId("project-main"),
            additionalRoots: [
              { kind: "path", path: "/tmp/extra" },
              { kind: "path", path: "/tmp/extra/nested" },
            ],
          },
          readModel,
        }),
      );

      expect(failure.message).toContain(
        "Additional roots path '/tmp/extra' and path '/tmp/extra/nested' are nested within each other.",
      );
    }),
  );

  it.effect("rejects a path ref nested inside a project ref's workspace root", () =>
    Effect.gen(function* () {
      const readModel = yield* makeBaseReadModel;

      const failure = yield* Effect.flip(
        decideOrchestrationCommand({
          command: {
            type: "project.meta.update",
            commandId: CommandId.make("cmd-nested-project-root"),
            projectId: asProjectId("project-main"),
            additionalRoots: [
              { kind: "project", projectId: asProjectId("project-other") },
              { kind: "path", path: "/tmp/other/packages" },
            ],
          },
          readModel,
        }),
      );

      expect(failure.message).toContain(
        "Additional roots project 'project-other' and path '/tmp/other/packages' are nested within each other.",
      );
    }),
  );

  it.effect("rejects more than the maximum number of additional roots", () =>
    Effect.gen(function* () {
      const readModel = yield* makeBaseReadModel;
      const additionalRoots: WorkspaceRootRef[] = Array.from({ length: 9 }, (_, index) => ({
        kind: "path",
        path: `/tmp/root-${index}`,
      }));

      const failure = yield* Effect.flip(
        decideOrchestrationCommand({
          command: {
            type: "project.meta.update",
            commandId: CommandId.make("cmd-too-many-roots"),
            projectId: asProjectId("project-main"),
            additionalRoots,
          },
          readModel,
        }),
      );

      expect(failure.message).toContain(
        "At most 8 additional workspace roots are allowed (received 9).",
      );
    }),
  );

  it.effect("rejects a dangling project ref", () =>
    Effect.gen(function* () {
      const readModel = yield* makeBaseReadModel;

      const failure = yield* Effect.flip(
        decideOrchestrationCommand({
          command: {
            type: "thread.meta.update",
            commandId: CommandId.make("cmd-dangling-project-root"),
            threadId: ThreadId.make("thread-1"),
            additionalRoots: [{ kind: "project", projectId: asProjectId("project-missing") }],
          },
          readModel,
        }),
      );

      expect(failure.message).toContain(
        "Additional root references missing or deleted project 'project-missing'.",
      );
    }),
  );
});
