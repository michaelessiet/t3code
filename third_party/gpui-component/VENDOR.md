# Vendored: gpui-component

- Upstream: https://github.com/longbridge/gpui-component
- Vendored rev: `ff3eb1128ac1058f1bb88e777744ce1237aa3b79` (main, 2026-08)
- License: Apache-2.0 (see LICENSE-APACHE)

This is Vitre's working fork of gpui-component, vendored into the monorepo
(rather than consumed as a git dep) for two reasons:

1. **Single-gpui guarantee.** Upstream floats zed `main` with no rev; Cargo
   cannot `[patch]` a git source with another rev of the same URL, so pinning
   requires editing the manifests directly. All `zed-industries/zed` deps in
   the root `Cargo.toml` here are pinned to the same rev as the workspace root
   (`f66ed399cdde86092af8af3dc7b418abf45f37f8`), asserted by the
   `cargo tree -i gpui` CI gate.
2. **Editor internals.** The M2 editor (multi-cursor, gutter injection, vim
   modal layer, LSP provider traits over WS RPCs) needs changes inside
   `crates/ui` that upstream would not take in component form.

Local changes beyond manifest pinning are tagged `// vitre:` in code.
To rebase: diff against the upstream rev above, re-apply, re-pin.
