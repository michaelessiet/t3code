# Vitre handover — state of the migration as of 2026-09-12

This is a session-handover document for whoever (human or model) picks up the
Vitre work next. It is a snapshot; the two living documents it summarizes are
authoritative when they disagree:

- **Master plan**: `~/.claude/plans/ok-so-i-d-like-floating-owl.md` — the full
  migration plan (architecture, milestones M0–M9, licensing firewall, risks).
- **Parity matrix + roadmap**: `docs/vitre-parity.md` — ~190 behavior rows with
  statuses, and the prioritized phase roadmap. **Every feature PR updates its
  rows in the same commit**, with a `Done <date> (<sha>)` note.

Project memory (`~/.claude/projects/-Users-michaelessiet-Developer-open-source-t3code/memory/`)
also carries this project's history — `t3code-vitre-gpui-migration.md` is the
main file and duplicates the gotchas below in condensed form.

---

## 1. What Vitre is

A native Rust/GPUI rewrite of T3 Code's Electron desktop app (`apps/web` +
`apps/desktop`), at **1:1 feature parity** — the standing user directive is
that Vitre must initially look and behave *identical* to Electron; design
iteration comes after parity. Two structural decisions shape everything:

1. **The Node sidecar (`apps/server`) stays as-is, bundled.** Vitre is a pure
   protocol client over the existing HTTP + WebSocket Effect-RPC contract
   (~100 methods, plain-JSON envelopes, Ack-paced streams). No server changes.
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

- **Worktree**: `.claude/worktrees/vitre-m0` (a git worktree of this repo).
  Work from inside it; don't `cd` out. The git stash stack is shared with the
  main checkout — never bare `git stash`/`git stash pop` (use WIP commits, or
  tagged `stash push` + `apply <sha>`).
- **Branch**: `feat/vitre-m3-panels` (all work committed; nothing stashed or
  dangling). Base for PRs: `main`.
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

Commits: `git -c core.hooksPath=/dev/null commit` (repo hooks assume the
Electron toolchain). Style in this branch: `feat(vitre): <lowercase headline>`
with a narrative body explaining the why; parity-doc rows flipped in the same
commit.

---

## 2. Current status

**Milestones** (plan §Milestones): M0 (foundation), M1 (chat core), M2
(files + editor → self-hosting) are **complete**. M3 (terminal + diff + right
panel + source control — "phase 2" in the parity doc's roadmap; the phase
numbering lives in `docs/vitre-parity.md`, not the master plan) is complete
except the 2.5 satellites, 2.7d, and some right-panel polish (all listed in
§4). A slice of M5 (settings) was pulled forward and shipped 2026-09-08.

**Parity snapshot (recounted 2026-09-12): 71 present · 40 partial · 79 absent
· 1 waived.** The absent mass is whole subsystems scheduled M4+ (preview,
knowledge graph, connections/multi-environment, drafts).

Recent commit history (newest first):

| SHA | What |
|---|---|
| `317b35f62` | Parity-matrix refresh: six rows the settings commit left stale in other sections; snapshot recount |
| `00f87d6d8` | chore: track `run-vitre.sh`, `run-vitre-parity.sh`, `scripts/vitre/stage-vitre.swift` |
| `a929f481f` | **Settings screen** (see §3) |
| `bd38e2c97` | Failed file read renders the server's message on the surface that asked (`TypedError::user_message()`) |
| `16f7a67bb` | M2 polish bundle closing phase 0 (reveal band, reveal-in-tree, F12, mod+i, placeholders, files-panel open-state persistence) |
| `ec587235d` | File-tree keyboard navigation (vim + arrows) |
| `56726feef` | Git diff gutter (fork `DiffGutter`/`BlockOverlay` primitives, imara-diff + CodeMirror-snapping ports) |

Fork primitives added so far (all in `third_party/gpui-component`):
`set_diff_gutter`, `set_block_overlay`, `set_line_highlight`,
`scroll_to_center`, `show_completions`, vim modal layer hooks, plus the
`setting` framework (upstream) now consumed by the settings screen.

