# Vitre handover — state of the migration as of 2026-09-15

This is a session-handover document for whoever (human or model) picks up the
Vitre work next. §2 is the current completion snapshot; §4 preserves the dated
implementation history and §7 gives the next steps. Supporting documents:

- **Master plan**: `~/.claude/plans/ok-so-i-d-like-floating-owl.md` — the full
  migration plan (architecture, milestones M0–M9, licensing firewall, risks).
- **Parity matrix + roadmap**: `docs/vitre-parity.md` — ~190 behavior rows with
  statuses, and the prioritized phase roadmap. **Every feature PR updates its
  rows in the same commit**, with a `Done <date> (<sha>)` note.
- **Local Phase 5 acceptance**: `docs/vitre-phase5-worklog.md` — implemented
  features, native differences, focused verification and screenshot evidence.

The parity matrix still contains duplicate/stale rows, including search/replace
and editable keybindings marked missing after their implementation. Its older
aggregate counts are not a current completion metric. Resolve disagreements
against the current code and dated verification notes before selecting work.

Project memory (`~/.claude/projects/-Users-michaelessiet-Developer-open-source-t3code/memory/`)
also carries this project's history — `t3code-vitre-gpui-migration.md` is the
main file and duplicates the gotchas below in condensed form.

---

## 1. What Vitre is

A native Rust/GPUI rewrite of T3 Code's Electron desktop app (`apps/web` +
`apps/desktop`). **The latest user direction requires 1:1 feature and design
parity with T3 Code except for changes explicitly requested** (including the
glass presentation, animations and Bezel orbs). This supersedes earlier broad
permission to substitute native designs. A working native equivalent is not,
by itself, evidence that a parity item is finished.

1. **The Node sidecar (`apps/server`) remains bundled.** Vitre is a
   protocol client over the existing HTTP + WebSocket Effect-RPC contract
   (plain-JSON envelopes, Ack-paced streams). Narrow compatibility
   fixes now include uncapped explorer listings and recognition of existing
   native project IDs by the graph store; the backend is not being rewritten.
2. **Licensing is a hard firewall.** The app is MIT. gpui is **upstream
   `zed-industries/zed`** pinned by rev in the root `Cargo.toml` — not a fork;
   the GPL `ztracing` dep is removed via
   `[patch."https://github.com/zed-industries/zed"]` pointing at local MIT
   no-op stubs (`third_party/ztracing` + `third_party/ztracing-macro`; zed
   issue #55470). UI components come from a forked gpui-component (Apache,
   vendored at `third_party/gpui-component`, **may be modified**). Zed's GPL
   crates (`editor`, `vim`, `project`, `workspace`, `ui`, `terminal`,
   `markdown`, `git_ui`, their `.scm` files) are **clean-room reference only —
   never copy code or query data from them**. `cargo deny check licenses` is
   the gate. (Note: Cargo.toml comments cite a `docs/vitre-licensing.md` that
   was never written — the licensing story lives here and in the plan.)

### Where the code lives

- **Current checkout**: `/Users/michaelessiet/Developer/open-source/t3code`.
  The original M0 worktree was `.claude/worktrees/vitre-m0`. The git stash stack is shared with the
  main checkout — never bare `git stash`/`git stash pop` (use WIP commits, or
  tagged `stash push` + `apply <sha>`).
- **Branch**: `feat/vitre-rust-port`. The full committed history includes the
  foundation through Phase 5, the September 15 UI/settings/editor follow-ups,
  and the final native-shell integration. The checkout still contains unrelated
  desktop shutdown experiments and generated Tauri output; preserve them and
  keep them outside the Rust-port PR. Base for the PR: `main`.
- **Crates**: `crates/vitre-app` (the binary — views, panels, main), of note
  `vitre-state` (pure reducers/logic, heavily unit-tested), `vitre-client`
  (EnvironmentClient: sessions, calls, subscriptions), `vitre-rpc` (wire),
  `vitre-contracts` (generated serde types + method table),
  `vitre-contracts-gen` (the Rust emitter behind the contracts drift gate —
  flow documented in `scripts/vitre/README.md`), `vitre-sidecar` (Node
  supervision). Plus the `third_party/ztracing{,-macro}` stubs (see licensing).
  Theme tokens live in `crates/vitre-app/themes/vitre.json`, `include_str!`'d
  by `main.rs` into the gpui-component `Theme` light/dark palettes.
- **Fork**: `third_party/gpui-component` — an **`exclude`d inner cargo
  workspace** in the same git repo. Root `cargo test --workspace` does NOT
  compile its tests (root `cargo fmt --all` does reach it).

The Rust-port work is committed in behavior/phase slices on
`feat/vitre-rust-port`. Preserve unrelated working-tree changes and continue to
use the branch's `feat(vitre): <lowercase headline>` style; update relevant
parity rows alongside future behavior changes.

---

## 2. Current status

**Vitre's core local macOS app is implemented and usable for testing. The full
port is not complete or release-ready.** “Phase 5 complete” referred to its
local implementation scope, not complete Electron parity or release acceptance.
The Rust/GPUI desktop client intentionally continues to use the bundled Node
backend; a Rust backend rewrite is not part of this migration.

The roadmap's phase numbers and the master plan's milestone numbers differ:

| Scope                          | Snapshot                                                                                                                                                                                                                     |
| ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Foundation and Phase 0 / M0–M2 | Foundation, core chat, files/editor and committed M2 polish landed. Later parity refinements remain below.                                                                                                                   |
| Phase 1 / chat-core catch-up   | Local core and native cleanup implemented: rich messages, composer commands, model/mode controls, plans, attachments, references and activity presentation.                                                                  |
| Phase 2 / M3                   | Dock, diff, terminal, source control, review contexts and project scripts implemented. Dock animation and definition-close regressions fixed September 15.                                                                   |
| Phase 3 / M4                   | Embedded macOS browser, device controls, automation host, picker, capture/recording and mini player implemented; some end-to-end and failure testing remains.                                                                |
| Phase 4 / M5                   | Local settings and shell plumbing exist, but the settings parity audit is **reopened**: features and design do not yet fully match T3 Code. See the September 15 follow-up below.                                            |
| Local Phase 5 / M8 long tail   | Drafts, search/replace, attached folders, explorer enhancements, snooze and graph UI implemented; see acceptance limits below.                                                                                               |
| Remote-environments track      | Deferred: multi-environment catalog, Connections settings, T3 Connect sign-in, hosted pairing, cloud relay, SSH and WSL.                                                                                                     |
| M6 release work                | A reproducible Apple Silicon `.app`/DMG recipe, embedded Node runtime and packaged sidecar/auth/RPC self-test are present. Developer ID signing, notarization, release/update feed and automatic updates remain outstanding. |

