//! Generated contract types for the T3 sidecar's Effect-RPC surface.
//!
//! Source of truth: `contracts.gen.json` in this crate, exported from
//! `packages/contracts` by `scripts/vitre/export-contracts.ts` (Stage A) and
//! emitted as Rust by `vitre-contracts-gen` (Stage B). Regenerate with:
//!
//! ```sh
//! node scripts/vitre/export-contracts.ts
//! node scripts/vitre/export-fixtures.ts
//! pnpm exec vp fmt crates/vitre-contracts
//! cargo run -p vitre-contracts-gen && cargo fmt -p vitre-contracts
//! ```
//!
//! CI fails on drift (vitre-ci "Contracts drift"); wire-format fidelity is
//! covered by the fixture round-trip tests. Do not hand-edit `generated.rs`.

pub mod generated;
pub mod support;

pub use generated::*;
pub use support::{DurationMillis, EffectOption, RpcMethod};