---

## 3. What shipped in the last session (context for review questions)

**Settings screen (`a929f481f`)** — Electron's settings is a *route*, not a
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
  theme, vimMode, wordWrap (through `set_soft_wrap`), autoSaveEnabled +
  autoSaveDelayMs (the previously hard-coded 500 ms debounce),
  showFileConflictWarning, confirmThreadDelete (gates the delete dialog).
- `crates/vitre-app/src/settings.rs` — the surface, built on the fork's
  `setting` framework (General page + a "Not yet in Vitre" page that *names*
  the nine missing Electron sections — Providers, Keybindings, Language
  servers, Source control, Connections, Knowledge graph, Diagnostics, Beta,
  Archived threads — instead of faking them). Server rows
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

## 4. Next work, in order

### Phase 1 — chat-core catch-up (`docs/vitre-parity.md` §Phase 1) — THE NEXT PHASE

Biggest daily-driver wins; M1 debt. Slices 1.1–1.10 (1.6's derive pipeline
already done 2026-09-05). Highlights: optimistic user messages (1.1), markdown
upgrades — highlighted code blocks with copy, table copy/expand, streaming
smoothing (1.2), slash commands + plan mode (1.3), model-picker completion +
traits picker (1.4), user-bubble enrichment + lightbox (1.5), composer input
parity (1.7), banner stack (1.8), header project segment + toasts (1.9),
`#` thread references + `$` skills (1.10).

### Leftover M3 items (can interleave)

- **2.5 satellites**: terminal links, running indicators, dock terminal
  surface content.
- **2.7d**: editor line annotations for review comments (fork gutter surgery,
  same bucket as the git diff gutter).
- **Right-panel polish** (parity rows near the dock section): dirty-dot per
  tab, sheet mode, maximize control. Ctrl+tab tab-cycling is deliberately
  folded into slice 4.1 (runtime keymap).

### Then

Phase 3 (M4 preview — gated on the wry/WKWebView child-view spike S3), the
rest of Phase 4 (runtime user keymap 4.1 is the big one: when-AST → gpui
predicates, rebuild on `subscribeServerConfig`; settings sections 4.3;
desktop shell 4.4), Phase 5 long tail.

### Known defects / open user questions (all raised, none answered)

1. **Orphaned sidecar on quit** — SIGTERM to the app leaves the Node sidecar
   reparented to launchd, still holding its port (observed repeatedly this
   week; two sidecars on one `state.sqlite` is a real hazard). Needs the
   supervisor's kill-process-group path wired to app termination. Until
   fixed: `pkill -f 'target/debug/vitre'; sleep 2; pkill -f 'apps/server/dist/bin.mjs'`.
2. **Dock tab ↔ in-panel navigation desync** — `FilesPanel` tree clicks don't
   route through the right-panel store, so dock tab labels go stale;
   Electron's `onOpenFile` → `useRightPanelStore.openFile` creates a
   per-path surface. Offered to file+fix; no answer yet.
3. **250k workspace-listing cap** — remove or raise? Asked, unanswered.

---

## 5. How to build, test, run, verify