### Remaining work and acceptance limits

- **Settings parity (active priority):** finish the complete General control
  inventory and behavior; provider add-instance wizard, editable environment
  variable list, model visibility/order/favorites and custom-model management;
  LSP/source-control/Archive presentation and workflows; search navigation and
  global restore-defaults behavior. Connections remains part of the missing
  remote-environment infrastructure. Do not mark these complete from screenshots
  of collapsed provider rows, or from the existence of a generic edit dialog.
- **Live verification:** actual graph runtime installation/extraction and
  long-running failure recovery; provider-backed first send from a local draft;
  authenticated MCP → browser-host round-trip; real WebKit process-crash
  recovery. Existing graph query/explain/path checks used a seeded graph.
- **Platform coverage:** recent integrated verification is on macOS. Linux
  and Windows native behavior remains unverified; non-macOS preview capture
  is not implemented.
- **Distribution:** `scripts/vitre/build-dmg.sh` produces a locally installable,
  ad-hoc-signed Apple Silicon DMG and rejects a package whose embedded sidecar
  cannot complete auth/RPC startup without shell `PATH`. It is not notarized;
  public distribution still requires Developer ID credentials and the M6
  release/update pipeline.
- **Local parity refinements:** diff syntax coloring; terminal hover
  underlines and localhost-preview routing; branch-mismatch reconciliation;
  remaining provider/model metadata treatment and notification routing/actions.
  Editor parity also remains partial, including the manual-save dirty-buffer
  leave dialog and broader editing capabilities. Recheck each matrix row
  against current code before implementing it.
- **Native differences requiring parity review:** one unsent draft per project, a root
  switcher, read-only Markdown preview, bounded first-frame image preview,
  side-by-side source comparison, numeric browser viewport controls and GIF
  recording without audio. These are documented equivalents/limits, not
  evidence of the newly requested 1:1 parity. Re-evaluate unapproved differences.
- **Data limits:** explorer listings are virtualized without an entry-count
  cap. Workspace search is separately capped at 2,000 matches and refuses
  replace-all on incomplete results; graph canvas neighborhoods use up to
  120 nodes. See the Phase 5 worklog for the exact constraints.

### Latest verified fixes and runtime handoff

September 15 language/editor upgrade (committed on `feat/vitre-rust-port`):

- React highlighting is no longer a partial TypeScript-only query: TSX composes
  the full JavaScript/TypeScript rules with JSX elements/attributes and embedded
  template languages; JSX resolves to the JavaScript grammar. Query precedence
  now preserves specific captures instead of losing them to generic identifiers.
- Extension aliases cover JS/TS module variants, C/C++ headers/sources, Python
  stubs, Elixir scripts and other existing grammars. Special filenames include
  CMakeLists.txt, Makefile, Gemfile and Cargo.lock. Previously empty Swift, C#,
  CMake and Protobuf highlighting is wired; every bundled grammar is checked
  for a nonempty, compiling query. Theme scopes inherit the nearest configured
  parent and dotted `comment.doc` colors deserialize correctly.
- Native LSP document open/change/close operations are serialized. Position
  requests wait for queued text changes, including hover. Custom-language files
  opened before server discovery are attached when their extension arrives.
  Unicode completion anchors, `filterText`, and `sortText` are honored.
- New commands: Ctrl+Space completion (existing Cmd+I also works), Shift+F12
  workspace references, Cmd+Shift+Space signature help on macOS. Parameter hints
  also update on `(` and `,`, clear on `)`/Escape, and use a separate anchored
  panel, independent of mouse hover. The editor context menu exposes these
  commands alongside formatting and standard edit operations. References use
  a virtualized location list with same-file and cross-file reveal.
- Cmd+/ wraps JSX children with expression comments rather than rendering `//`
  as literal text. Multiline selections are wrapped once and toggle back;
  unsafe nested-comment/partial-tag cases do not corrupt the source.
- Delayed definition, formatting, and auto-import results are rejected when
  their originating document changes. Formatting now reports server failures
  and explains cancellation instead of applying old offsets to newer text.

The follow-up adds semantic tokens, preview-first rename/text-edit code actions,
linked snippets, simultaneous selections and local stdio DAP debugging. See
[`vitre-editor-tools.md`](vitre-editor-tools.md) for shortcuts, configuration,
safety behavior and explicit limits. The live LLDB fixture stops at a verified
breakpoint, evaluates a variable, steps with F10 and disconnects with Shift+F5.

Follow-up acceptance: 19 focused backend tests; 24 native files/editor tests,
the semantic UTF-16 test, dialog geometry test and three runtime-keymap tests
passed. The complete disposable native language/editor/LLDB pass passed after
integration, including const-to-let quick-fix preview/apply and cross-file
revision-conflict rejection. Screenshot inspection also fixed missing dialog
action buttons, undersized preview panes, snippet language aliases and a browser
shortcut collision (snippets now use Cmd+Option+J). Backend synchronization now
honors incremental servers using the previous document's UTF-16 range, fixing
a real TypeScript server crash when multiline snippets/undo changed line count.
Scoped Clippy, server typechecking, formatting and lint passed. The verification
app and sidecar were stopped afterward. No native Vitre process remains running
at this snapshot; always confirm process identity again before acting on it.

