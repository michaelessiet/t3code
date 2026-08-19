import {
  EventId,
  ProjectId,
  ProviderInstanceId,
  ThreadId,
  TurnId,
  type OrchestrationEvent,
  type OrchestrationThread,
  type OrchestrationThreadActivity,
} from "@t3tools/contracts";
import { describe, expect, it } from "vite-plus/test";

import { buildThreadFeed, type ThreadFeedActivity } from "../../mobile/src/lib/threadActivity.ts";
import { deriveWorkLogEntries } from "../../web/src/session-logic.ts";
import {
  MCP_TOOL_CALL_AGGRESSIVE_STRING_BYTE_BUDGET,
  MCP_TOOL_CALL_DATA_BYTE_BUDGET,
  MCP_TOOL_CALL_STRING_BYTE_BUDGET,
  projectActivityEvent,
  projectActivityPayload,
  projectThreadDetailSnapshot,
} from "../src/orchestration/ActivityPayloadProjection.ts";

function makeActivity(
  id: string,
  itemType: string,
  data: Record<string, unknown>,
): OrchestrationThreadActivity {
  return {
    id: EventId.make(id),
    tone: "tool",
    kind: "tool.completed",
    summary: `Completed ${itemType}`,
    payload: {
      itemType,
      title: itemType,
      detail: `${itemType} detail`,
      status: "completed",
      requestKind: "command",
      data,
    },
    turnId: TurnId.make(`turn-${id}`),
    createdAt: "2026-07-27T00:00:00.000Z",
  };
}

function makeThread(activities: ReadonlyArray<OrchestrationThreadActivity>): OrchestrationThread {
  return {
    id: ThreadId.make("thread-projection"),
    projectId: ProjectId.make("project-projection"),
    title: "Activity projection",
    modelSelection: {
      instanceId: ProviderInstanceId.make("codex"),
      model: "gpt-5.4",
    },
    runtimeMode: "full-access",
    interactionMode: "default",
    branch: null,
    worktreePath: null,
    latestTurn: null,
    createdAt: "2026-07-27T00:00:00.000Z",
    updatedAt: "2026-07-27T00:00:00.000Z",
    archivedAt: null,
    settledOverride: null,
    settledAt: null,
    deletedAt: null,
    messages: [],
    proposedPlans: [],
    activities,
    checkpoints: [],
    session: null,
  };
}

const fixtures = [
  makeActivity("command", "command_execution", {
    item: {
      command: ["bash", "-lc", "pnpm test"],
      input: { command: "fallback input", ignored: "input bulk" },
      result: { command: "fallback result", aggregatedOutput: "x".repeat(10_000) },
      commandActions: [{ type: "unknown", output: "y".repeat(5_000) }],
    },
    command: "fallback data",
    kind: "execute",
    toolCallId: "tool-command",
    rawOutput: {
      content: "\n```\nfirst useful line\nsecond line",
      stdout: "unused stdout",
      ignored: "raw bulk",
    },
    ignored: "top-level bulk",
  }),
  makeActivity("file-change", "file_change", {
    item: {
      changes: [
        { oldPath: "src/old.ts", newPath: "src/new.ts", patch: "large patch".repeat(1_000) },
        { filePath: "src/second.ts" },
      ],
    },
    ignored: "top-level bulk",
  }),
  makeActivity("dynamic", "dynamic_tool_call", {
    toolCallId: "tool-dynamic",
    rawOutput: {
      stdout: "dynamic summary\nlong output".repeat(1_000),
    },
    ignored: "top-level bulk",
  }),
  makeActivity("collab", "collab_agent_tool_call", {
    kind: "delegate",
    rawOutput: {
      content: "``` \n```",
      stdout: "must not be used when content is present",
    },
    ignored: "top-level bulk",
  }),
  makeActivity("mcp", "mcp_tool_call", {
    item: {
      server: "repository",
      tool: "search",
      arguments: { query: "activity projection" },
      aggregatedOutput: "mcp payload remains available",
    },
    ignored: "MCP data is rendered verbatim",
  }),
  makeActivity("search", "web_search", {
    rawOutput: {
      totalFiles: 42,
      truncated: true,
      content: "ignored because totalFiles wins",
    },
    ignored: "top-level bulk",
  }),
  makeActivity("image", "image_view", {
    ignored: "top-level bulk",
  }),
] satisfies ReadonlyArray<OrchestrationThreadActivity>;

