// @effect-diagnostics nodeBuiltinImport:off globalConsole:off - Standalone codegen exporter, runs via plain `node` outside the Effect runtime.
// Stage A golden wire fixtures for the Vitre contracts pipeline. Each fixture
// is authored as encoded (wire) JSON and validated against the live contracts
// by a decode → re-encode fixed-point assert, so every file is guaranteed
// decodable by the sidecar's schemas. Stage B's Rust tests deserialize each
// fixture and re-serialize it byte-equal.
//
// Run: node scripts/vitre/export-fixtures.ts (after export-contracts.ts)

import * as contracts from "@t3tools/contracts";
import * as Schema from "effect/Schema";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { annotateContracts, assertOk, isSchemaLike, sortKeysDeep } from "./lib.ts";

type AnyRecord = Record<string, unknown>;

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const fixturesDir = join(repoRoot, "crates", "vitre-contracts", "fixtures");
const contractsPath = join(repoRoot, "crates", "vitre-contracts", "contracts.gen.json");

// Same annotate pass as export-contracts.ts so def names line up.
annotateContracts(contracts as unknown as AnyRecord);
const generated = JSON.parse(readFileSync(contractsPath, "utf8")) as {
  $defs: Record<string, unknown>;
};

function defNameOf(schema: unknown): string {
  assertOk(isSchemaLike(schema), "fixture schema is a schema");
  const ast = (schema as { ast: AnyRecord }).ast;
  const checks = ast.checks as AnyRecord[] | undefined;
  const slot = Array.isArray(checks) && checks.length > 0 ? checks[checks.length - 1]! : ast;
  const identifier = (slot.annotations as AnyRecord | undefined)?.identifier;
  assertOk(typeof identifier === "string", "fixture schema has an identifier");
  return identifier;
}

let count = 0;
function fixture(schema: unknown, wire: unknown): void {
  const name = defNameOf(schema);
  assertOk(name in generated.$defs, `$defs.${name} exists in contracts.gen.json`);
  // The rpc transport wraps every schema in toCodecJson before encode/decode
  // (effect RpcClient.js encodePayload/decodeExit), so that derived codec IS
  // the wire format — validate fixtures through the same path.
  const codec = Schema.toCodecJson(schema as never) as never;
  const decoded = Schema.decodeUnknownSync(codec)(wire);
  const reEncoded = Schema.encodeUnknownSync(codec)(decoded);
  assertOk(
    JSON.stringify(sortKeysDeep(reEncoded)) === JSON.stringify(sortKeysDeep(wire)),
    `${name}: decode → encode is a fixed point\n  wire: ${JSON.stringify(wire)}\n  got:  ${JSON.stringify(reEncoded)}`,
  );
  const out = sortKeysDeep({ schema: name, encoded: wire });
  writeFileSync(join(fixturesDir, `${name}.json`), `${JSON.stringify(out, null, 2)}\n`);
  count += 1;
  console.log(`ok ${name}`);
}

mkdirSync(fixturesDir, { recursive: true });
const c = contracts as unknown as AnyRecord;

// 1. Recursive when-clause AST (keybindings.ts) — deep and/not/or nesting.
fixture(c.KeybindingWhenNode, {
  type: "and",
  left: { type: "not", node: { type: "identifier", name: "editorFocused" } },
  right: {
    type: "or",
    left: { type: "identifier", name: "chatFocused" },
    right: { type: "identifier", name: "terminalFocused" },
  },
});

// 2. Settings patch — optionalKey omission + DurationFromMillis wire (millis).
fixture(c.ServerSettingsPatch, {
  enableAssistantStreaming: true,
  automaticGitFetchInterval: 300000,
});

// 3. optional(NullOr(...)) with explicit null + optional omission (`path`).
fixture(c.ResolvedWorkspaceRoot, {
  ref: { kind: "path", path: "/tmp/vitre-fixture" },
  repositoryIdentity: null,
  status: "ok",
});

// 4. subscribeThread stream element — the ephemeral-delta member.
fixture(c.OrchestrationThreadStreamItem, {
  kind: "ephemeral-delta",
  threadId: "thread-fixture-01",
  messageId: "message-fixture-01",
  turnId: null,
  delta: "Hello from Vitre",
  offset: 0,
  createdAt: "2026-08-27T00:00:00.000Z",
});

// 5. subscribeShell resume input.
fixture(c.OrchestrationSubscribeShellInput, {
  afterSequence: 42,
  requestCompletionMarker: true,
});

// 6. Schema.Option(...) wire encoding under toCodecJson:
// {_tag: "Some", value} | {_tag: "None"} (no _id on the wire).
fixture(c.ServerSignalProcessResult, {
  pid: 1234,
  signal: "SIGINT",
  signaled: true,
  message: { _tag: "Some", value: "delivered" },
});

// 7. Tagged error class — `_tag` on the wire.
fixture(c.EnvironmentAuthorizationError, {
  _tag: "EnvironmentAuthorizationError",
  message: "missing scope",
  requiredScope: "orchestration:read",
});

// 8. subscribeShell live event — thread-removed member.
fixture(c.OrchestrationShellStreamEvent, {
  kind: "thread-removed",
  sequence: 7,
  threadId: "thread-fixture-01",
});

// 9. Branded ID — plain string on the wire.
fixture(c.ThreadId, "thread-fixture-01");

// 10. Cross-field filter (fromTurnCount <= toTurnCount); def name comes from
// the filter identifier "OrchestrationTurnDiffRange".
fixture(c.OrchestrationGetTurnDiffInput, {
  threadId: "thread-fixture-01",
  fromTurnCount: 1,
  toTurnCount: 3,
});

console.log(`wrote ${count} fixtures to ${fixturesDir}`);