Scope remains important: this is **not complete Zed/VS Code replacement
acceptance**. Semantic services depend on the bundled TS/JS service or installed
language servers. Resource/command-based refactors, advanced snippets and
multi-cursor behavior, full debugger/test integration, external-library
navigation, universal grammar/extension coverage and large-file editing remain.
Do not equate a loaded grammar or a working adapter with full IDE support.

Focused verification: six language/query/color tests, nine bridge/UTF-16 and
completion tests, and six line-command/undo/read-only tests. Native disposable
`VITRE_VERIFY_ICONS=languages` uses TSX/JSX fixtures against the real bundled
TypeScript server (no paid provider turn): typed React prop suggestions,
streaming prop-type errors, cross-file definition results, parameter hints,
references, unsaved-buffer hover, and real formatting including rejection of
a deliberately stale response. Images are under `/tmp/vitre-ui-languages-*.png`.
Final app-scoped Clippy and targeted formatting/whitespace checks passed.
The final native pass exercised Ctrl+Space, Cmd+Shift+Space, Shift+F12 and
Escape through real keyboard dispatch; completion, signature and reference
screenshots were inspected. All disposable app/sidecar instances were stopped.
Run this mode only with a disposable `VITRE_HOME` whose directory name starts
with `vitre-polish-`; the fixtures never seed the user's normal home.

September 15 live-provider/model-picker follow-up (committed on `feat/vitre-rust-port`):

- Chat now folds both the initial server-config snapshot and subsequent
  `providerStatuses` events into one live provider projection. The model
  picker, composer label/options, draft landing, provider commands/skills and
  model-lock checks no longer remain stuck on the bootstrap provider list while
  Settings shows newer probe results.
- Picker availability matches T3 Code: all configured enabled instances remain
  visible in the provider rail, while only `ready` and non-`unavailable`
  instances contribute selectable models. Unavailable providers are disabled
  with the server's explanation instead of being silently omitted; the local
  `installed` bit is not used as a proxy for custom/remote readiness. Provider
  icon/label rows share a consistent leading inset and the rail scrolls when
  many configured instances exceed the popover height.
- Focused provider-state coverage and app-scoped Clippy pass. The disposable
  `VITRE_VERIFY_ICONS=ui` run switched from Codex to Claude and asserted that
  Claude's live models—not stale Codex rows—were rendered. It also visibly kept
  unavailable Grok/OpenCode entries disabled in the rail; evidence is
  `/tmp/vitre-ui-model-menu-claude.png`. The fixture app and sidecar were
  stopped afterward.

September 15 settings/context-menu follow-up (committed on `feat/vitre-rust-port`):

- Both sidebars now share overflow/secondary-click thread actions, including
  inline rename, unread, path/ID copying, branch carry-over, lifecycle actions
  and the existing guarded deletion flow. macOS Control-click is recognized.
  The shared context-menu component now gives the innermost target ownership
  before ordinary click selection; nested targets have a mouse-dispatch test.
  Keyboard navigation skips disabled entries on initial selection and wrapping.
  Inline rename defers focus until menu dismissal, and leaves native input
  Cut/Copy/Paste in charge while editing, including under project context menus.
  Native inputs also recognize macOS Control-click. Archive rows have Unarchive/Delete
  context menus; rendered assistant/plan Markdown has selection/source copying.
- Settings now has application-owned route chrome: left-aligned navigation,
  top-right search, bounded centered content, compact provider rows with actual
  provider artwork/status, enable switches, expandable configuration and
  update/reset actions. Provider details reuse a measured spring-height region.
  Server writes are serialized and merge against fresh settings. Provider forms
  preserve legacy configuration and check for conflicting legacy edits.
- Knowledge Graph has a settings page for enable/auto-rebuild, runtime status
  and install progress, executable path, retention and size limits. The actual
  graph runtime installation acceptance gap below still applies.
- Native verification uses `VITRE_VERIFY_ICONS=menus` with an isolated
  `VITRE_HOME=/tmp/vitre-polish-…`. Evidence includes
  `/tmp/vitre-ui-menus-{secondary,control-click,project,sidebar-v2}.png` and
  `/tmp/vitre-ui-settings-*.png`. It exercises both sidebars, copy target
  identity, selection preservation, rename commit/cancel, provider expansion/form
  save/removal and the settings pages. This is **not** a complete settings
  parity acceptance pass; the explicit remaining items above still apply.
- Final focused checks: 20 settings/client-settings tests, 3 real GPUI menu
  dispatch tests, and the unread-persistence test passed. Scoped
  `cargo clippy -p vitre-app --all-targets -- -D warnings`, targeted rustfmt and
  whitespace checks passed. The final clean-profile native pass used
  `/tmp/vitre-polish-menus-bdJA3M`; its app and sidecar were stopped afterward.
  That disposable instance was stopped.

September 15 Markdown-preview crash follow-up (committed on `feat/vitre-rust-port`):

- Fixed a native abort when a valid Markdown inline-code span crossed a source
  line. Code-span line endings are now rendered as spaces before reaching
  GPUI's single-line shaper, while byte offsets remain stable for selection.
  The safeguard is shared by Markdown and HTML-derived inline code.
- A real GPUI render regression and the full disposable `phase5` editor pass
  both cover the multiline form. `/tmp/vitre-ui-phase5-markdown-tree.png`
  captures the file preview rendering `first second`; the phase5 and surface
  UI passes completed, and scoped Clippy remained warning-free.

September 15 fixes keep the dock entrance identity stable across tab changes,
reload the fallback file after closing a definition target, and reject a late
read after rapid A → B → A switching. The native regression reproduced the
old failures and now passes in inline and maximized layouts. It uses resolved
definition locations through the real navigation callback, real file reads and
Cmd+W; it does not claim a new language-server resolution test.

Verification evidence: 11 dock-state tests and four files tests, app-scoped
Clippy, the native `VITRE_VERIFY_ICONS=tabs` pass and inspected screenshots.
The preceding Phase 5 pass included focused Rust checks, 52 backend graph-store
tests, server typecheck/bundle and isolated native UI acceptance. These are
recorded results, not a claim that the entire workspace suite passed.