describe("projectActivityPayload", () => {
  function comparableActivity(activity: ThreadFeedActivity) {
    return {
      ...activity,
      fullDetail: activity.getFullDetail(),
      copyText: activity.getCopyText(),
      getFullDetail: undefined,
      getCopyText: undefined,
    };
  }

  function comparableThreadFeed(activities: ReadonlyArray<OrchestrationThreadActivity>) {
    return buildThreadFeed(makeThread(activities)).map((entry) =>
      entry.type === "activity-group"
        ? {
            ...entry,
            activities: entry.activities.map(comparableActivity),
          }
        : entry,
    );
  }

  it("drops unread bulk while retaining command, file, tool, and summary inputs", () => {
    const projected = projectActivityPayload(fixtures[0]!);
    expect(projected.payload).toEqual({
      itemType: "command_execution",
      title: "command_execution",
      detail: "command_execution detail",
      status: "completed",
      requestKind: "command",
      data: {
        item: {
          command: ["bash", "-lc", "pnpm test"],
          input: { command: "fallback input" },
          result: { command: "fallback result" },
        },
        command: "fallback data",
        toolCallId: "tool-command",
        kind: "execute",
        rawOutput: { content: "first useful line" },
      },
    });

    expect(projectActivityPayload(fixtures[1]!).payload).toMatchObject({
      data: {
        files: [{ path: "src/new.ts" }, { path: "src/old.ts" }, { path: "src/second.ts" }],
      },
    });
  });

  it("passes under-budget MCP tool data through byte-identical", () => {
    expect(projectActivityPayload(fixtures[4]!)).toBe(fixtures[4]);
    expect(JSON.stringify(projectActivityPayload(fixtures[4]!).payload)).toBe(
      JSON.stringify(fixtures[4]!.payload),
    );
  });

  it("keeps current web and mobile derived output identical for every tool item type", () => {
    for (const activity of fixtures) {
      const projected = projectActivityPayload(activity);
      expect(deriveWorkLogEntries([projected])).toEqual(deriveWorkLogEntries([activity]));
      expect(comparableThreadFeed([projected])).toEqual(comparableThreadFeed([activity]));
    }
  });

  it("projects snapshot and event transports without mutating their sources", () => {
    const activity = fixtures[0]!;
    const thread = makeThread([activity]);
    const snapshot = { snapshotSequence: 7, thread };
    const projectedSnapshot = projectThreadDetailSnapshot(snapshot);

    expect(projectedSnapshot.thread.activities[0]).not.toBe(activity);
    expect(snapshot.thread.activities[0]).toBe(activity);

    const event = {
      sequence: 8,
      eventId: EventId.make("event-activity"),
      aggregateKind: "thread",
      aggregateId: thread.id,
      occurredAt: "2026-07-27T00:00:01.000Z",
      commandId: null,
      causationEventId: null,
      correlationId: null,
      metadata: {},
      type: "thread.activity-appended",
      payload: {
        threadId: thread.id,
        activity,
      },
    } satisfies Extract<OrchestrationEvent, { type: "thread.activity-appended" }>;

    const projectedEvent = projectActivityEvent(event);
    expect(projectedEvent).not.toBe(event);
    expect(
      projectedEvent.type === "thread.activity-appended"
        ? projectedEvent.payload.activity
        : undefined,
    ).toEqual(projectActivityPayload(activity));
    expect(event.payload.activity).toBe(activity);
  });
});

