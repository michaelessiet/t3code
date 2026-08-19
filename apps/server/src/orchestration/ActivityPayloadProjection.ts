import type {
  OrchestrationEvent,
  OrchestrationThreadActivity,
  OrchestrationThreadDetailSnapshot,
} from "@t3tools/contracts";

/**
 * Total serialized byte budget for an `mcp_tool_call` activity's `data`
 * record. Clients render `data.item` as a pretty-printed JSON block when a
 * work-log row is expanded; collapsed rows only show <=84-char previews. A
 * 32 KiB budget is several hundred rendered lines — comfortably more than the
 * expanded view is ever read for — while chopping the pathological payloads
 * (base64 screenshots, whole-file dumps) that inflate thread loads.
 */
export const MCP_TOOL_CALL_DATA_BYTE_BUDGET = 32 * 1024;

/**
 * Per-string byte budget applied when an `mcp_tool_call` payload exceeds
 * {@link MCP_TOOL_CALL_DATA_BYTE_BUDGET}. Individual strings (tool output
 * text, file contents, base64 blobs) are the realistic size carriers.
 */
export const MCP_TOOL_CALL_STRING_BYTE_BUDGET = 4 * 1024;

/**
 * Fallback per-string budget used when trimming with
 * {@link MCP_TOOL_CALL_STRING_BYTE_BUDGET} still leaves the payload over the
 * total budget (e.g. very many medium-sized strings).
 */
export const MCP_TOOL_CALL_AGGRESSIVE_STRING_BYTE_BUDGET = 256;

/**
 * Activity kinds whose payloads are consumed by logic rather than display —
 * the server's shell-summary/pending-user-input derivations and the clients'
 * pending approval/user-input derivations read `requestId` (plus
 * `requestKind`/`requestType`/`questions`/`detail`) from these payloads. They
 * are never trimmed, regardless of size.
 */
const NEVER_TRIMMED_ACTIVITY_KINDS: ReadonlySet<string> = new Set([
  "user-input.requested",
  "user-input.resolved",
  "provider.user-input.respond.failed",
  "approval.requested",
  "approval.resolved",
  "provider.approval.respond.failed",
]);

/**
 * Identifier keys that must survive trimming intact at any depth, no matter
 * how long their values are, because consumers correlate records by them.
 */
const PRESERVED_IDENTIFIER_KEYS: ReadonlySet<string> = new Set(["requestId", "toolCallId"]);