### Gates (all must be green before a commit)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets          # -D warnings is the CI bar
cargo test --workspace                          # excludes the fork
cargo test -p vitre-app --bin vitre             # vitre-app is BIN-ONLY: never --lib
cargo test --manifest-path third_party/gpui-component/crates/base/Cargo.toml --lib
cargo test --manifest-path third_party/gpui-component/crates/ui/Cargo.toml --lib
cargo deny check licenses
```

CI (`.github/workflows/vitre-ci.yml`) runs MORE than the local list — worth
knowing before a push surprises you:

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
and running fork tests rewrites `third_party/gpui-component/Cargo.lock` with
pure resolver churn — **revert that file before committing**.

### Running

Both launchers `exec ./target/debug/vitre` **without building** — run
`cargo build` (or `cargo run -p vitre-app`) first after code changes.

- `./run-vitre.sh` — dev binary against the real `~/.vitre` home; puts nvm's
  node on PATH (GUI PATH lacks it) and points `VITRE_SERVER_ENTRY` at
  `apps/server/dist/bin.mjs`. Server-side changes need that dist rebuilt
  (`pnpm --filter t3 run build:bundle` — there is no root `build:bundle`
  script). Sidecar port-scans from 3773 (currently lands 3774).
- `./run-vitre-parity.sh` — same, but `VITRE_HOME=/tmp/vitre-parity-home`
  (isolated, seedable). For clean-slate: `VITRE_HOME=$(mktemp -d)`.
- Committed verification hook: `VITRE_OPEN_THREAD=<thread-id>` selects that
  thread once the shell carries it (for sandboxed runs where synthetic clicks
  are dropped).

### Live verification recipe (macOS, sandboxed session)

Synthetic clicks (CGEvent) are dropped and osascript keystrokes fail without
accessibility permission. The working pattern:

1. Add a **temporary, env-gated hook** in the relevant `render` (e.g.
   `VITRE_OPEN_SETTINGS` flipping state once via an `AtomicBool`), verify,
   then **remove the hook before committing**.
2. `swift scripts/vitre/stage-vitre.swift stage` — hides every other regular
   app (gpui does not draw occluded windows: a covered window keeps its last
   frame AND stops running render-driven work, which looks exactly like a
   hang), activates Vitre, prints window ids. `unhide` reverses it.
3. `screencapture -o -x -l <window-id> out.png` (works across Spaces), then
   read the image.
4. **Never mutate real user threads in `~/.vitre`** — read its sqlite with
   `sqlite3 "file:...?mode=ro"`; for write tests copy the home to /tmp and
   run with `VITRE_HOME` pointing there.

In gpui tests: `.debug_selector(|| "...".into())` on an element +
`cx.debug_bounds("...")` after `cx.update(|window, cx| window.draw(cx).clear(cx))`
gives painted geometry; `cx.simulate_keystrokes` needs the target focused and
the relevant `cx.bind_keys` done in the test.

---

## 6. Gotchas that cost real time (condensed; full list in project memory)

**gpui / fork**
- `absolute()` with no inset keeps its *static* flow position — always pair
  with `.top_0().left_0()`. (Broke completion menus for two days.)
- gpui list rows are layout roots; `mx_auto` is a no-op inside them.
- Action bindings dispatch **before** key listeners; at equal depth the
  *later* registration wins — that's how vim (registered after
  `gpui_component::init`, gated on `vim`/`vim_command` key contexts) takes
  escape/enter back from the Input's keymap.
- Dialog *builder* closures run inside `ChatApp::render`; entity reads there
  panic. Use `Rc<Cell>`, touch the app only from content/footer closures.
- `scroll_to_center` only sets `deferred_scroll_offset`, and any later
  selection change recomputes it — centre **last**, and re-centre on
  `window.on_next_frame` when file + reveal arrive together.
- The fork's settings framework dispatches rows by the field's *runtime*
  type; an unsupported pairing is `unimplemented!()` **at paint time**. Draw
  the surface in a test — but only the *selected* page renders.
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

1. `cd .claude/worktrees/vitre-m0` — confirm `git status` is clean on
   `feat/vitre-m3-panels`.
2. Read `docs/vitre-parity.md` §Phase 1 and pick slice 1.1 (optimistic user
   messages) or 1.2 (markdown upgrades) unless the user redirects.
3. Before starting: ask the user whether to fix the **orphaned-sidecar**
   defect first (it bites every manual verification run) and whether the
   dock-desync fix should be filed as its own slice.
4. Keep the discipline: parity rows + tests + live screenshot verification in
   the same commit as the feature; fork `Cargo.lock` churn reverted; temp
   verification hooks removed.
