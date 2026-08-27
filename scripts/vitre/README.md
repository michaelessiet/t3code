# Vitre contracts export (Stage A)

- `node scripts/vitre/export-contracts.ts` — exports the sidecar's Effect-RPC surface
  (`WsRpcGroup`, 101 methods) as wire-side JSON Schema into
  `crates/vitre-contracts/contracts.gen.json` (deterministic; asserts internally).
- `node scripts/vitre/export-fixtures.ts` — writes golden wire fixtures to
  `crates/vitre-contracts/fixtures/*.json`, each validated by a decode → re-encode
  fixed point through `Schema.toCodecJson` (the codec the rpc transport actually uses).
- `lib.ts` — shared annotate pass naming `$defs` after contract export names.

Regeneration flow (what CI's drift gate must mirror): run both scripts, then
`pnpm exec vp fmt crates/vitre-contracts` — the committed canonical form is the
formatted one. Regenerate + fmt is byte-stable.

Consumer: Stage B Rust emitter (`vitre-contracts-gen`) in `crates/vitre-contracts`.