describe("projectActivityPayload mcp_tool_call size budgets", () => {
  function makeMcpActivity(
    id: string,
    data: Record<string, unknown>,
    kind = "tool.completed",
  ): OrchestrationThreadActivity {
    return {
      id: EventId.make(id),
      tone: "tool",
      kind,
      summary: "Completed mcp_tool_call",
      payload: {
        itemType: "mcp_tool_call",
        title: "MCP tool call",
        detail: "repository · search",
        status: "completed",
        data,
      },
      turnId: TurnId.make(`turn-${id}`),
      createdAt: "2026-07-27T00:00:00.000Z",
    };
  }

  function payloadRecord(activity: OrchestrationThreadActivity): Record<string, unknown> {
    return activity.payload as Record<string, unknown>;
  }

  function dataRecord(activity: OrchestrationThreadActivity): Record<string, unknown> {
    return payloadRecord(activity).data as Record<string, unknown>;
  }

  const oversizedItem = {
    server: "browser",
    tool: "screenshot",
    toolCallId: "tool-mcp-oversized",
    input: { url: "https://example.test" },
    result: {
      content: [{ type: "image", data: "QUJD".repeat(64_000) }],
      aggregatedOutput: "line of output\n".repeat(8_000),
    },
  };

  it("trims an oversized MCP payload with markers, metadata, and valid JSON", () => {
    const activity = makeMcpActivity("mcp-oversized", { item: oversizedItem });
    const originalDataBytes = Buffer.byteLength(JSON.stringify(dataRecord(activity)), "utf8");
    expect(originalDataBytes).toBeGreaterThan(MCP_TOOL_CALL_DATA_BYTE_BUDGET);

    const projected = projectActivityPayload(activity);
    expect(projected).not.toBe(activity);
    // Source stays untouched (events remain the source of truth).
    expect(dataRecord(activity)).toEqual({ item: oversizedItem });

    const payload = payloadRecord(projected);
    expect(payload.trimmed).toBe(true);
    expect(payload.originalDataBytes).toBe(originalDataBytes);
    expect(payload.itemType).toBe("mcp_tool_call");
    expect(payload.title).toBe("MCP tool call");
    expect(payload.detail).toBe("repository · search");
    expect(payload.status).toBe("completed");

    const data = dataRecord(projected);
    const serialized = JSON.stringify(data);
    expect(Buffer.byteLength(serialized, "utf8")).toBeLessThanOrEqual(
      MCP_TOOL_CALL_DATA_BYTE_BUDGET,
    );
    // Structure survives; only strings were truncated.
    const item = data.item as Record<string, unknown>;
    const result = item.result as Record<string, unknown>;
    const content = result.content as Array<Record<string, unknown>>;
    expect(item.server).toBe("browser");
    expect(item.tool).toBe("screenshot");
    expect(item.input).toEqual({ url: "https://example.test" });
    expect(content[0]!.type).toBe("image");
    expect(content[0]!.data).toMatch(/…\[truncated \d+ bytes\]$/u);
    expect(result.aggregatedOutput).toMatch(/…\[truncated \d+ bytes\]$/u);
    // Round-trips as JSON (what the projection tables and wire formats store).
    expect(JSON.parse(serialized)).toEqual(data);
  });

  it("keeps identifier keys intact even when their values exceed the string budget", () => {
    const hugeRequestId = `request-${"r".repeat(MCP_TOOL_CALL_STRING_BYTE_BUDGET * 2)}`;
    const hugeToolCallId = `tool-${"t".repeat(MCP_TOOL_CALL_STRING_BYTE_BUDGET * 2)}`;
    const activity = makeMcpActivity("mcp-request-id", {
      requestId: hugeRequestId,
      item: {
        ...oversizedItem,
        toolCallId: hugeToolCallId,
        nested: { requestId: hugeRequestId },
      },
    });

    const projected = projectActivityPayload(activity);
    expect(projected).not.toBe(activity);
    const data = dataRecord(projected);
    const item = data.item as Record<string, unknown>;
    expect(data.requestId).toBe(hugeRequestId);
    expect(item.toolCallId).toBe(hugeToolCallId);
    expect((item.nested as Record<string, unknown>).requestId).toBe(hugeRequestId);
  });

  it("never trims payloads for kinds consumed by request/approval logic", () => {
    for (const kind of [
      "user-input.requested",
      "user-input.resolved",
      "provider.user-input.respond.failed",
      "approval.requested",
      "approval.resolved",
      "provider.approval.respond.failed",
    ]) {
      const activity = makeMcpActivity(`mcp-${kind}`, { item: oversizedItem }, kind);
      expect(projectActivityPayload(activity)).toBe(activity);
    }
  });

  it("falls back to the aggressive string budget when many medium strings overflow", () => {
    const mediumString = "m".repeat(MCP_TOOL_CALL_STRING_BYTE_BUDGET - 1024);
    const activity = makeMcpActivity("mcp-many-strings", {
      item: {
        server: "repository",
        tool: "multi-read",
        result: Object.fromEntries(
          Array.from({ length: 24 }, (_, index) => [`file-${index}`, mediumString]),
        ),
      },
    });

    const projected = projectActivityPayload(activity);
    expect(projected).not.toBe(activity);
    const data = dataRecord(projected);
    expect(Buffer.byteLength(JSON.stringify(data), "utf8")).toBeLessThanOrEqual(
      MCP_TOOL_CALL_DATA_BYTE_BUDGET,
    );
    const result = (data.item as Record<string, unknown>).result as Record<string, string>;
    const trimmedValue = result["file-0"]!;
    expect(trimmedValue.startsWith("m".repeat(MCP_TOOL_CALL_AGGRESSIVE_STRING_BYTE_BUDGET))).toBe(
      true,
    );
    expect(trimmedValue).toMatch(/…\[truncated \d+ bytes\]$/u);
  });

  it("never splits multi-byte characters when truncating", () => {
    const activity = makeMcpActivity("mcp-multibyte", {
      item: {
        server: "emoji",
        tool: "dump",
        result: { aggregatedOutput: "🙂".repeat(40_000) },
      },
    });

    const projected = projectActivityPayload(activity);
    const result = (dataRecord(projected).item as Record<string, unknown>).result as Record<
      string,
      string
    >;
    const trimmedValue = result.aggregatedOutput!;
    expect(trimmedValue.isWellFormed()).toBe(true);
    expect(trimmedValue).toMatch(/…\[truncated \d+ bytes\]$/u);
    expect(JSON.parse(JSON.stringify(trimmedValue))).toBe(trimmedValue);
  });

  it("is deterministic across repeated projection and event replay", () => {
    const activity = makeMcpActivity("mcp-deterministic", { item: oversizedItem });
    const first = projectActivityPayload(activity);
    const second = projectActivityPayload(activity);
    expect(second).toEqual(first);
    expect(JSON.stringify(second.payload)).toBe(JSON.stringify(first.payload));

    const event = {
      sequence: 9,
      eventId: EventId.make("event-mcp-deterministic"),
      aggregateKind: "thread",
      aggregateId: ThreadId.make("thread-projection"),
      occurredAt: "2026-07-27T00:00:02.000Z",
      commandId: null,
      causationEventId: null,
      correlationId: null,
      metadata: {},
      type: "thread.activity-appended",
      payload: {
        threadId: ThreadId.make("thread-projection"),
        activity,
      },
    } satisfies Extract<OrchestrationEvent, { type: "thread.activity-appended" }>;

    const replayed = projectActivityEvent(event);
    expect(
      replayed.type === "thread.activity-appended" ? replayed.payload.activity : undefined,
    ).toEqual(first);
  });
});
