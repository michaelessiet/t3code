# Vitre Phase 5 implementation and UI acceptance

Local native Phase 5 implemented on `feat/vitre-rust-port`, 2026-09-14.
This is not a claim of complete Electron parity. Existing unrelated worktree
changes were preserved and excluded from the phase commits.

- [x] Search/replace: virtual results, globs, regex, Unicode highlights, revision-guarded writes, keyboard access
- [x] Drafts: per-thread/project persistence, ordered background writes, restore and local new-chat landing
- [x] Explorer: mentions, drag context, copy/paste and cross-thread copying, filter, metadata, Markdown/images, comparison
- [x] Multi-root: attach/remove, persisted root selection, root-specific file tabs and git context
- [x] Snooze and resume with readable local-time banner
- [x] Graph: enable/install/build controls, status polling, search/explain/path, native visualization and source navigation
- [x] Remote milestone evaluated: remains explicitly deferred; no implicit sign-in/SSH/cloud expansion
- [x] Screenshot-driven cleanup and regression of existing chat/model/search/editor surfaces
- [x] Focused verification and handover/parity reconciliation (details below)

Release signing/feed provisioning remains a separate M6 dependency for native
self-updates and OS protocol registration.

## Verification

- Native debug harness: `VITRE_VERIFY_ICONS=phase5`, isolated home
  `/tmp/vitre-polish-phase5.CWoKm9`; both `[vitre-phase5-test] PASS` and
  `[vitre-ui-test] PASS`. No writes to the user's workspace data or paid turns.
  The isolated app and its sidecar were stopped after verification; the user's
  previously running app instance was left untouched.
- Real backend: search and replace, folder attach/switch, snooze, graph query,
  explain/subgraph/path using a seeded graph, binary preview, copy to another
  workspace and overwrite refusal. Draft independence and local-before-send state.
- Native UI: light/dark and actual 900×680 window, Markdown and image preview,
  comparison dialog, editor Cmd+/ and Undo, model-menu arrow keys/Escape,
  chat rail/code chips, sidebar/dock transitions, QuickSearch code preview.
- Focused Rust tests: search, graph layout, file-tree projection, drafts,
  root preference round-trip and runtime keybindings. Scoped Clippy with warnings denied.
- Backend: 52 tests across graph-store key/path suites; affected-file formatting
  and lint, server package typecheck (only existing Effect suggestions), and bundle.
  Exclude `**/.claude/**` when running these tests from the root: otherwise the
  nested old worktree is accidentally discovered and fails its test setup.

Representative screenshots:

- [New chat, narrow/light](/tmp/vitre-ui-phase5-draft-narrow.png)
- [Search and replacement](/tmp/vitre-ui-phase5-search-dark.png)
- [Graph](/tmp/vitre-ui-phase5-graph-populated.png)
- [Conflict comparison](/tmp/vitre-ui-phase5-compare.png)
- [Markdown](/tmp/vitre-ui-phase5-markdown-tree.png)
- [Image preview](/tmp/vitre-ui-phase5-image.png)
- [Chat and inline code](/tmp/vitre-ui-chat.png)
- [Anchored model menu](/tmp/vitre-ui-model-menu.png)

## Native differences and unverified paths

- One unsent draft per project, not multiple named draft routes/sidebar cards.
  Existing thread drafts retain text, attachments and contexts independently.
- Root switcher instead of stacked trees; single-row selection/dragging.
- Search is capped by the backend at 2,000 matches; incomplete result sets
  cannot be replaced. This does not reintroduce explorer-list truncation.
- Regex replacement uses native capture syntax (`$1`, `${name}`, `$&`, `$$`),
  not the complete JavaScript replacement language.
- Comparison is side-by-side source, not a highlighted diff. Markdown preview
  is read-only; checkbox changes are made in Source. Image preview is bounded,
  local and first-frame; SVG remains source.
- Graph canvas uses bounded neighbourhoods (120 nodes). Runtime installation,
  real extraction, long-running failure recovery and provider-backed draft
  promotion still need live coverage. No graph tool or provider CLI was installed.
- Remote connections/sign-in/SSH/cloud/WSL and signed releases remain deferred.
- CodeRabbit CLI was unavailable; local inspection, focused checks and native
  screenshots were used. This does not assert every historical matrix gap is closed.