Final branch integration rechecked `cargo check -p vitre-app --all-targets`,
24 focused files/editor tests, the three runtime-keymap tests, the right-panel
preference regression and two sidecar shutdown/process-group tests. The current
server-tree Phase 5 graph/workspace suites passed 28 tests and the LSP suites
passed 19. Running server tests from the repository root also discovers an old
`.claude/worktrees/vitre-m0` copy; run focused server files from `apps/server`
to avoid that unrelated duplicate-suite failure.

The Apple Silicon packaging pass built and mounted `Vitre-0.0.28-arm64.dmg`,
verified its APFS checksum and ad-hoc bundle seal, then reran the packaged
self-test directly from the read-only image with Node absent from `PATH`. The
embedded server completed all migrations, token exchange, WebSocket setup,
`server.getConfig` and ping. This validates local installation mechanics, not
Gatekeeper/notarization acceptance for public distribution.

The earlier manual test launches are gone. Use `./run-vitre.sh` for a fresh
launch with the normal `~/.vitre` home. Never reuse recorded PIDs or assume port
`3774` is available; recheck process identity and ports before acting. Editor
acceptance uses separate disposable `vitre-polish-*` profiles, not the user's
app database.

### Native UI and editor baseline

Native UI follow-up (2026-09-14, committed on `feat/vitre-rust-port`): sidebar and workspace dock now
retain their content during spring-driven open/close transitions; the turn rail
is a compact, vertically centered left overlay with animated active markers.
Inline code uses real monospace chips (padding, rounded border, translucent
fill), including wrapping and selection mapping back to the original text.
The model picker is an anchored composer popover with web-matching provider
marks, provider rail, selected-model indication, arrows/Enter and Escape.
Dialogs inset child backgrounds inside their curved corners and animate
width and measured body height; QuickSearch's preview column also animates.
Editor canvas and gutter use the same editor theme token, even with the input
frame hidden.

Editor line commands are in `files/commands.rs`: Cmd+/ (language-aware comment),
Option+Up/Down (move lines), Shift+Option+Up/Down (duplicate), Cmd+Shift+K
(delete lines), Cmd+L (select line), Cmd+Enter / Cmd+Shift+Enter (insert below /
above), Cmd+[ / Cmd+] (outdent / indent), Cmd+Option+F (replace). Existing
copy/paste, undo/redo, find, save, formatting and definition shortcuts remain.
The commands respect read-only buffers and are atomic undo transactions; unknown
comment syntaxes and strict JSON are left untouched. This is not a multicursor
editor. Cmd maps to Ctrl on non-macOS platforms; those platforms remain unverified.

Focused regression coverage: `cargo test -p vitre-app --bin vitre ui::tests`
and `cargo test -p vitre-app --bin vitre files::commands::tests` (8 tests).
The debug-only `VITRE_VERIFY_ICONS=ui` fixture adds an integrated native UI pass
in a disposable `VITRE_HOME=/tmp/vitre-polish-*`, including editor key dispatch,
model-menu navigation/dismissal, and QuickSearch card-to-code preview. It saves
screenshots as `/tmp/vitre-ui-*.png`; it does not send provider turns.

Current committed implementation (newest first):

| SHA         | What                                                                                                                         |
| ----------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `06602fdda` | Final native workspace-shell integration across chat, dock and application keymaps                                           |
| `3e79aeac8` | Production editor language tooling, refactors, snippets, multi-cursor editing and DAP debugging                              |
| `d21ffb4fa` | Phase 5 workspace workflows: drafts, search/replace, multi-root explorer and graph                                           |
| `de3a6911b` | Phase 1/2 interaction cleanup, menus, animations, terminal links and draft dispatch                                          |
| `0746f4c84` | Phase 4 settings, runtime keybindings and native shell parity                                                                |
| `61b3b75dc` | Phase 3 embedded preview, automation, capture and recording                                                                  |
| `ce8d11fa1` | Animated glass UI, Bezel orbs and the native icon foundation                                                                 |
| `96c9e9e81` | Original handover document at the start of the final implementation pass                                                     |
| `317b35f62` | Parity-matrix refresh: six rows the settings commit left stale in other sections; snapshot recount                           |
| `00f87d6d8` | chore: track `run-vitre.sh`, `run-vitre-parity.sh`, `scripts/vitre/stage-vitre.swift`                                        |
| `a929f481f` | **Settings screen** (see §3)                                                                                                 |
| `bd38e2c97` | Failed file read renders the server's message on the surface that asked (`TypedError::user_message()`)                       |
| `16f7a67bb` | M2 polish bundle closing phase 0 (reveal band, reveal-in-tree, F12, mod+i, placeholders, files-panel open-state persistence) |
| `ec587235d` | File-tree keyboard navigation (vim + arrows)                                                                                 |
| `56726feef` | Git diff gutter (fork `DiffGutter`/`BlockOverlay` primitives, imara-diff + CodeMirror-snapping ports)                        |

Fork primitives added so far (all in `third_party/gpui-component`):
`set_diff_gutter`, `set_block_overlay`, `set_line_highlight`,
`scroll_to_center`, `show_completions`, vim modal layer hooks, plus the
`setting` framework (upstream) now consumed by the settings screen.

---

## 3. Historical committed baseline (September 8; later work supersedes it)

This section describes the original settings commit. The expanded settings and
other local phases are summarized in §2 and the dated updates in §4.

**Settings screen (`a929f481f`)** — Electron's settings is a _route_, not a
dialog, so Vitre's `SettingsPanel` **replaces the workspace** in
`ChatApp::render` (chat streams/terminals/buffers survive the visit). Opened
via sidebar footer button, palette row, `mod+,`; Escape/Back leaves
(`SettingsClose` action scoped to the `Settings` key context).

