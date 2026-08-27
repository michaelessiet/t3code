// @effect-diagnostics nodeBuiltinImport:off globalConsole:off - Standalone codegen exporter, runs via plain `node` outside the Effect runtime.
// Stage A of the Vitre contracts pipeline: export the sidecar's Effect-RPC
// surface (WsRpcGroup, 101 methods) as JSON Schema (draft 2020-12, wire/encoded
// side) into crates/vitre-contracts/contracts.gen.json. Stage B
// (vitre-contracts-gen) emits serde Rust from that file.
//
// Run: node scripts/vitre/export-contracts.ts

import * as contracts from "@t3tools/contracts";
import * as Schema from "effect/Schema";
import * as SchemaRepresentation from "effect/SchemaRepresentation";
import * as RpcSchema from "effect/unstable/rpc/RpcSchema";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { annotateContracts, assertOk, sortKeysDeep } from "./lib.ts";

type AnyRecord = Record<string, unknown>;

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const outPath = join(repoRoot, "crates", "vitre-contracts", "contracts.gen.json");

// --- 1. annotate every contract export so defs get stable names -------------

const annotateSummary = annotateContracts(contracts as unknown as AnyRecord);

// --- 2. collect payload/success/error roots per method ----------------------

interface RpcLike {
  readonly _tag: string;
  readonly payloadSchema: { readonly ast: AnyRecord };
  readonly successSchema: { readonly ast: AnyRecord };
  readonly errorSchema: { readonly ast: AnyRecord };
}

const group = (contracts as unknown as AnyRecord).WsRpcGroup as {
  readonly requests: ReadonlyMap<string, RpcLike>;
};
assertOk(group?.requests instanceof Map, "WsRpcGroup.requests is a Map");

interface MethodRoots {
  tag: string;
  stream: boolean;
  payloadIndex: number;
  successIndex: number;
  errorIndices: number[];
}

const roots: Array<{ ast: AnyRecord }> = [];
const methodRoots: MethodRoots[] = [];

function errorMembers(errorSchema: { readonly ast: AnyRecord }): Array<{ ast: AnyRecord }> {
  const ast = errorSchema.ast;
  if (ast._tag === "Never") return [];
  if (ast._tag === "Union") {
    const members = ast.types as AnyRecord[];
    return members.map(
      (memberAst) => Schema.make(memberAst as never) as unknown as { ast: AnyRecord },
    );
  }
  return [errorSchema];
}

// Present at runtime on this effect version but missing from RpcSchema's
// type declarations.
const getStreamSchemas = (RpcSchema as unknown as AnyRecord).getStreamSchemas as (
  schema: unknown,
) => {
  _tag: "Some" | "None";
  value?: { success: { ast: AnyRecord }; error: { ast: AnyRecord } };
};

for (const [tag, rpc] of group.requests) {
  const streamSchemas = getStreamSchemas(rpc.successSchema);
  const stream = streamSchemas._tag === "Some";
  const successSchema = stream ? streamSchemas.value!.success : rpc.successSchema;
  const errorSchema = stream ? streamSchemas.value!.error : rpc.errorSchema;

  const payloadIndex = roots.push(rpc.payloadSchema) - 1;
  const successIndex = roots.push(successSchema) - 1;
  const errorIndices = errorMembers(errorSchema).map((member) => roots.push(member) - 1);
  methodRoots.push({ tag, stream, payloadIndex, successIndex, errorIndices });
}

// --- 3. one multi-document JSON Schema generation over all roots ------------

const multi = SchemaRepresentation.fromASTs(roots.map((r) => r.ast) as never);
const doc = SchemaRepresentation.toJsonSchemaMultiDocument(multi as never) as unknown as {
  schemas: unknown[];
  definitions: Record<string, unknown>;
};
assertOk(Array.isArray(doc.schemas), `multi-document has schemas[] (got ${Object.keys(doc)})`);
assertOk(
  doc.schemas.length === roots.length,
  `no root dropped: ${doc.schemas.length} schemas for ${roots.length} roots`,
);

const defs: Record<string, unknown> = { ...doc.definitions };

// --- 4. ref-ify roots that came back inline ---------------------------------

function pascalTag(tag: string): string {
  return tag
    .split(/[^A-Za-z0-9]+/)
    .filter((part) => part.length > 0)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join("");
}