function asRecord(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function asTrimmedString(value: unknown): string | null {
  if (typeof value !== "string") {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function pushChangedFile(target: string[], seen: Set<string>, value: unknown): void {
  const normalized = asTrimmedString(value);
  if (!normalized || seen.has(normalized)) {
    return;
  }
  seen.add(normalized);
  target.push(normalized);
}

function collectChangedFiles(
  value: unknown,
  target: string[],
  seen: Set<string>,
  depth: number,
): void {
  if (depth > 4 || target.length >= 12) {
    return;
  }
  if (Array.isArray(value)) {
    for (const entry of value) {
      collectChangedFiles(entry, target, seen, depth + 1);
      if (target.length >= 12) {
        return;
      }
    }
    return;
  }

  const record = asRecord(value);
  if (!record) {
    return;
  }

  pushChangedFile(target, seen, record.path);
  pushChangedFile(target, seen, record.filePath);
  pushChangedFile(target, seen, record.relativePath);
  pushChangedFile(target, seen, record.filename);
  pushChangedFile(target, seen, record.newPath);
  pushChangedFile(target, seen, record.oldPath);

  for (const nestedKey of [
    "item",
    "result",
    "input",
    "data",
    "changes",
    "files",
    "edits",
    "patch",
    "patches",
    "operations",
  ]) {
    if (!(nestedKey in record)) {
      continue;
    }
    collectChangedFiles(record[nestedKey], target, seen, depth + 1);
    if (target.length >= 12) {
      return;
    }
  }
}

function projectCommandData(data: Record<string, unknown>): Record<string, unknown> | undefined {
  const item = asRecord(data.item);
  if (!item) {
    return undefined;
  }

  const projectedItem: Record<string, unknown> = {};
  if ("command" in item) {
    projectedItem.command = item.command;
  }

  const input = asRecord(item.input);
  if (input && "command" in input) {
    projectedItem.input = { command: input.command };
  }

  const result = asRecord(item.result);
  if (result && "command" in result) {
    projectedItem.result = { command: result.command };
  }

  return Object.keys(projectedItem).length > 0 ? projectedItem : undefined;
}

function summarizeToolTextOutput(value: string): string | null {
  const lines: string[] = [];
  for (const rawLine of value.split(/\r?\n/u)) {
    const line = rawLine.replace(/\s+/g, " ").trim();
    if (line.length > 0) {
      lines.push(line);
    }
  }

  const firstLine = lines.find((line) => line !== "```");
  if (firstLine) {
    return firstLine.length <= 84 ? firstLine : `${firstLine.slice(0, 83).trimEnd()}…`;
  }
  if (lines.length > 1) {
    return `${lines.length.toLocaleString()} lines`;
  }
  return null;
}

function projectRawOutput(value: unknown): Record<string, unknown> | undefined {
  const rawOutput = asRecord(value);
  if (!rawOutput) {
    return undefined;
  }

  if (typeof rawOutput.totalFiles === "number" && Number.isFinite(rawOutput.totalFiles)) {
    return {
      totalFiles: rawOutput.totalFiles,
      ...(rawOutput.truncated === true ? { truncated: true } : {}),
    };
  }

  const content = asTrimmedString(rawOutput.content);
  if (content) {
    const summary = summarizeToolTextOutput(content);
    return summary ? { content: summary } : undefined;
  }

  const stdout = asTrimmedString(rawOutput.stdout);
  if (stdout) {
    const summary = summarizeToolTextOutput(stdout);
    return summary ? { content: summary } : undefined;
  }

  return undefined;
}

function utf8ByteLength(value: string): number {
  return Buffer.byteLength(value, "utf8");
}

/**
 * Returns the longest prefix of `value` that encodes to at most `maxBytes`
 * UTF-8 bytes without splitting a code point (or surrogate pair).
 */
function truncateToUtf8Bytes(value: string, maxBytes: number): string {
  let bytes = 0;
  let end = 0;
  for (const character of value) {
    const codePoint = character.codePointAt(0) ?? 0;
    const size = codePoint <= 0x7f ? 1 : codePoint <= 0x7ff ? 2 : codePoint <= 0xffff ? 3 : 4;
    if (bytes + size > maxBytes) {
      break;
    }
    bytes += size;
    end += character.length;
  }
  return value.slice(0, end);
}

function trimLongString(value: string, stringByteBudget: number): string {
  const size = utf8ByteLength(value);
  if (size <= stringByteBudget) {
    return value;
  }
  const kept = truncateToUtf8Bytes(value, stringByteBudget);
  return `${kept}…[truncated ${size - utf8ByteLength(kept)} bytes]`;
}

/**
 * Deep-copies a JSON-shaped value, truncating oversized strings while
 * preserving object/array structure. Values stored under
 * {@link PRESERVED_IDENTIFIER_KEYS} are copied verbatim.
 */
function trimJsonValue(value: unknown, stringByteBudget: number): unknown {
  if (typeof value === "string") {
    return trimLongString(value, stringByteBudget);
  }
  if (Array.isArray(value)) {
    return value.map((entry) => trimJsonValue(entry, stringByteBudget));
  }
  const record = asRecord(value);
  if (record) {
    const trimmed: Record<string, unknown> = {};
    for (const [key, entry] of Object.entries(record)) {
      trimmed[key] =
        PRESERVED_IDENTIFIER_KEYS.has(key) && typeof entry === "string"
          ? entry
          : trimJsonValue(entry, stringByteBudget);
    }
    return trimmed;
  }
  return value;
}

/**
 * Budget-trims an `mcp_tool_call` activity payload. Clients render
 * `payload.data.item` verbatim (as an expandable JSON block), so the structure
 * is preserved and only oversized strings are truncated with an explicit
 * `…[truncated N bytes]` marker. Under-budget payloads are returned
 * byte-identical (same reference). Trimmed payloads carry `trimmed: true` and
 * the original serialized byte size so clients could surface the truncation.
 *
 * Pure and deterministic: replaying the same event (e.g. boot-time
 * reprojection or event-stream catch-up) always produces the same result. The
 * event store keeps the full payload, so trimming is reversible.
 */
function projectMcpToolCallActivity(
  activity: OrchestrationThreadActivity,
  payload: Record<string, unknown>,
  data: Record<string, unknown>,
): OrchestrationThreadActivity {
  if (NEVER_TRIMMED_ACTIVITY_KINDS.has(activity.kind)) {
    return activity;
  }

  const originalDataBytes = utf8ByteLength(JSON.stringify(data));
  if (originalDataBytes <= MCP_TOOL_CALL_DATA_BYTE_BUDGET) {
    return activity;
  }

  let trimmedData = trimJsonValue(data, MCP_TOOL_CALL_STRING_BYTE_BUDGET) as Record<
    string,
    unknown
  >;
  if (utf8ByteLength(JSON.stringify(trimmedData)) > MCP_TOOL_CALL_DATA_BYTE_BUDGET) {
    trimmedData = trimJsonValue(data, MCP_TOOL_CALL_AGGRESSIVE_STRING_BYTE_BUDGET) as Record<
      string,
      unknown
    >;
  }

  return {
    ...activity,
    payload: {
      ...payload,
      data: trimmedData,
      trimmed: true,
      originalDataBytes,
    },
  };
}

/**
 * Removes activity payload fields that no current client reads while retaining
 * the full payload in persistence and the event store.
 */
export function projectActivityPayload(
  activity: OrchestrationThreadActivity,
): OrchestrationThreadActivity {
  const payload = asRecord(activity.payload);
  const data = asRecord(payload?.data);
  if (!payload || !data) {
    return activity;
  }
  if (payload.itemType === "mcp_tool_call") {
    // Clients need the full `data.item` structure for the expanded MCP view,
    // so this branch keeps the shape and only enforces size budgets.
    return projectMcpToolCallActivity(activity, payload, data);
  }

  const projectedData: Record<string, unknown> = {};
  const item = projectCommandData(data);
  if (item) {
    projectedData.item = item;
  }
  if ("command" in data) {
    projectedData.command = data.command;
  }

  const changedFiles: string[] = [];
  collectChangedFiles(data, changedFiles, new Set<string>(), 0);
  if (changedFiles.length > 0) {
    // Both clients discover file names by walking objects with path-like keys.
    projectedData.files = changedFiles.map((path) => ({ path }));
  }

  if ("toolCallId" in data) {
    projectedData.toolCallId = data.toolCallId;
  }
  if ("kind" in data) {
    projectedData.kind = data.kind;
  }

  const rawOutput = projectRawOutput(data.rawOutput);
  if (rawOutput) {
    projectedData.rawOutput = rawOutput;
  }

  return {
    ...activity,
    payload: {
      ...payload,
      data: projectedData,
    },
  };
}

export function projectThreadDetailSnapshot(
  snapshot: OrchestrationThreadDetailSnapshot,
): OrchestrationThreadDetailSnapshot {
  return {
    ...snapshot,
    thread: {
      ...snapshot.thread,
      activities: snapshot.thread.activities.map(projectActivityPayload),
    },
  };
}

export function projectActivityEvent(event: OrchestrationEvent): OrchestrationEvent {
  if (event.type !== "thread.activity-appended") {
    return event;
  }
  return {
    ...event,
    payload: {
      ...event.payload,
      activity: projectActivityPayload(event.payload.activity),
    },
  };
}