- `crates/vitre-app/src/client_settings.rs` — new `ClientSettings` global,
  persisted to `<home>/client-settings.json` with Electron's keys and
  defaults (`settings.ts` `ClientSettingsSchema`) — except `theme`, a
  deliberate Vitre addition (Electron keeps theme in localStorage
  `t3code:theme`, outside clientSettings). It absorbed and migrates
  the old `EditorPrefs` (`editor-state.json`; vim preference carries over).
  Every key is honoured live via `cx.observe_global::<ClientSettings>`:
  theme, glassOpacity (40–100, default 80), vimMode, wordWrap (through `set_soft_wrap`), autoSaveEnabled +
  autoSaveDelayMs (the previously hard-coded 500 ms debounce),
  showFileConflictWarning, confirmThreadDelete (gates the delete dialog).
- `crates/vitre-app/src/settings.rs` — the surface, built on the fork's
  `setting` framework. It now has General plus live/read-only Providers,
  Keybindings, and Language Servers pages; the remaining page names Source
  control, Connections, Knowledge graph, Diagnostics, Beta, and Archived
  threads instead of faking them. Server rows
  (assistant streaming, auto-compact + threshold, provider update checks)
  round-trip `server.getSettings`/`updateSettings` with **no optimistic
  write** — the switch moves on the server's echo, as in Electron. Patches
  are **one-key JSON objects**; the read loop waits on `client.sessions()`
  and re-reads on every reconnect.
- Theme: `main.rs` now routes window appearance through
  `settings::apply_theme` — "System" follows the OS, explicit light/dark pin
  the Vitre palettes and ignore the OS appearance observer.
- Verified live against a real sidecar (write landed as exactly
  `{"autoCompactEnabled": false}` in the sidecar's `settings.json`, nothing
  else touched). Escape is covered by a gpui test, not a screenshot.

**Failed-read fix (`bd38e2c97`)** — `TypedError::Failed`'s Display was a
constant string; now `TypedError::user_message()` (message → detail → `_tag`
→ raw JSON) is what all UI shows, and a failed `open_file` becomes the
requested surface's own content instead of a banner over the previous file.

---

## 4. Implementation history and verification evidence

### Phase 1 — chat-core catch-up (`docs/vitre-parity.md` §Phase 1) — CORE LANDED 2026-09-12

The daily-driver core landed 2026-09-12: optimistic sends, rich markdown and
stream smoothing; `/`, `#`, `$` and `@` command menus; interaction/runtime/
traits controls; searchable provider-instance model picker; plan cards and
same-thread Implement/Refine; image/file attachments including screenshot
paste and OS drop; long-message collapse, copy/timestamps; provider/env/error
banners; anchored sends and a click-to-jump minimap; and the project/title
header segment.

### Phase 1/2 native cleanup and motion — 2026-09-14

Implemented workspace plan saving with create-only and revision protection;
new-thread implementation duplicate-click protection and partial-failure routing;
scored/highlighted model search; settled/snoozed Resume banners; stacked native
runtime/watchdog notifications; OSC-8 and wrapped terminal links including
workspace file/line/column navigation; thread-scoped running indicators; and
editor line-selection comments with immutable source snapshots and thread guards.
Unsaved buffers block replacement and are retained across root-cache eviction.
This does not implement Electron's full save/discard/cancel dialog on tab close.

The user explicitly authorized native design improvements where pixel parity
is impractical. New presentation uses Bezel's MIT thinking-orb module adapted
in `third_party/bezel-orbs` to the existing GPUI pin (no second GPUI fork), a
quiet empty-state composition, an explicit animated Build/Plan segmented switch,
composer focus transitions, dock entrance transitions and a spring terminal drawer.
Active work changes orb state between working/reasoning/composing. Idle artwork
is static; Reduce Motion, background-window pausing and hidden timer cancellation
are respected. The upstream license and exact revision are retained in VENDOR.md.

Verification: 107 vitre-app tests plus 2 orb-engine tests; scoped Clippy with
warnings denied; native isolated integration pass for Build/Plan acknowledgements,
model-picker rendering, plan save/collision rejection, editor-comment dialog,
dock/terminal transitions, actual orb animation, Reduce Motion and hidden timer
cancellation. Reproduce with a disposable `VITRE_HOME` named `vitre-polish-*`
and `VITRE_VERIFY_POLISH=1`; the debug-only harness uses application commands,
never the installed app's database or paid provider turns. Stop the test app and
its sidecar after verification. Screenshot: `/tmp/vitre-polish-welcome.png`.

Existing features corrected in the roadmap (not newly reimplemented): model
favorites/provider locks/jump shortcuts, previous/next image controls, rich inline
reference chips, minimap highlighting/assistant preview, dock terminal content,
dirty indicators, sheet/maximize layouts and tab-cycling shortcuts.

Explicit refinements still open: diff syntax coloring, terminal hover underlines
and localhost-preview routing, provider brand accents/new-model metadata,
branch-mismatch reconciliation, persistent editor gutter widgets, and full
Electron toast per-thread filtering/action coverage. These are not claimed closed.

### Native iconography and layout refinement — 2026-09-14

The native app now embeds 76 Pierre/T3 file icons in both light and dark palettes,
including the web client's filename overrides and multipart-extension precedence.
The same resolver serves explorer rows, file tabs/breadcrumbs, quick search,
changed-file cards, mentions and attachment chips. Artwork is rendered as images,
not monochrome SVG masks. Source licenses are retained under
`crates/vitre-app/icons/file-types/`; regenerate/check with Node 24 using
`scripts/vitre/export-file-icons.mjs` (the default prints an apply_patch patch).

Project headers and the chat breadcrumb load real favicons through `assets.createUrl`.
The loader constrains origin, redirects, download size, decode dimensions and SVG
resources, caches successful/missing artwork, and limits concurrent requests.
Invalid/missing favicons use stable, theme-aware initial badges. Raster and SVG
project marks retain their aspect ratio.

Layout uses 16px icons, 28px tree rows, 16px nesting and 44px header bars. Project
groups have aligned thread guides and active indicators; Search is now a working
mouse target. The sidebar no longer grows into spare space, the file tree uses a
bounded proportion of the dock, and opening a tab scrolls it into view. Narrow
windows use a readable, opaque dock overlay with a visible Hide panel action;
the normal workspace retains native glass and existing Bezel motion.