function isPureRef(schema: unknown): schema is { $ref: string } {
  return (
    schema != null &&
    typeof schema === "object" &&
    typeof (schema as AnyRecord).$ref === "string" &&
    Object.keys(schema as AnyRecord).length === 1
  );
}

let synthesized = 0;
function refFor(rootIndex: number, baseName: string): string {
  const schema = doc.schemas[rootIndex];
  if (isPureRef(schema)) return schema.$ref;
  let name = baseName;
  let n = 2;
  while (name in defs) {
    name = `${baseName}${n}`;
    n += 1;
  }
  defs[name] = schema;
  synthesized += 1;
  return `#/$defs/${name}`;
}

const methods = methodRoots
  .map((m) => {
    const base = pascalTag(m.tag);
    return {
      tag: m.tag,
      stream: m.stream,
      payload: refFor(m.payloadIndex, `${base}Payload`),
      success: refFor(m.successIndex, `${base}Success`),
      errors: m.errorIndices.map((idx, i) => refFor(idx, `${base}Error${i}`)),
    };
  })
  .sort((a, b) => (a.tag < b.tag ? -1 : a.tag > b.tag ? 1 : 0));

// --- 5. verification ---------------------------------------------------------

function collectRefs(value: unknown, into: string[]): void {
  if (Array.isArray(value)) {
    for (const item of value) collectRefs(item, into);
  } else if (value != null && typeof value === "object") {
    for (const [key, item] of Object.entries(value as AnyRecord)) {
      if (key === "$ref" && typeof item === "string") into.push(item);
      else collectRefs(item, into);
    }
  }
}

const allRefs: string[] = [];
for (const m of methods) {
  allRefs.push(m.payload, m.success, ...m.errors);
}
collectRefs(defs, allRefs);
for (const ref of allRefs) {
  const match = /^#\/\$defs\/(.+)$/.exec(ref);
  assertOk(match, `ref has $defs form: ${ref}`);
  assertOk(match[1]! in defs, `ref resolves: ${ref}`);
}

assertOk(
  methods.length === group.requests.size,
  `method count ${methods.length} === requests.size ${group.requests.size}`,
);
const subscribeShell = methods.find((m) => m.tag === "orchestration.subscribeShell");
const getConfig = methods.find((m) => m.tag === "server.getConfig");
assertOk(subscribeShell?.stream === true, "orchestration.subscribeShell is streaming");
assertOk(getConfig?.stream === false, "server.getConfig is not streaming");
assertOk("KeybindingWhenNode" in defs, "$defs.KeybindingWhenNode exists");
assertOk(
  JSON.stringify(defs.KeybindingWhenNode).includes('"#/$defs/KeybindingWhenNode"'),
  "KeybindingWhenNode is recursive via $ref",
);
// Tripwire against the encoded-AST memo collapse (see cloneEncodingChain in
// lib.ts): branded IDs must each keep their own def, and fields must point at
// the right one.
for (const id of ["ThreadId", "ProjectId", "MessageId", "TurnId", "EnvironmentId"]) {
  assertOk(id in defs, `$defs.${id} exists (branded IDs not collapsed)`);
}
assertOk(
  JSON.stringify(defs.OrchestrationShellStreamEvent).includes(
    '"threadId":{"$ref":"#/$defs/ThreadId"}',
  ),
  "thread-removed.threadId refs ThreadId",
);

// --- 6. assemble + write -----------------------------------------------------

const output = sortKeysDeep({ version: 1, methods, $defs: defs });
mkdirSync(dirname(outPath), { recursive: true });
writeFileSync(outPath, `${JSON.stringify(output, null, 2)}\n`);

const streaming = methods.filter((m) => m.stream).length;
console.log(`wrote ${outPath}`);
console.log(
  `methods: ${methods.length} (${streaming} streaming), roots: ${roots.length}, ` +
    `$defs: ${Object.keys(defs).length} (${synthesized} synthesized)`,
);
console.log(
  `annotate: ${annotateSummary.annotated} annotated, ${annotateSummary.preNamed} pre-named, ` +
    `${annotateSummary.builtinsSkipped} builtin skipped, ` +
    `${annotateSummary.multiCandidate.length} multi-candidate slots`,
);
for (const slot of annotateSummary.multiCandidate) {
  console.log(`  multi-candidate: chose ${slot.chosen} from [${slot.candidates.join(", ")}]`);
}
console.log("sample method:", JSON.stringify(subscribeShell));
