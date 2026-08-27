//! Generated contract types for the T3 sidecar's Effect-RPC surface.
//!
//! Source of truth: `contracts.gen.json` in this crate, exported from
//! `packages/contracts` by `scripts/vitre/export-contracts.ts` (Stage A) and
//! emitted as Rust by `vitre-contracts-gen` (Stage B). Regenerate with
//! `node scripts/vitre/export-contracts.ts && cargo run -p vitre-contracts-gen`;
//! CI fails on drift. Do not hand-edit `generated.rs`.

pub mod generated;

pub use generated::*;