Verification: 8 focused icon/asset tests (including 325 web/native resolver cases
and rasterizing all 152 themed SVG assets), 2 focused files tests, scoped Clippy,
export consistency and script formatting. Native isolated checks cover signed
favicons, missing/broken fallbacks, nested files/tabs, real Search/Hide panel mouse
events and dock restoration, plus dark/light and narrow-layout screenshots.
Run `VITRE_VERIFY_ICONS=dark|light|narrow` with a disposable `VITRE_HOME` whose
basename starts with `vitre-polish-`. Use separately from `VITRE_VERIFY_POLISH`.
A tiling window manager can override the narrow resize; float only the test
window before the visual check. No provider turns or live user data are needed.
Screenshot artifacts: `/tmp/vitre-icons-final-dark.png`, `/tmp/vitre-icons-final-light.png`,
`/tmp/vitre-icons-final-narrow.png`.

### Phase 3/4 close-out — 2026-09-14

Phase 3/4 close-out implementation (2026-09-14):

- Browser: native child lifecycle, real WK navigation flags, stop/reload,
  SPA status, 17 device presets/custom dimensions/rotation/zoom/appearance,
  modal visibility and exact remote-close cleanup. Four-guest budget protects
  visible/mini/recording guests; health checks attempt one recovery reload.
- Automation: opt-in host with exact thread/tab routing, focus/reconnect,
  readiness/deadlines, pinned Playwright selectors, page/accessibility/PNG
  snapshots, native keys and other page operations. Picker highlights and
  captures context plus a composer PNG. GIF recording is bounded and encoded
  off the UI thread. Mini player reuses the guest and supports move/resize/dock.
- Settings: live provider config, masked API-key entry, models/accent,
  chord recording/conflicts/upsert/reset, LSP config/status, clone/publish,
  diagnostics/history/traces/signals, opt-in beta sidebar and archive actions.
  General adds timestamps, focus/manual/delay autosave, default model, font
  size and external editor. Fresh merges protect unrelated settings and reject
  stale edits. CLI/account login remains provider-managed.
- Shell: native menus/editing roles, theme/UI zoom, safe window restoration,
  reopen/deep-link routing, slow-RPC tracking, keymap/provider notices and
  streamed managed-Claude installation.

Do not claim full Electron parity: native history does not survive guest
eviction; numeric viewport inputs and GIF/no-audio capture are deliberate
native differences. Authenticated MCP-to-host end-to-end and real WK process
crash fault injection still need coverage. Windows/Linux native behavior is
not verified. Native key injection requires a visible browser tab.

M6 is a real blocker for app self-update and OS protocol registration: there
is no signed native Vitre bundle/release feed in this checkout. Do not wire
Electron/Tauri update artifacts into the Rust binary. Local Phase 5 native
features have now landed; smaller differences remain recorded in the parity matrix.

Historical verification (not a required full-package rerun for every change):

Final macOS pass: 101 app tests and the focused client request-lifecycle test
passed; scoped Clippy, formatting and host-JS syntax checks passed. The native
fixture harness also passed, including mini-player typing across a redraw,
beta-sidebar rendering and disposable provider form save/delete. Settings
was screenshot-inspected; provider installs and remote publishing were not run.

```sh
cargo test -p vitre-app --bin vitre
cargo test -p vitre-client requests::tests
cargo clippy -p vitre-app -p vitre-client --all-targets -- -D warnings
node --check crates/vitre-app/assets/preview-host.js
```

The debug-only `chat/preview_verification.rs` harness uses an isolated
`VITRE_HOME`, a seeded `VITRE_OPEN_THREAD`, and `VITRE_VERIFY_PREVIEW_URL`
pointing to `scripts/vitre/preview-fixture.html` on loopback HTTP. It checks
native snapshots/selectors/type/keys/click, device dimensions, appearance,
GIF capture, mini/dock and a disposable provider form save/delete. It does
not send model turns or mutate remote repositories/install provider CLIs.
Regenerate the embedded selector engine and its license with
`node scripts/vitre/build-preview-runtime.mjs` after changing pinned Playwright.

### Local Phase 5 and native UI acceptance — 2026-09-14

Implemented composer draft persistence and the new-conversation landing; cached
search/replace panels; attached workspace folders with root-specific file tabs,
search and git context; explorer filtering, metadata, Markdown/image preview,
conflict comparison, mentions and safe local file/folder copying; snooze/resume;
and the graph state UI, force-layout canvas, query/explain/path and source links.

Screenshot-driven fixes included left-aligned search rows, actual Markdown
preview activation, full-height compare editors, correctly scoped git branches,
compact sidebar actions, branded anchored model controls and a responsive
900×680 draft landing. Mode-switch sizing now follows UI zoom. Existing panel
motion, chat rail, inline code and model/search dialogs passed native regression.

Use `docs/vitre-phase5-worklog.md` for exact verification, screenshots and limits.
The graph store now accepts the native client's existing `vitre-project-<timestamp>`
IDs alongside web UUIDs; strict path validation and its tests remain in place.
Do not migrate user project IDs or rewrite their thread references.

Remote environments remain a separate deferred milestone. Graph tool installation
and extraction are wired but were not actually run; no paid model turn was sent.
One local draft per project and a root switcher are intentional native equivalents,
not replicas of web draft routes or stacked multi-root trees.

### Dock tab regressions resolved — 2026-09-15

The dock entrance identity no longer includes the selected surface; tab and
close-button identities follow their surface IDs. Switching tabs therefore
keeps the dock's animation state. File activation now includes the automatic
fallback after closing a tab, so closing a definition target reloads the
selected file even when its reveal request was already applied. Returning to
the current buffer also invalidates pending reads from a rapid A → B → A switch.

`VITRE_VERIFY_ICONS=tabs` with a disposable `VITRE_HOME` named
`/tmp/vitre-polish-*` reproduces the old failures and passes with the fixes.
It feeds a resolved location through the real definition-navigation callback,
checks actual editor text after Cmd+W, and exercises file/tree and rapid tab
switching in inline and maximized layouts. Screenshots:
`/tmp/vitre-ui-tabs-inline.png` and `/tmp/vitre-ui-tabs-overlay.png`.
The 11 dock-state tests, four files tests and app-scoped Clippy also pass.

