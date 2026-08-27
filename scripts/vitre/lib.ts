// Shared helpers for the Vitre Stage A contract exporters.
//
// effect's JSON Schema generator hoists a schema into `$defs` iff its AST
// carries an `identifier` annotation. The annotation slot is the last check's
// annotations when checks exist, otherwise the AST's own annotations
// (mirrors effect's internal/schema/annotations.js resolution). We annotate
// the live ASTs in-memory with their export names so every contract type gets
// a stable named def. Runtime-only mutation of the loaded module graph —
// nothing is persisted.

import * as Schema from "effect/Schema";

type AnyRecord = Record<string, unknown>;

interface SchemaLike {
  readonly ast: AnyRecord;
}

// Shared builtin ASTs: annotating these would rename every plain primitive
// usage across the whole document.
const builtinAsts: ReadonlySet<unknown> = new Set(
  [
    Schema.String,
    Schema.Number,
    Schema.Boolean,
    Schema.Null,
    Schema.Undefined,
    Schema.Void,
    Schema.Never,
    Schema.Unknown,
    Schema.Any,
    (Schema as AnyRecord).Int as SchemaLike | undefined,
    (Schema as AnyRecord).Finite as SchemaLike | undefined,
  ]
    .filter((s): s is SchemaLike => s != null && typeof (s as SchemaLike).ast === "object")
    .map((s) => s.ast),
);

export function isSchemaLike(value: unknown): value is SchemaLike {
  if (value == null) return false;
  if (typeof value !== "object" && typeof value !== "function") return false;
  const ast = (value as AnyRecord).ast;
  return ast != null && typeof ast === "object" && typeof (ast as AnyRecord)._tag === "string";
}

function annotationSlot(ast: AnyRecord): AnyRecord {
  const checks = ast.checks as AnyRecord[] | undefined;
  if (Array.isArray(checks) && checks.length > 0) {
    return checks[checks.length - 1] as AnyRecord;
  }
  return ast;
}

// effect's representation builder resolves a transformation to its encoded
// side via getLastEncoding and MEMOIZES the resulting def name on that
// encoded AST object. Branded IDs (ThreadId, ProjectId, …) and other
// transformed schemas all terminate at the SHARED builtin Schema.String.ast /
// Schema.Number.ast, so without intervention the first schema processed wins
// the name and every other one collapses into it ("threadId": $ref
// EnvironmentId). Cloning the encoding chain per export gives each named
// schema a private terminal to memoize on. Wire shape is unchanged.
function cloneEncodingChain(ast: AnyRecord): void {
  const links = ast.encoding as AnyRecord[] | undefined;
  if (!Array.isArray(links) || links.length === 0) return;
  ast.encoding = links.map((link) => {
    const to = link.to as AnyRecord;
    const clonedTo: AnyRecord = Object.assign(
      Object.create(Object.getPrototypeOf(to) as object),
      to,
    );
    cloneEncodingChain(clonedTo);
    return Object.assign(Object.create(Object.getPrototypeOf(link) as object), link, {
      to: clonedTo,
    });
  });
}

export interface AnnotateSummary {
  annotated: number;
  preNamed: number;
  builtinsSkipped: number;
  multiCandidate: Array<{ chosen: string; candidates: string[] }>;
}

/**
 * Annotate every schema export of the given module namespace with an
 * `identifier` derived from its export name. Exports are visited in sorted
 * order; aliases sharing one annotation slot are resolved by preferring a
 * candidate name present in the slot's `brands` annotation (so e.g. ThreadId
 * keeps its name over an alphabetically-earlier alias), else the shortest
 * candidate, else alphabetical.
 */
export function annotateContracts(moduleExports: AnyRecord): AnnotateSummary {
  const summary: AnnotateSummary = {
    annotated: 0,
    preNamed: 0,
    builtinsSkipped: 0,
    multiCandidate: [],
  };
  const candidatesBySlot = new Map<AnyRecord, string[]>();
  const chainCloned = new Set<AnyRecord>();
  for (const name of Object.keys(moduleExports).sort()) {
    const value = moduleExports[name];
    if (!isSchemaLike(value)) continue;
    const ast = value.ast as AnyRecord;
    if (builtinAsts.has(ast)) {
      summary.builtinsSkipped += 1;
      continue;
    }
    if (!chainCloned.has(ast)) {
      chainCloned.add(ast);
      cloneEncodingChain(ast);
    }
    const slot = annotationSlot(ast);
    const annotations = slot.annotations as AnyRecord | undefined;
    if (annotations && typeof annotations.identifier === "string") {
      summary.preNamed += 1;
      continue;
    }
    const existing = candidatesBySlot.get(slot);
    if (existing) existing.push(name);
    else candidatesBySlot.set(slot, [name]);
  }
  for (const [slot, names] of candidatesBySlot) {
    // Tiebreak between aliases sharing one slot: a name matching the slot's
    // brand annotation wins (ThreadId over an alphabetically-earlier alias),
    // else the shortest name (the base schema over a longer alias like
    // OrchestrationProposedPlanId = TrimmedNonEmptyString), else alphabetical.
    let chosen = [...names].sort((a, b) => a.length - b.length || (a < b ? -1 : 1))[0]!;
    const annotations = slot.annotations as AnyRecord | undefined;
    const brands = annotations?.brands;
    if (brands != null && names.length > 1) {
      const brandNames = new Set(Array.from(brands as Iterable<unknown>).map(String));
      const branded = names.find((n) => brandNames.has(n));
      if (branded) chosen = branded;
    }
    if (names.length > 1) {
      summary.multiCandidate.push({ chosen, candidates: names });
    }
    slot.annotations = { ...(slot.annotations as AnyRecord | undefined), identifier: chosen };
    summary.annotated += 1;
  }
  return summary;
}

/** Recursively sort object keys for byte-stable JSON output. Arrays keep order. */
export function sortKeysDeep(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sortKeysDeep);
  if (value != null && typeof value === "object") {
    const out: AnyRecord = {};
    for (const key of Object.keys(value as AnyRecord).sort()) {
      out[key] = sortKeysDeep((value as AnyRecord)[key]);
    }
    return out;
  }
  return value;
}

export function assertOk(condition: unknown, message: string): asserts condition {
  if (!condition) {
    throw new Error(`assertion failed: ${message}`);
  }
}