### Resolved defects and follow-up context

1. **Resolved 2026-09-12: orphaned sidecar on quit.** The sidecar now starts
   in its own Unix process group; explicit/drop shutdown sends TERM to the
   group, waits up to five seconds, then uses KILL as a fallback. SIGTERM,
   SIGINT and SIGHUP received by the GPUI process are forwarded from an
   async-signal-safe handler. Unit tests cover backoff interruption and group
   termination; an external integration check sent SIGTERM only to Vitre and
   confirmed both the app and Node process disappeared.
2. **Dock tab ↔ in-panel navigation desync — resolved September 14–15.**
   Root-aware `FilesEvent::Opened` handling now routes tree/definition opens
   into the dock store. The September 15 fixes above address automatic close
   fallback, tab animation identity and stale asynchronous reads. Do not pick
   this up as an unimplemented slice; preserve its regression coverage.
3. **Workspace-listing cap — resolved 2026-09-14.** Removed the 250k index
   and 50k ignored-entry count ceilings. `projects.listEntries` sizes its
   request to the actual index and returns the complete snapshot; quick-search
   limits and existing dependency/cache exclusions remain unchanged. The native
   explorer uses GPUI `uniform_list`, a cached row projection, background tree
   construction, and virtual-index-aware focus/create/rename. Older sidecars
   that still return `truncated` show an update-and-refresh warning.
   Verified: 35 focused backend tests (including 250,001 indexed and 50,001
   ignored files), native 50,000-row viewport/Home/End/create/rename tests,
   real checkout listing (46,338 entries, no truncation), and isolated native
   nested-file/dock verification. Screenshot: `/tmp/vitre-virtual-tree.png`.

---

## 5. How to build, test, run, verify

### Local verification: keep it scoped

Follow root `AGENTS.md`: run the smallest relevant tests and targeted formatting,
lint/type checks for the changed packages. Do not run workspace-wide checks as
a routine local completion step. The latest dock-fix checks were:

```sh
cargo test -p vitre-state right_panel::tests
cargo test -p vitre-app --bin vitre files::tests
cargo clippy -p vitre-app --all-targets -- -D warnings
git diff --check -- crates/vitre-app docs/vitre-handover.md
```

`vitre-app` is binary-only: use `--bin vitre`, never `--lib`. Format/check only
affected Rust files; when operating on an individual module use rustfmt's
`--config skip_children=true` to avoid unrelated descendant formatting. Changes
to the fork need the relevant named tests in its own Cargo workspace. Backend
changes need focused `vp test run <test-files>` coverage; exclude `**/.claude/**`
when root discovery would include the old nested worktree.

### CI and release gates

CI (`.github/workflows/vitre-ci.yml`) owns the broader checks:

- Workspace formatting, Clippy with warnings denied, and workspace tests
  (the excluded gpui-component workspace needs its own targeted coverage).
- `cargo build --workspace --all-targets` and `cargo deny check licenses bans
sources` (three checks, not just licenses).
- **Single-gpui gate**: `cargo tree -i gpui --prefix none | grep -c "^gpui v"`
  must be exactly 1.
- **Contracts drift gate**: `node scripts/vitre/export-contracts.ts` + fixture
  regen + `cargo run -p vitre-contracts-gen`, then
  `git diff --exit-code crates/vitre-contracts` — regenerate in the same PR
  whenever contracts change (flow: `scripts/vitre/README.md`).
- **Live-server spikes** against a real sidecar (`cargo run -p vitre-rpc
--example spike_rpc|spike_typed`, `-p vitre-client --example spike_client`)
  — these need `pnpm --filter t3 run build:bundle` first.

Fork-test caveats: one pre-existing, font-metric-dependent failure is known
(`command::state::tests::first_enabled_selection_resets_scroll_to_its_late_row`);
running fork tests can rewrite `third_party/gpui-component/Cargo.lock`. Inspect
that diff and remove only resolver churn introduced by your own run; preserve
pre-existing changes.

### Running

`./run-vitre.sh` now runs `cargo build -p vitre-app --bin vitre` before
launching and stops on build failure. `cargo check`, `cargo test`, and Clippy
do not rebuild `target/debug/vitre`. The parity launcher still requires an
explicit build first.

Plan/Build toggle fix (2026-09-12): mode updates arrive through the shell,
so the toolbar and next-turn dispatch share the same pending → shell →
detail resolver. Pending writes serialize until shell acknowledgment;
failures roll back only the matching thread. Verified Build → Plan → Build
with GPUI mouse events in an isolated database copy, matching screenshots
and persisted `thread.interaction-mode-set` events. Temporary input hook
removed after verification. Earlier launches had accidentally reused the
08:15 binary despite source changes at 23:41.

- `./run-vitre.sh` — dev binary against the real `~/.vitre` home; puts nvm's
  node on PATH (GUI PATH lacks it) and points `VITRE_SERVER_ENTRY` at
  `apps/server/dist/bin.mjs`. Server-side changes need that dist rebuilt
  (`vp run build:bundle` from `apps/server`; there is no root `build:bundle`
  script). Read the actual chosen port from the startup log.
- `./run-vitre-parity.sh` — same, but `VITRE_HOME=/tmp/vitre-parity-home`
  (isolated, seedable). This script overrides `VITRE_HOME`; use
  `VITRE_HOME="$(mktemp -d)" ./run-vitre.sh` for a fresh ordinary test home,
  or the named fixture recipe below for automated acceptance.
- Committed verification hook: `VITRE_OPEN_THREAD=<thread-id>` selects that
  thread once the shell carries it (for sandboxed runs where synthetic clicks
  are dropped).

### Live verification recipe (native macOS)

Prefer the existing disposable native harnesses. They dispatch GPUI input inside
the app, so they do not depend on global accessibility keystroke injection.
For example, from the repository root:

```sh
vitre_test_home=$(mktemp -d /tmp/vitre-polish-tabs.XXXXXX)
VITRE_HOME="$vitre_test_home" VITRE_VERIFY_ICONS=tabs ./run-vitre.sh
```

Use `VITRE_VERIFY_ICONS=phase5` for local Phase 5 acceptance and `=ui` for the
editor/chat/model-menu/search-preview pass. Each icon fixture seeds disposable
projects and requires a home basename beginning with `vitre-polish-`; never
point fixture runs at live user state. A PASS message leaves the app running,
so stop that test instance and its sidecar after verification. An app launched
at the user's request for manual testing may remain running.

`chat/ui_verification.rs` floats only its own fixture window when AeroSpace is
present, activates/captures it by PID and saves `/tmp/vitre-ui-*.png`. Inspect
the screenshots as well as the assertions. Occluded GPUI windows can pause
render-driven work, so keep the test window visible. Temporary diagnostic hooks
should be removed when no longer needed; the guarded regression harnesses are
intentional reusable coverage.

Read live user databases only with `sqlite3 "file:...?mode=ro"`. For tests that
need existing data, use an isolated copy and set `VITRE_HOME` explicitly.

In gpui tests: `.debug_selector(|| "...".into())` on an element +
`cx.debug_bounds("...")` after `cx.update(|window, cx| window.draw(cx).clear(cx))`
gives painted geometry; `cx.simulate_keystrokes` needs the target focused and
the relevant `cx.bind_keys` done in the test.

---

## 6. Gotchas that cost real time (condensed; full list in project memory)

**gpui / fork**

- Native glass requires a transparent **gpui-component `Root`**, not just a
  transparent `ChatApp`: `Root::render` paints the opaque theme background
  before its child. Override it with `Root::new(...).bg(glass::root(cx))`.
  Confirmed 2026-09-12 on macOS with an external blue/green/orange test window:
  colours show through at 80% opacity while the backdrop's text is blurred.
- `absolute()` with no inset keeps its _static_ flow position — always pair
  with `.top_0().left_0()`. (Broke completion menus for two days.)
- gpui list rows are layout roots; `mx_auto` is a no-op inside them.
- Action bindings dispatch **before** key listeners; at equal depth the
  _later_ registration wins — that's how vim (registered after
  `gpui_component::init`, gated on `vim`/`vim_command` key contexts) takes
  escape/enter back from the Input's keymap.
- Dialog _builder_ closures run inside `ChatApp::render`; entity reads there
  panic. Use `Rc<Cell>`, touch the app only from content/footer closures.
- `scroll_to_center` only sets `deferred_scroll_offset`, and any later
  selection change recomputes it — centre **last**, and re-centre on
  `window.on_next_frame` when file + reveal arrive together.
- The fork's settings framework dispatches rows by the field's _runtime_
  type; an unsupported pairing is `unimplemented!()` **at paint time**. Draw
  the surface in a test — but only the _selected_ page renders.
- The fork paints editor selections only when the window is active AND the
  input focused — inactive-window screenshots silently show no selection.
- `IconName` is generated from the assets crate's icons dir by
  `crates/ui/build.rs`; the svg must be committed.

**RPC / contracts**

- `EnvironmentClient::call` fails immediately (`ConnectionClosed`) with no
  live session — anything that can run during sidecar boot must wait on
  `client.sessions()` and re-read on change.
- `ServerSettingsPatch` has no `Default`; the server deep-merges (except
  `providerInstances`/`languageServers`/`automaticGitFetchInterval`, which
  replace whole). Build one-key `serde_json::json!` patches and `from_value`
  them; test each key name (a typo silently yields an empty patch). Echoing a
  redacted provider secret back **destroys it**.
- All user-facing typed-RPC errors go through `TypedError::user_message()`.

**Testing**

- A real `EnvironmentClient` inside `#[gpui::test]` trips
  `assert_correct_thread`. Construct it inside `runtime.enter()`, then
  `runtime.shutdown_background()` before building the view — the client stays
  constructible but inert.
- `Theme::change` only repaints the window it's handed; from an `&mut App`-only
  context call `cx.refresh_windows()`.
- rust-analyzer crashes on this workspace (`server_crashed` on `lsp.didOpen`)
  — verify LSP features on TypeScript (vtsls). Server-side LSP traces:
  `~/.vitre/userdata/logs/server.trace.ndjson`.

**Electron reference reading**

- Client/server settings split: `packages/contracts/src/settings.ts`. Wire
  client semantics: `packages/client-runtime/src/rpc/{session,client}.ts`.
  Reducers: `packages/client-runtime/src/state/{thread,shell}Reducer.ts`.
  Editor behaviors: `apps/web/src/components/files/codemirror/*`.

---

## 7. Immediate pickup checklist for the next session

1. Work in `/Users/michaelessiet/Developer/open-source/t3code` on
   `feat/vitre-rust-port`. Inspect the dirty worktree and preserve all existing
   implementation and unrelated changes; do not assume a clean checkout or
   return to the old nested worktree. Recheck any running Vitre instance.
2. Read §2 and the Phase 5 worklog first. Reconcile stale/duplicate parity rows
   against current code before estimating remaining work or quoting counts.
   Drafts, graph UI, workspace search, editable keybindings and the dock fixes
   have already been implemented.
3. Prioritize the reopened settings parity audit against the actual React
   implementation and the user's comparison screenshots. Preserve only the
   explicitly requested visual differences. Then, for local acceptance, close
   the explicit verification gaps: graph runtime
   install/extraction, provider-backed draft promotion, authenticated browser
   automation and crash recovery. Use disposable environments and real failure
   cases, and record what actually ran.
4. Address the remaining local refinements listed in §2/§4. Do not treat the old
   native-design latitude as permission to omit reference features. Remote
   environments, cross-platform validation and signed-release infrastructure
   remain separate work tracks; completing local Phase 5 does not close them.
5. For each implementation change, run focused checks plus the affected native
   flow, inspect screenshots when relevant, update the handover/parity evidence,
   and stop disposable verification processes. Commit/push only when requested.
