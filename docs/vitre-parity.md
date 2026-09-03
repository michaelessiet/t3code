# Vitre ↔ Electron feature-parity matrix

Status of the native Rust/GPUI app (`crates/vitre-*`) against the Electron desktop app
(`apps/web` + `apps/desktop`), per the migration plan's parity-QA process. Updated from a
189-row sweep of the Electron codebase on **2026-09-03** (after the add-project flow landed
in `24f497a63`).

**Snapshot: 41 present · 35 partial · 113 absent.** M1 (chat core) and M2 slices 1–5
(files, tree ops, LSP bridge, ⌘S/format, QuickSearch + command palette, sidebar grouping,
add-project) are the "present" mass; the "absent" mass is dominated by whole subsystems the
master plan schedules as M3+ (terminal, diff panel, preview, settings, source control,
knowledge graph, multi-environment). M3 slice 2.1 (right-panel dock) landed 2026-09-03.

How to update: flip a row's status in the same PR that changes it; add a `Done <date> (<sha>)`
note. Rows that will never be ported get status `waived` with a written reason (e.g. WSL
sources before a Windows build).

---

## Prioritized roadmap

Ordering is by daily-driver impact and dependency structure, mapped onto the master plan's
milestones (`~/.claude/plans/ok-so-i-d-like-floating-owl.md`). Sizes are the sweep's
S/M/L/XL estimates.

### Phase 0 — finish M2 (committed scope, in progress)

| # | Slice | Size | Key Electron refs |
|---|---|---|---|
| 0.1 | **Git diff gutter**: live HEAD diff, staged-hollow/unstaged-solid via index baseline, click peek with revert + hunk nav, overview ruler | XL | `apps/web/src/components/files/codemirror/gitDiffGutter.ts`, `gitDiffBaselineState.ts` (vcs.getFileBaseline) — needs custom gutter painting in the gpui-component fork |
| 0.2 | **Vim subset** (exactly the CM6 set): normal/insert/visual, hjkl w/b/e 0/$ gg/G, d/c/y/p, `gh` hover, `gd` def, `:w` save; vim tree nav j/k/h/l | XL | `apps/web/src/components/files/codemirror/CodeMirrorFileEditor.tsx` (@replit/codemirror-vim usage) — modal layer in the fork editor |
| 0.3 | **M2 polish bundle**: reveal-open-file-in-tree (expand ancestors + scroll), reveal-line range highlight + centring in the main editor, F12 go-to-definition chord, mod+i manual completion trigger, empty-open-thread placeholder, files-panel open-state persistence | S–M each | `FileBrowserPanel.tsx`, `revealLine.ts`, `lspBridge.ts`, `FilePreviewPanel.tsx` |

### Phase 1 — chat-core catch-up (M1 debt; biggest daily-driver wins)

| # | Slice | Size | Key Electron refs |
|---|---|---|---|
| 1.1 | Optimistic user messages + local dispatch snapshot (message appears on send, not on projection echo) | L | `ChatView.logic.ts` |
| 1.2 | Markdown upgrades: syntax-highlighted code blocks with copy + title bar, table copy/expand, streaming smoothing | XL | `ChatMarkdown.tsx` |
| 1.3 | Slash commands + plan mode: `/` menu (built-ins + provider commands), Build/Plan footer toggle, runtime-mode selector, proposed-plan timeline card, Implement/Refine follow-up banner | L+M+M+L+L | `composerSlashCommandSearch.ts`, `ChatComposer.tsx`, `ProposedPlanCard.tsx`, `ComposerPlanFollowUpBanner.tsx` |
| 1.4 | Model picker completion (search, provider-instance rail, favorites, locked-provider mode) + traits picker (reasoning effort / ultrathink) | XL+L | `ModelPickerContent.tsx`, `TraitsPicker.tsx` |
| 1.5 | User-message bubble enrichment (attachment grids, chips, collapse, hover copy/timestamp) + image lightbox | L+M | `MessagesTimeline.tsx` (UserTimelineRow), `ExpandedImageDialog.tsx` |
| 1.6 | Timeline dynamics: working elapsed timer (S), work-log "+N previous" grouping (M), turn folding (L), anchored-send scroll mode (L), minimap rail (L) | S–L | `MessagesTimeline.logic.ts`, `timelineScrollAnchoring.ts` |
| 1.7 | Composer input parity: paste/drag-drop attachments, attachments-only send, send-button busy/disabled states, type-to-focus, mention arrow-nav + loading states | S–M each | `ChatComposer.tsx`, `ComposerPrimaryActions.tsx`, `ComposerCommandMenu.tsx` |
| 1.8 | Banner stack: provider status banner, thread-error polish (dismiss/clamp/sanitize), env-unavailable + version-mismatch banners (branch-mismatch waits on git subsystem) | M | `ComposerBannerStack.tsx`, `ProviderStatusBanner.tsx` |
| 1.9 | Chat header project segment (favicon + name / title tooltip) + toast-system upgrade (stacking, thread-scoped routing, actions) | S+M | `ChatHeader.tsx`, `ui/toast.tsx` |
| 1.10 | `#` thread-reference trigger + chip rendering, `$` skills trigger + inline chips | M+M | `ChatComposer.tsx`, `SkillInlineText.tsx`, `packages/shared/src/threadReferences.ts` |

### Phase 2 — M3: right panel, diff, terminal, source control

Right-panel dock/tabs is the structural prerequisite for everything else here.

| # | Slice | Size | Key Electron refs |
|---|---|---|---|
| 2.1 | **Done 2026-09-03** — Right-panel dock: multi-surface tab strip, per-thread persistence, close/cycle chords, resize + width persistence | XL+S | `rightPanelStore.ts`, `RightPanelTabs.tsx` |
| 2.2 | Diff engine: Rust imara-diff + virtualized GPUI renderer, unified/split, intra-line highlights, tree-sitter syntax | XL | `DiffWorkerPoolProvider.tsx` (@pierre/diffs — clean-room; note the sticky-inset gotcha memory) |
| 2.3 | Diff panel shell: branch vs per-turn modes, base-ref combobox, collapse/wrap/whitespace toggles, open-in-editor | L+S+S+S | `DiffPanel.tsx`, `baseRefChoices.ts`, `diffCollapse.ts` |
| 2.4 | Turn-diff summaries + changed-files card under assistant messages (chat's top absent feature; unblocked by 2.2) | M+XL | `useTurnDiffSummaries.ts`, `ChangedFilesTree.tsx` |
| 2.5 | Terminal: alacritty_terminal PTY-less grid renderer + attach/write/resize lifecycle, input arbitration, links, selection→chat-context, tabs/splits, theme sync, running indicators | XL core + M satellites | `ThreadTerminalDrawer.tsx`, `state/terminalSessions.ts`, `terminal-links.ts`, `terminalUiStateStore.ts` |
| 2.6 | Source control: branch toolbar (refs/worktree env mode), git actions (commit/push/PR with staged progress), PR-thread dialog, worktree cleanup | L+L+M+S | `BranchToolbar.tsx`, `GitActionsControl.tsx` (logic.ts is pure/portable), `PullRequestThreadDialog.tsx` |
| 2.7 | Review comments: line-range annotation in editor + diff feeding composer context | XL | `reviewComments.ts`, `AnnotatableCodeView.tsx` (z-20 resize-handle gotcha memory) |
| 2.8 | Terminal contexts + project scripts control (needs 2.5) | XL+L | `ComposerPendingTerminalContexts.tsx`, `ProjectScriptsControl.tsx` |

### Phase 3 — M4: preview subsystem

Gated on the wry/WKWebView child-view spike (S3 in the master plan); the Tauri-era
`preview.rs` port is the head start.

| # | Slice | Size | Key Electron refs |
|---|---|---|---|
| 3.1 | Webview host embedded under gpui layout | XL | `apps/desktop/src/preview/Manager.ts` |
| 3.2 | Session lifecycle + browser chrome, discovered local servers, device toolbar/zoom/appearance | L+M+L | `PreviewChromeRow.tsx`, `useDiscoveredLocalServers.ts`, `BrowserDeviceToolbar.tsx` |
| 3.3 | Automation (ghost cursor, `preview_*` MCP), element pick → composer annotation, recording, mini player, crash recovery | L×3+M+S | `PreviewAutomationHosts.tsx`, `PickPreload.ts`, `browserRecording.ts`, `ThreadPreviewMiniPlayer.tsx` |

### Phase 4 — M5: settings, keybindings, shell polish

| # | Slice | Size | Key Electron refs |
|---|---|---|---|
| 4.1 | Runtime user keymap: translate server-config keybindings (when-AST → gpui predicates) into `cx.bind_keys`, rebuild on subscribeServerConfig; plus the unported default chords (mod+n, mod+w, ctrl+tab, mod+shift+e, f2, mod+o, mod+i, mod+d, ⌘1–9 thread jumps) | XL | `packages/shared/src/keybindings.ts`, `apps/web/src/keybindings.ts` |
| 4.2 | Settings shell + General panel (theme, timestamps, autosave modes, default model, word wrap, editor font size) | L+L | `SettingsSidebarNav.tsx`, `SettingsPanels.tsx` — unblocks the editor's autosave/wrap/font settings rows |
| 4.3 | Providers settings (instance cards, add wizard, models, accents), keybindings editor UI, language servers, source-control settings, archived threads, beta flags, diagnostics | XL+L+M+M+M+S+L | `settings/*` |
| 4.4 | Desktop shell: app menu bar, window-bounds persistence + splash, `vitre://` deep links, native input context menus, explicit theme setting, updater UI | M each, L updater | `DesktopApplicationMenu.ts`, `DesktopWindow.ts`, `ElectronProtocol.ts`, `desktopUpdate.logic.ts` |
| 4.5 | Notifications: slow-RPC watchdog, keybindings-update toast, provider-update pills, Claude binary install dialog | S+S+M+M | `SlowRpcRequestToastCoordinator.tsx`, `ProviderUpdateLaunchNotification.tsx`, `ClaudeBinaryInstallDialog.tsx` |

### Phase 5 — long tail (M8) and deferred tracks

- **Drafts**: composer draft store, draft routes/hero, index auto-draft landing (XL+M+M) — `composerDraftStore.ts`.
- **Knowledge graph**: state machine (M), canvas — custom force-layout GPU rendering (XL), sidebar queries (L) — `graph/*`.
- **Multi-root threads**: multi-root file browser (XL), workspace-roots control (M), git-root switcher (S) — `MultiRootFileBrowser.tsx`.
- **Search panel** (persistent, with replace-in-files; distinct from QuickSearch) (XL; replace logic in `SearchPanel.logic.ts` is pure/portable).
- **File-tree extras**: chat integrations (Copy Mention / Add to Chat / cross-thread copy) (L), drag-to-composer mention (M), keyboard nav (M), filename filter (M), header metadata (S), conflict Compare… dialog (M), markdown preview (L), image preview (M).
- **Thread snooze** + settled banners (M), glass opacity (M).
- **Deferred until the remote-environments milestone**: multi-environment catalog (XL), Connections settings (XL), Clerk sign-in (XL), cloud relay (XL), SSH (XL), WSL (XL), hosted pairing UI, version-skew prompt.

---

## Matrix

Statuses: ✅ present (behavior parity, divergences noted) · 🟡 partial · ❌ absent.
Sizes are porting-effort estimates: S < 1d, M ~1–3d, L ~1wk, XL > 1wk.

### approvals

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Pending-approval panel: PENDING APPROVAL eyebrow, kind summary, 1/N count, mono detail box | `apps/web/src/components/chat/ComposerPendingApprovalPanel.tsx` | `crates/vitre-app/src/chat.rs` | ✅ present | S | Vitre's render_approval_panel matches: command/file-read/file-change summaries, detail label, scrollable mono detail, queue count. Electron additionally mirrors the detail into the disabled prompt editor's placeholder. |
| Approval actions: Cancel turn / Decline / Always allow this session / Approve once | `apps/web/src/components/chat/ComposerPendingApprovalActions.tsx` | `crates/vitre-app/src/chat.rs` | ✅ present | S | Vitre replaces the composer footer with the four buttons, disables while ThreadApprovalRespond is in flight (responding set), same decisions cancel/decline/acceptForSession/accept. |

### auth

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Pairing route (token entry/consume, hosted pairing surface) | `apps/web/src/components/auth/PairingRouteSurface.tsx` | `crates/vitre-rpc/src/http.rs` | 🟡 partial | L | Vitre already does the desktop-local equivalent: bootstrap-token exchange → ws ticket → RPC session (exchange_bootstrap_token/websocket_ticket). The /pair UI, hostedPairing.ts URL flows, and authGateState routing are absent and only needed for remote/hosted environments. |
| T3 Connect (Clerk) sign-in + account/mobile-clients profile | `apps/web/src/components/clerk/T3ConnectSidebarSignIn.tsx` | — | ❌ absent | XL | Clerk-managed auth (useT3ConnectAuthPrompt, MobileClientsUserProfilePage); desktop side DesktopClerk.ts. Native port needs a system-browser OAuth flow + keychain token storage; XL and out of local-only scope. |
| Cloud environment connect (relay onboarding, relay client install, /connect CLI auth) | `apps/web/src/components/cloud/ConnectOnboardingDialog.tsx` | — | ❌ absent | XL | cloud.getRelayClientStatus/installRelayClient RPCs, src/cloud/* (dpop, managedAuth, linkEnvironment), routes/connect.tsx + connect_.callback.tsx. Out of scope for local-single-environment Vitre; catalog only. |
| SSH remote environments (host add, password prompt) | `apps/desktop/src/ssh/DesktopSshEnvironment.ts` | — | ❌ absent | XL | Renderer: components/desktop/SshPasswordPromptDialog.tsx + state/desktopSshHosts.ts; entirely desktop-main-process (ipc/methods/sshEnvironment.ts). Multi-environment prerequisite. |
| WSL environments (Windows) | `apps/desktop/src/wsl/DesktopWslEnvironment.ts` | — | ❌ absent | XL | state/desktopWslState.ts + wslPaths.ts in renderer; Windows-only, irrelevant until a Windows Vitre build exists. |
| Version skew detection + manual server update prompt | `apps/web/src/versionSkew.ts` | — | ❌ absent | M | Compares client/server versions from server.getConfig + subscribeServerLifecycle; ServerUpdateAction.tsx runs server.updateServer with 12min pending expiry. For Vitre (bundled sidecar) this reduces to an assert, but hosted/remote parity needs it. |

### banners

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Composer banner stack: env-unavailable+Reconnect, version mismatch+Update, branch mismatch+Switch, settled/snoozed | `apps/web/src/components/chat/ComposerBannerStack.tsx` | — | ❌ absent | L | Stacked dismissible alerts above the composer with hover-expand; branch-mismatch banner offers 'Switch branch' with uncommitted-changes confirm dialog (resolveLocalCheckoutBranchMismatch); version mismatch wires ServerUpdateAction. Vitre has no equivalent surface. |
| Provider status banner (unauthenticated / warning / error) above composer | `apps/web/src/components/chat/ProviderStatusBanner.tsx` | — | ❌ absent | M | Keyed dismissal (getProviderStatusBannerKey), 'Sign in via the CLI to authenticate again' for unauthenticated. Vitre surfaces no provider health at all. |
| Thread error banner | `apps/web/src/components/chat/ThreadErrorBanner.tsx` | `crates/vitre-app/src/chat.rs` | 🟡 partial | S | Vitre shows an inline danger box from last_error or session.last_error under the header. Missing: dismiss button, line-clamp with full-text tooltip, sanitizeThreadErrorMessage, draft-vs-thread error map racing (LocalThreadErrorEntry). |

### chat-header

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Chat header: project favicon + name / thread title with tooltip | `apps/web/src/components/chat/ChatHeader.tsx` | `crates/vitre-app/src/chat.rs` | 🟡 partial | S | Vitre header shows only the shell thread title (truncated) and a files-panel toggle. Missing: ProjectFavicon + project name leading segment with '/' separator, title tooltip. |
| Header actions: workspace roots, project scripts, Open In picker, git actions | `apps/web/src/components/chat/ChatHeader.tsx` | — | ❌ absent | XL | WorkspaceRootsControl (multi-root attach), ProjectScriptsControl (run scripts into thread terminal with keybindings, file-based scripts via useT3ProjectFileScripts), OpenInPicker (external editors, primary-env gated), GitActionsControl (commit/PR affordances). None exist in Vitre; several depend on terminal/git subsystems Vitre lacks. |

### checkpoints

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Revert to a user message (ThreadCheckpointRevert with confirm) | `apps/web/src/components/ChatView.tsx` | `crates/vitre-app/src/chat.rs` | ✅ present | S | Vitre: hover Undo2 button per user message, two-step arm ('Revert?') instead of Electron's native confirm dialog, revert target = next assistant checkpoint's turnCount-1 (port of revertTurnCountByUserMessageId), blocks while running with error text. Electron also infers checkpoint turn counts for summaries lacking them (inferredCheckpointTurnCountByTurnId). |

### command-palette

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Command palette root (⇧⌘P): Actions + Recent Threads (12); typed query swaps to full Projects/Threads corpora with Electron ranking | `apps/web/src/components/CommandPalette.tsx` | `crates/vitre-app/src/palette/command_palette.rs` | ✅ present | S | rank.rs ports normalizeSearchText / rankSearchFieldMatch (exact3/prefix2/substring1, first-matching-term dominates 1000-index*100). Snapshot-at-open model matches Electron. Unit-tested against the Electron rules. |
| '>' actions-only prefix filter and Backspace-pops-submenu, footer key hints | `apps/web/src/components/CommandPalette.logic.ts` | `crates/vitre-app/src/palette/command_palette.rs` | ✅ present | S | capture_key_down intercepts Backspace at empty query (query field would swallow it). Footer mirrors CommandFooter minus browse hints. Escape closes on first press (cancel_clears_query(false)). |
| 'New thread in {project}' + 'New thread in…' submenu; project rows open latest thread or start one | `apps/web/src/components/CommandPalette.tsx` | `crates/vitre-app/src/palette/command_palette.rs` | ✅ present | S | PaletteAction::NewThread{project_id:None} = startNewThreadFromContext; OpenProject = openProjectFromSearch. Divergence: Electron's project rows come from logical-project sidebar grouping (buildSidebarProjectPickerEntries); Vitre uses the raw project list. |
| Workspace action rows: Quick open, Search chats and files, Toggle right panel, New file, New folder (gated on active thread) | `apps/web/src/components/CommandPalette.tsx` | `crates/vitre-app/src/palette/command_palette.rs` | ✅ present | M | Same searchTerms strings ported. New file/folder route to FilesPanel::create_at_root (workspace root) whereas Electron's fileTree.newFile targets the tree's focused directory. Missing rows (subsystems absent in Vitre): Toggle terminal, Focus file tree, Manage workspace roots, knowledge graph pair, Open settings, Add project (add-project mode is another agent's scope). |
| Shortcut chips resolved from the live keymap | `apps/web/src/components/CommandPalette.tsx` | `crates/vitre-app/src/palette/command_palette.rs` | ✅ present | S | shortcut_for uses window.highest_precedence_binding_for_action + Kbd::format, so rebinding in main.rs updates chips automatically — same contract as Electron's shortcutLabelForCommand. |
| ⌘1–⌘9 positional project jump (thread.jump.N) | `apps/web/src/components/CommandPalette.logic.ts` | `crates/vitre-app/src/palette/command_palette.rs` | 🟡 partial | M | Vitre renders hard-coded '⌘N' chips on the first nine project rows (project_items) but no thread.jump actions exist anywhere — the chords are not bound globally or inside the palette, so the chips advertise shortcuts that do nothing. Electron binds mod+1..9 globally via THREAD_JUMP_KEYBINDING_COMMANDS. |
| Full command palette command set | `apps/web/src/components/CommandPalette.tsx` | `crates/vitre-app/src/palette/command_palette.rs` | 🟡 partial | M | Vitre has: new thread, new-thread-in… submenu, quick open, content search, toggle files panel, add project (browse/clone), projects/threads groups, shortcut chips from live keymap. Electron adds: new file/folder (Vitre has these), open/build knowledge graph, open settings, toggle terminal, toggle right panel, focus file tree, manage workspace roots, project scripts. Extend as subsystems land. |

### composer

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Composer prompt editor with Enter-to-send / Shift+Enter newline and auto-grow | `apps/web/src/components/ComposerPromptEditor.tsx` | `crates/vitre-app/src/chat.rs` | ✅ present | S | Vitre Textarea auto_grow(1,8), PressEnter{shift:false} sends (or accepts top mention). Electron's editor additionally renders inline terminal-context chips and $skill tokens inside the text — those are absent (tracked separately). |
| @-mention popover searching workspace files via projects.searchEntries | `apps/web/src/components/chat/ComposerCommandMenu.tsx` | `crates/vitre-app/src/chat.rs` | 🟡 partial | M | Vitre: token parser (start/whitespace-gated, quote-aware), generation-counter search (ProjectsSearchEntries limit 8) rooted at worktree-or-workspace-root, Enter accepts top row, click applies, quoted insert for spaced paths. Missing: arrow-key navigation/highlight of rows, multi-root search with root labels (useMultiRootComposerPathSearch), loading state, drag-file-from-tree to insert mention (composerMentionDrag), directory vs file descriptions with parent path. |
| # thread-reference trigger listing environment threads ranked by project/recency | `apps/web/src/components/chat/ChatComposer.tsx` | — | ❌ absent | M | composerTrigger kind 'thread': filters non-archived shells, prefix-match boost, same-project boost, 20 cap, cross-project label + relative time; inserts [#Title](t3code://thread/id). |
| Attachments via file picker with caps (8 files / 10MB) and removable chips | `apps/web/src/components/chat/ChatComposer.tsx` | `crates/vitre-app/src/chat.rs` | ✅ present | S | Vitre: native dialog, MIME inference port, data-URL TurnAttachment::Image/File on ThreadTurnStart, name+size chips with remove, error notifications. Divergence: chips show an icon, not image thumbnails; Electron shows 64px image previews and a 'draft attachment may not persist' warning. |
| Paste and drag-drop images/files into the composer | `apps/web/src/components/chat/ChatComposer.tsx` | — | ❌ absent | M | onComposerPaste + drag enter/over/drop handlers with drop-target ring styling; PROVIDER_SEND_TURN_MAX_IMAGE_BYTES limit label. Vitre only has the file dialog. |
| Send with attachments only (no text) via bootstrap prompt | `apps/web/src/components/ChatView.tsx` | — | ❌ absent | S | IMAGE_ONLY_BOOTSTRAP_PROMPT substitutes when only files are attached; hasSendableContent counts attachments. Vitre's send() returns early on empty text even with staged attachments. |
| Send / stop primary button with busy, connecting, and disabled-reason states | `apps/web/src/components/chat/ComposerPrimaryActions.tsx` | `crates/vitre-app/src/chat.rs` | 🟡 partial | S | Vitre: circular send (ArrowUp) swaps to red stop square while running; dispatches ThreadTurnInterrupt. Missing: spinner while isSendBusy/isConnecting, disabled when env unavailable / no provider / no sendable content, aria-labels per state, 'Preparing worktree...' hint. |
| Optimistic user messages + local dispatch snapshot (message appears instantly on send) | `apps/web/src/components/ChatView.logic.ts` | — | ❌ absent | L | createLocalDispatchSnapshot/hasServerAcknowledgedLocalDispatch drive optimisticUserMessages merged into the timeline plus attachment blob-preview handoff to server URLs. Vitre waits for the projection echo, so a sent message appears only after the server round-trip. |
| Send blocked while an approval or user-input request is pending | `apps/web/src/components/ChatView.tsx` | `crates/vitre-app/src/chat.rs` | ✅ present | S | Both derive pending requests from the activity log (derive_pending_approvals / derive_pending_user_inputs in vitre-state/session_logic.rs) and give the composer's primary action to the request. |
| Type-to-focus: printable keys anywhere focus the composer | `apps/web/src/components/ChatView.tsx` | — | ❌ absent | M | shouldTypeToFocusComposer filters editable/interactive/floating-layer targets then focuses and inserts. Vitre requires clicking into the Textarea; its on_focus_lost restore targets the shell focus handle, not the composer. |
| Compact footer controls: responsive collapse into CompactComposerControlsMenu | `apps/web/src/components/chat/CompactComposerControlsMenu.tsx` | — | ❌ absent | M | ResizeObserver-driven footer compaction merges traits/runtime/plan toggles into one menu; mobile collapsed composer with expand transition (draftHeroTransition). Vitre footer is fixed-layout. |

### context-meter

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Context-window usage ring beside send with detail on hover | `apps/web/src/components/chat/ContextWindowMeter.tsx` | `crates/vitre-app/src/chat.rs` | ✅ present | S | Both derive from latest context-window.updated activity (derive_latest_context_window_snapshot); >90% turns red. Vitre packs percent, used/max, total processed, compacts-automatically into a tooltip; Electron uses a popover with a progress bar and provider display name. |

### desktop-shell

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Application menu bar (Check for Updates, Settings, zoom roles, standard menus) | `apps/desktop/src/window/DesktopApplicationMenu.ts` | — | ❌ absent | M | App menu with update-check feedback dialogs, Settings… items, View zoom roles (CmdOrCtrl+= etc.). Vitre has no Menu setup in main.rs; gpui supports native macOS menus (cx.set_menus). |
| Window management: bounds persistence, splash window, reveal-or-create | `apps/desktop/src/window/DesktopWindow.ts` | `crates/vitre-app/src/main.rs` | 🟡 partial | M | Vitre has titlebar parity (hiddenInset, traffic lights at 16,18) and system-appearance sync but opens a fixed centered 1100x720 every launch; Electron persists bounds, shows a splash window (SplashScreen.tsx) until renderer ready, and revealOrCreateMain on dock click. Single main window in both — no true multi-window to port. |
| Glass/translucency styling (glassOpacity setting) | `apps/web/src/components/SidebarV2.tsx` | — | ❌ absent | M | MIN/MAX_GLASS_OPACITY in contracts settings drive CSS translucency in sidebar/root (no Electron vibrancy API on this branch). GPUI can do window blur/transparency natively; low priority per UI-parity directive (pure-black dark variant is what Vitre themes transcribe). |
| Theme control: manual light/dark/system + UI zoom | `apps/web/src/hooks/useTheme.ts` | `crates/vitre-app/src/main.rs` | 🟡 partial | S | Vitre only syncs system appearance (sync_system_appearance + observer). Missing: explicit theme setting persisted in ClientSettings, ElectronTheme nativeTheme coordination, and editor/UI zoom (lib/desktopEditorZoom.ts, menu zoom roles). |
| Deep-link protocol handler (t3code:// URLs) | `apps/desktop/src/electron/ElectronProtocol.ts` | — | ❌ absent | M | Protocol registration + routing into app actions (pairing callbacks, thread references t3code:// tokens per memory). macOS: Info.plist URL types + gpui on_open_urls. |
| Native context-menu fallback for standard text editing | `apps/web/src/contextMenuFallback.ts` | `crates/vitre-app/src/chat/project_actions.rs` | 🟡 partial | S | Vitre has gpui popup context menus for sidebar project rows (rename/grouping/remove dialogs) but no general cut/copy/paste context menu on inputs. |

### diff-panel

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Diff panel shell: branch-diff vs per-turn checkpoint modes | `apps/web/src/components/DiffPanel.tsx` | — | ❌ absent | L | 981-line panel: DiffPanelSelection {kind branch\\|turn} persisted per thread (apps/web/src/diffPanelStore.ts); turn selector with timestamps, file list, empty/loading states (DiffPanelShell.tsx). Data via review.getDiffPreview + vcs.getFileBaseline/getFileStatuses. |
| Diff rendering engine: unified(stacked)/split, intra-line highlights, syntax highlighting | `apps/web/src/components/DiffWorkerPoolProvider.tsx` | — | ❌ absent | XL | @pierre/diffs CodeView in a web-worker pool does patch parsing, intra-line word diffs, syntax highlight, virtualized sticky file headers (see memory: per-frame sticky insets gotcha). Native port = Rust diff (similar/imara-diff) + tree-sitter highlight + custom virtualized GPUI element; biggest single diff-area item. |
| Base ref selection (automatic base + searchable ref combobox) | `apps/web/src/lib/baseRefChoices.ts` | — | ❌ absent | S | buildBaseRefChoices/filterBaseRefChoices over vcs.listRefs; AUTOMATIC_BASE_REF sentinel; per-thread baseRef persistence in diffPanelStore. |
| Diff view options: collapse all/expand all, wrap, whitespace toggle | `apps/web/src/lib/diffCollapse.ts` | — | ❌ absent | S | Per-file collapse map + areAllDiffFilesCollapsed/toggleAllDiffFiles; TextWrap and Pilcrow (whitespace) toggles, stacked/split ToggleGroup in DiffPanel.tsx toolbar. |
| Turn diff summaries (changed-file chips per turn in chat) | `apps/web/src/hooks/useTurnDiffSummaries.ts` | — | ❌ absent | M | review.getDiffPreview per TurnId; lib/turnDiffTree.ts groups files into a tree; chat rows link into the diff panel at that turn. |
| Review comments: annotate diff/file lines into composer context | `apps/web/src/components/diffs/AnnotatableCodeView.tsx` | — | ❌ absent | L | Line-range selection over CodeView, LocalCommentAnnotation (components/files/) renders comment cards, reviewCommentContext.ts serializes them into the next prompt. Gotcha from memory: root listener needs stopPropagation vs resize-handle overlay. |
| Diff file actions: open in editor / preferred external editor | `apps/web/src/diffFileActions.ts` | — | ❌ absent | S | openDiffFilePrimaryAction + editorPreferences.ts (shell.openInEditor RPC) with per-user preferred editor. |
| Git root switcher for multi-root threads | `apps/web/src/components/GitRootSwitcher.tsx` | — | ❌ absent | S | gitRootStore.ts selects which workspace root the diff/source-control views scope to. |
| Checkpoint diff state (revert-point diffs) | `apps/web/src/lib/checkpointDiffState.ts` | — | ❌ absent | M | useCheckpointDiff powers turn-scoped checkpoint comparisons and revert affordances in chat. |

### drafts

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Draft threads: local composer draft store, draft routes, promote-on-send | `apps/web/src/routes/_chat.draft.$draftId.tsx` | — | ❌ absent | XL | Drafts live in composerDraftStore (prompt, images, env mode, model persist per draftId), route /draft/$draftId renders ChatView routeKind='draft', promotion navigates to /$environmentId/$threadId after hero transition. Vitre's NewThread dispatches ThreadCreate immediately with title 'New thread' — no draft state, no deferred creation. |
| Draft hero headline 'What should we build in {project}?' with inline project switcher | `apps/web/src/components/chat/DraftHeroHeadline.tsx` | — | ❌ absent | M | Dotted-underline project menu (grouped/sorted like sidebar), 'New project' item opens command palette add-project; hero→timeline FLIP animation (draftHeroTransition.ts). Index route auto-opens a draft for the most recent project (_chat.index.tsx). |

### drag-drop

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| OS file drop onto composer → attachments | `apps/web/src/components/chat/ChatComposer.tsx` | `crates/vitre-app/src/chat.rs` | 🟡 partial | S | Vitre attaches via file picker (attach_files, assets.createUrl path). Electron accepts dataTransfer Files drop with copy dropEffect and drag-over highlight (ChatComposer.tsx:2065-2094). GPUI has ExternalPaths drag support. |
| File-tree row drag → composer mention chip | `apps/web/src/components/files/fileTreeDragMention.ts` | — | ❌ absent | M | Capture-phase dragstart tags tree drags with composer-mention payload (multi-selection aware); composerMentionDrag.ts consumes on composer drop. Needs GPUI on_drag with custom drag type between files panel and composer. |

### editor

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Code editor surface: line numbers, tree-sitter syntax highlighting, Electron-token theme (light/dark) | `apps/web/src/components/files/codemirror/CodeMirrorFileEditor.tsx` | `crates/vitre-app/src/files.rs` | ✅ present | S | gpui-component fork EditorState with set_highlighter(language_for_path); themes/vitre.json transcribes index.css tokens + codemirror/theme.ts highlight block (per memory: without it the editor kept the default light highlight theme). languages.ts extension mapping approximated by fork registry (Makefile→make special-case only). |
| Debounced autosave (500ms afterDelay) with dirty indicator | `apps/web/src/components/files/fileSaveCoordinator.ts` | `crates/vitre-app/src/files.rs` | 🟡 partial | M | Vitre hard-codes always-on afterDelay/500ms (AUTOSAVE_DEBOUNCE) with a dirty dot in the header. Missing Electron settings surface: autoSaveEnabled toggle, onFocusChange mode (flush on editor/window blur), configurable delay, manual mode with dirty-buffer survival across unmounts, and flush-on-enable. |
| Cmd+S manual save (file.save; flushes debounce; no-op when clean or write in flight) | `apps/web/src/components/files/fileSaveBus.ts` | `crates/vitre-app/src/files.rs` | ✅ present | S | SaveFile action bound to mod+s in main.rs, dispatches up focus path to FilesPanel::save_now → flush → FileBuffer::begin_save. Electron routes through registerActiveFileSave + global command with when '!terminalFocus'. |
| Save conflict machine: baseRevision-guarded writes, stale_revision + external-change conflict banner with Reload-from-disk / Keep-my-version | `apps/web/src/components/files/fileBufferConflict.ts` | `crates/vitre-state/src/file_buffer.rs` | ✅ present | M | FileBuffer ports the full state machine: self-written revision FIFO (SELF_WRITTEN_REVISION_LIMIT), resave-after-mid-save-edit, deferred watcher events during in-flight save, resolve_keep_mine unconditional write. RPC: projects.writeFile with baseRevision; failure ProjectFileFailure::StaleRevision. |
| Conflict 'Compare…' dialog (unsaved buffer vs on-disk diff before choosing resolution) | `apps/web/src/components/files/FileConflictCompareDialog.tsx` | — | ❌ absent | M | Vitre banner offers only Reload/Keep. Electron also has a settings toggle to silence the banner (showFileConflictWarning) while detection continues — also absent. |
| External-change reload of clean buffer via workspace watcher (self-written revisions ignored) | `apps/web/src/components/files/projectFilesQueryState.ts` | `crates/vitre-app/src/files.rs` | ✅ present | S | workspace_changed re-reads open file on watch events naming it (or Overflow); FileBuffer::disk_changed decides ignore/reload/conflict. Durable subscription loop with RESUBSCRIBE_AFTER_COMPLETION guard. RPC: projects.subscribeWorkspaceChanges. |
| Truncated >1MB file: read-only view with banner | `apps/web/src/components/files/FilePreviewPanel.tsx` | `crates/vitre-app/src/files.rs` | ✅ present | S | Electron swaps to Pierre VirtualizedFile viewer; Vitre keeps the same editor with .readonly(truncated) + banner and skips LSP attach. Behavior parity, different rendering. |
| Shift-Alt-F format document via LSP (result left unsaved; autosave/⌘S persists) | `apps/web/src/components/files/codemirror/useLspBridge.ts` | `crates/vitre-app/src/files.rs` | ✅ present | S | format_document flushes pending didChange first (lsp.flush_document), then lsp.format, applies edits via apply_lsp_edits and manually re-fires editor_edited (apply_lsp_edits emits no change event — gotcha documented in code). Bound in main.rs as fixed chord, matching Electron's non-rebindable editor keymap. |
| Word-wrap setting for the file editor | `apps/web/src/components/files/FilePreviewPanel.tsx` | — | ❌ absent | S | Electron reads clientSettings.wordWrap (EditorView.lineWrapping compartment). Fork EditorState supports soft_wrap (QuickSearch preview sets soft_wrap(false)) so the port is mostly a settings plumbing question — Vitre has no client-settings surface yet. |
| Editor font size Cmd/Ctrl +/-/0 with localStorage persistence and desktop-zoom yielding | `apps/web/src/components/files/codemirror/fontSize.ts` | — | ❌ absent | M | Electron clamps 8–32px, persists t3code:editor-font-size, and reports editor focus to the desktop shell so window zoom yields (desktopEditorZoom). No equivalent in Vitre. |
| Vim mode (buffer editing, gd/gh/C-] LSP actions, :w save routing, vim tree navigation) | `apps/web/src/components/files/codemirror/CodeMirrorFileEditor.tsx` | — | ❌ absent | XL | Known absent (user directive: part of remaining M2). Electron uses @replit/codemirror-vim with Vim.defineAction lspHover/lspDefinition and CodeMirror.commands.save → requestDocumentSave. Fork editor has no modal-editing layer; this is a large engine feature. |
| Git diff gutter (live HEAD diff, staged-hollow/unstaged-solid via index baseline, click peek widget with revert + prev/next hunk, scrollbar overview ruler, Alt-F5/Shift-Alt-F5 hunk nav) | `apps/web/src/components/files/codemirror/gitDiffGutter.ts` | — | ❌ absent | XL | Known absent (remaining M2 item). 727-line CM6 extension: debounced 250ms recompute, 20k-line cap, CRLF-safe baseline split (gitDiffBaselineText), revert.hunk userEvent, baseline fed from gitDiffBaselineState.ts (vcs HEAD/index oid+contents RPC). Needs custom GPUI gutter painting in the fork editor. |
| In-editor find/replace panel | `apps/web/src/components/files/codemirror/CodeMirrorFileEditor.tsx` | `third_party/gpui-component/crates/base/src/input/editor/search.rs` | ✅ present | S | Framework-provided: fork binds cmd-f Search and cmd-shift-f Replace in the Input context (base/src/input/base/state.rs:262-266). Gotcha: the fork's cmd-shift-f Replace shadows Vitre's global cmd-shift-f QuickSearchContent whenever the editor is focused — a real divergence from Electron where mod+shift+f is always content search. |
| Reveal line with range highlight and centered scroll (from search/gd/file links; cm-reveal-line decoration, 500-line cap, selection spans range) | `apps/web/src/components/files/codemirror/revealLine.ts` | `crates/vitre-app/src/files.rs` | 🟡 partial | M | FilesPanel::reveal only sets the cursor (set_cursor_position) — no persistent line-range highlight, no explicit y-center in the main editor. QuickSearch's preview does centre (palette/preview.rs centre_on_line) and relies on active-line background; the main editor jump has neither highlight nor centring. |
| Review comments / line annotations feeding the chat composer (line-range selection, draft comment forms, annotation remap on edit) | `apps/web/src/components/files/codemirror/reviewComments.ts` | — | ❌ absent | XL | Electron: reviewCommentsExtension + fileCommentAnnotations.ts + LocalCommentAnnotation.tsx, wired to composerDraftStore.addReviewComment. Depends on Vitre composer draft model; also the resize-handle z-20 gutter-click gotcha from memory lives here. |
| Markdown rendered preview toggle (+ task-checkbox toggling persisted with prompt-save scheduling) | `apps/web/src/components/files/FilePreviewPanel.tsx` | — | ❌ absent | L | RenderedMarkdownSurface + filePreviewMode.ts setMarkdownTaskChecked; stale-save resolution reloads+toasts. Vitre opens .md as source only. Fork has a markdown TextView that could render it. |
| Workspace image preview | `apps/web/src/components/files/FilePreviewPanel.tsx` | — | ❌ absent | M | Electron WorkspaceImagePreview via assets.createUrl asset URLs (workspace-file tag). Vitre routes every file through projects.readFile into the text editor; binary reads presumably surface as open-failed status. |
| Open file in preview browser / open in external editor (OpenInPicker, editor.openFavorite mod+o) | `apps/web/src/components/files/FilePreviewPanel.tsx` | — | ❌ absent | L | Both breadcrumb-bar affordances missing: openFileInPreview (preview subsystem absent in Vitre) and OpenInPicker external-editor launch (editors.* RPCs). Gated in Electron on primary environment. |
| Breadcrumbs bar for the open file | `apps/web/src/components/files/filePath.ts` | `crates/vitre-app/src/files.rs` | 🟡 partial | S | Vitre renders relative_path.replace('/', ' › ') as one truncated label in the header. Electron renders per-crumb segments (root name first, file bolded) with auto-scroll-to-current-crumb and per-crumb tooltips. |
| Unsaved-changes dialog when leaving a dirty buffer (manual-save mode) | `apps/web/src/components/files/UnsavedChangesDialog.tsx` | — | ❌ absent | S | Only meaningful once manual autosave mode exists in Vitre; today autosave is always on so nothing strands. Bundle with the autosave-settings port. |

### editor-lsp

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| LSP document lifecycle: didOpen(v0)/didChange(200ms debounce, monotonic version, fingerprint-deduped)/didClose | `apps/web/src/components/files/codemirror/useLspBridge.ts` | `crates/vitre-app/src/lsp/bridge.rs` | ✅ present | M | LspBridge ports the semantics plus a TextFingerprint improvement (flush compares content, not an edit flag — the completion-provider-runs-before-Change-event gotcha is documented in DocState). Position conventions: wire UTF-16 vs editor char-index, converted via positions.rs; cross-file targets stay WIRE until target loads. |
| Completions (word/trigger-char anchor gating, prefix/fuzzy ranking, member access) | `apps/web/src/components/files/codemirror/lspBridge.ts` | `crates/vitre-app/src/lsp/bridge.rs` | ✅ present | S | completion_anchor rule shared via vitre-state/src/lsp_gating.rs; Vitre reimplements CodeMirror's client-side FuzzyMatcher as query_rank (fork menu renders provider list verbatim). Anchor..cursor text_edit synthesized when the server names no range. isIncomplete ignored in both. |
| Lazy completionItem/resolve: detail/docs panel + auto-import additionalTextEdits | `apps/web/src/components/files/codemirror/lspBridge.ts` | `crates/vitre-app/src/lsp/bridge.rs` | ✅ present | S | resolve_completion fills detail/documentation and converts additionalTextEdits against the current buffer (never-overlaps assumption shared with Electron). RPC: lsp.resolveCompletion with resolveData string. |
| Signature help / parameter hints (query on '(' and ',', clear on ')', Escape; Mod-Shift-Space manual) | `apps/web/src/components/files/codemirror/lspBridge.ts` | — | ❌ absent | L | lsp.signatureHelp RPC + LspSignatureHelpResult exist in vitre-contracts (generated.rs) but nothing calls them; the gpui-component fork has no signature-help popover infrastructure, so this needs both fork work and bridge wiring. |
| Hover tooltips (mouse hover with markdown + highlighted code) | `apps/web/src/components/files/codemirror/lspBridge.ts` | `crates/vitre-app/src/lsp/bridge.rs` | ✅ present | S | HoverProvider impl; deliberately does not flush doc sync on mouse hover, matching Electron's lspHoverExtension. Markdown rendered by fork hover popover (tooltipMarkdown.ts equivalent lives in the fork). Keyboard hover-at-cursor (vim gh) absent with vim. |
| Go to definition (Cmd/Ctrl-click; cross-file jump with position conversion and focus handoff) | `apps/web/src/components/files/codemirror/lspBridge.ts` | `crates/vitre-app/src/lsp/bridge.rs` | 🟡 partial | S | Cmd-click works via fork handle_click_hover_definition; show_document handler in files.rs opens cross-file targets with pending_reveal WIRE-position conversion; out-of-workspace targets dropped (parity). Missing: F12 binding (Electron binds F12; fork's GoToDefinition action has no default chord and Vitre binds none) and vim gd/C-]. |
| Diagnostics squiggles from streaming subscription (per-file latest-wins replacement, severity mapping 1/2/4) | `apps/web/src/components/files/codemirror/useLspBridge.ts` | `crates/vitre-app/src/files.rs` | ✅ present | S | Durable lsp.subscribeDiagnostics loop; editor_diagnostics sorts by start (fork DiagnosticSet appends in order). Restored manually after set_value clears them on reload (gotcha in check_open_file_disk). |
| Manual completion trigger (editor.showCompletions, default mod+i when editorFocus) | `apps/web/src/components/files/FilePreviewPanel.tsx` | — | ❌ absent | S | Electron resolves the command against keybindings and calls startCompletion on the live view. Vitre relies on the fork's automatic is_completion_trigger only; note completion_anchor is called with explicit=false always, so a bare cursor with no word/trigger char can never open the menu manually. |
| Supported-language gating from server registry (lsp.serverStatus supportedExtensions with static built-in fallback) | `apps/web/src/components/files/codemirror/useLspBridge.ts` | `crates/vitre-app/src/lsp/bridge.rs` | ✅ present | S | refresh_server_status called on panel creation and per file open; vitre-state/src/lsp_gating.rs holds the static list. Truncated reads never attach LSP (parity with editor never mounting). |

### env-mode

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Branch toolbar under composer: local vs new-worktree env mode, base branch, PR checkout, environment picker | `apps/web/src/components/BranchToolbar.tsx` | — | ❌ absent | XL | resolveSendEnvMode gates first sends ('Select a base branch before sending in New worktree mode'), buildTemporaryWorktreeBranchName, PullRequestThreadDialog checkout, multi-environment switcher. Vitre is local-single-environment and always sends worktree_path: None. |

### environments

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Multi-environment catalog (local + SSH + WSL + cloud) and per-environment scoping | `apps/web/src/state/environments.ts` | `crates/vitre-rpc/src/session.rs` | 🟡 partial | XL | Vitre is deliberately single-local-environment (one RpcSession to the spawned sidecar). Electron: environment catalog (connection/catalog.ts), presentations, relay discovery, BranchToolbarEnvironmentSelector, saved environments (apps/desktop/src/settings/DesktopSavedEnvironments.ts). This is the structural fork every 'connections' feature hangs off — schedule as its own milestone before SSH/cloud parity. |

### file-tree

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Workspace file tree (flat listEntries snapshot, dirs-first sort, flattened empty dirs, closed initial expansion, ignored entries dimmed) | `apps/web/src/components/files/FileBrowserPanel.tsx` | `crates/vitre-app/src/files.rs` | ✅ present | S | Vitre builds via vitre-state/src/file_tree.rs (FileTreeModel::build, flattenEmptyDirectories semantics ported, row.ignored dimmed at opacity 0.5). Manual refresh only in both (watcher never refreshes entries — deliberate parity). RPC: projects.listEntries; truncated flag shown as footer banner. |
| Git status decorations in tree (M/A/U/D/R colors, conflict '!', folder tint-without-letter, untracked-directory expansion) | `apps/web/src/components/files/fileTreeGitStatus.ts` | `crates/vitre-state/src/vcs_tree_status.rs` | ✅ present | S | files.rs row_decoration maps TreeVcsStatus to theme colors matching Electron (warning/info/danger/success). Refreshed on every workspace-watch event and after saves (vcs.getFileStatuses). Errors leave decorations as-is, same as Electron's atom. |
| New file / New folder (header buttons, context menu, inline name-entry row; created file opens) | `apps/web/src/components/files/FileBrowserPanel.tsx` | `crates/vitre-app/src/files.rs` | ✅ present | S | Vitre uses commit-on-Enter/cancel-on-blur inline InputState (TreeEdit) instead of Electron's placeholder+startRenaming. Context-menu creates target the row's dir/parent like Electron. RPC: projects.mutateEntry Create. Divergence: Vitre refreshes on success (no optimistic tree mutation). |
| Rename entry (inline; open buffer follows rename incl. ancestor dirs, LSP doc rebinds) | `apps/web/src/components/files/FileBrowserPanel.tsx` | `crates/vitre-app/src/files.rs` | ✅ present | S | follow_rename keeps buffer attached, re-highlights, didClose/didOpen under new name, migrates expansion keys. Electron remounts editor instead. No F2 chord in Vitre (Electron: f2 when fileTreeFocus). |
| Delete entry with confirmation (closes open file if within deleted path) | `apps/web/src/components/files/FileBrowserPanel.tsx` | `crates/vitre-app/src/files.rs` | ✅ present | S | Vitre uses native window.prompt (PromptLevel::Warning) vs Electron window.confirm. close_if_within mirrors Electron behavior. |
| Tree context menu chat integrations: Copy, Copy Mention, Add to Chat, Copy to Thread…, Paste | `apps/web/src/components/files/FileBrowserPanel.tsx` | `crates/vitre-app/src/files.rs` | ❌ absent | L | Vitre context menu has only New File/Folder/Rename/Delete. Missing: composerMentionFromTreePath serialization, fileClipboardStore, copyEntryAcrossThreads.ts (cross-thread copy via readFile/writeFile pairs), ThreadDestinationPicker.tsx. Cmd+C/Cmd+V tree keyboard copy/paste also absent. |
| Drag tree entry into chat composer as @mention | `apps/web/src/components/files/fileTreeDragMention.ts` | — | ❌ absent | M | Needs GPUI drag-and-drop from tree row to composer; Electron tags dragstart with mention payload and suppresses the drag-selection open. Depends on composer mention support in Vitre chat. |
| Tree keyboard navigation (arrows, Home/End, vim j/k/h/l + gg/G remap, roving focus) | `apps/web/src/components/files/FileBrowserPanel.tsx` | — | ❌ absent | M | Vitre tree rows are mouse-only (on_mouse_down); no focus handle, no focused-item concept (targetDirectory equivalent uses context-menu row instead). Electron re-dispatches remapped keys into the shadow-DOM tree. |
| Tree filename search filter (hide-non-matches mode; header Search button; fileTree.search command) | `apps/web/src/components/files/FileBrowserPanel.tsx` | — | ❌ absent | M | Electron uses @pierre/trees model.openSearch(); Vitre has no in-tree filter (QuickSearch partially covers). Search interacts with reveal logic (search owns expansion) — port carefully. |
| Reveal open file in tree (select row, auto-expand ancestor chain, scroll into view, keyed by reveal request) | `apps/web/src/components/files/FileBrowserPanel.tsx` | `crates/vitre-app/src/files.rs` | 🟡 partial | M | Vitre highlights the open row (bg accent) but never expands ancestors or scrolls to it when a file is opened from QuickSearch/palette/gd — a file in a collapsed dir opens without being visible in the tree. Electron's revealRequestId de-dup and scroll-position preservation also missing. |
| Tree header metadata (file count, 'Indexing…', partial-listing marker) and refresh button | `apps/web/src/components/files/FileBrowserPanel.tsx` | `crates/vitre-app/src/files.rs` | 🟡 partial | S | Vitre has refresh/new-file/new-folder buttons and a truncation footer, but no project-name header with file count / Indexing state (its header shows the open file path instead). |
| Multi-root file browser (per-root sections, root labels, absolute-path mentions for attached roots) | `apps/web/src/components/files/MultiRootFileBrowser.tsx` | — | ❌ absent | XL | Vitre is single-cwd (thread's project workspaceRoot). Electron composes threadRoots (composeThreadRoots) and keys everything by rootPath incl. openFile options and workspaceRoots.manage command. Large cross-cutting feature. |

### home

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Index draft landing (auto-open draft for most recent project, add-project hero fallback) | `apps/web/src/routes/_chat.index.tsx` | `crates/vitre-app/src/chat.rs` | 🟡 partial | M | Vitre boots into sidebar + empty chat with a new-thread action; it does not auto-create a draft for the most recent project nor show the add-project hero (sortScopedProjectsForSidebar pick + handleNewThread auto-start with retry state). |
| Draft threads (persisted composer drafts, draft routes, drafts in sidebar) | `apps/web/src/composerDraftStore.ts` | — | ❌ absent | L | 3591-line store: per-project drafts with attachments/mentions survive restart, routes/_chat.draft.$draftId.tsx, sidebar draft rows. Vitre composer text is per-open-thread in memory only. Memory gotcha: QuickSearch draft scope fix. |
| Add project flows: browse local folder, clone repository, WSL folder | `apps/web/src/components/CommandPalette.tsx` | `crates/vitre-app/src/palette/command_palette.rs` | ✅ present | L | Done 2026-09-03 (24f497a63): sidebar FolderPlus opens the palette in add-project mode; Sources view (local / Git URL / providers with readiness badges via server.discoverSourceControl), filesystem.browse directory browsing with ⌘Enter/Enter semantics, provider lookup + sourceControl.cloneRepository, project.create with ProjectCreated → new thread. WSL folder source waived until a Windows build exists. Path helpers ported to vitre-state/src/browse_path.rs. |

### images

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Expanded image lightbox with arrow-key prev/next | `apps/web/src/components/chat/ExpandedImageDialog.tsx` | — | ❌ absent | M | Full-screen dialog, Escape/ArrowLeft/ArrowRight, index counter, click-outside zoom-out. Vitre never displays image content (composer or timeline). |

### knowledge-graph

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Graph panel state machine (runtime install → build → ready/failed) | `apps/web/src/components/graph/GraphPanel.tsx` | — | ❌ absent | M | Polls graph.status; states runtime-missing/not-built/building/ready/failed each with one action; graph.runtimeStatus/installRuntime/build RPCs, GraphBuildMode, install-event stream. |
| Graph canvas (force-layout rendering, zoom/pan, node selection) | `apps/web/src/components/graph/GraphCanvas.tsx` | — | ❌ absent | XL | sigma + graphology behind React.lazy; graph.snapshot/subgraph (GRAPH_SUBGRAPH_MAX_NODES cap), colors in graphColors.ts. Native port needs GPU point/edge rendering + layout in Rust — no gpui-component equivalent. |
| Graph sidebar: node search/details/explain/path queries | `apps/web/src/components/graph/GraphSidebar.tsx` | — | ❌ absent | L | graph.query/explain/path RPCs; node detail cards and neighbor navigation. |

### markdown

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Rich assistant markdown: highlighted code blocks w/ copy, tables w/ copy-as-MD/CSV + expand, streaming smoothing | `apps/web/src/components/ChatMarkdown.tsx` | `crates/vitre-app/src/chat.rs` | 🟡 partial | XL | Vitre uses gpui-component TextView::markdown (selectable, keyed per message). Electron adds: shiki-style syntax highlighting with LRU cache, per-block copy + title bar, MarkdownTable copy/expand, useSmoothedStreamingText for delta smoothing, '(empty response)' placeholder (Vitre has this one). |
| File-path links in markdown open the file in the editor panel (or external editor) | `apps/web/src/components/ChatMarkdown.tsx` | — | ❌ absent | M | Markdown code spans/links resolved against markdownCwd/threadRoots; honors openFilesInExternalEditor setting; regular URLs open in the preview browser (openUrlInPreview). Vitre markdown links do nothing special. |

### message-cards

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| User message bubble: attachments grid, file chips, context chips, collapse of long text | `apps/web/src/components/chat/MessagesTimeline.tsx` | `crates/vitre-app/src/chat.rs` | 🟡 partial | L | Vitre renders a right-aligned accent bubble with raw text only. Electron (UserTimelineRow) also renders: image grid with lightbox, file-attachment download chips, preview-annotation cards, element-context chips, terminal-context inline chips (parsed out of message text), CollapsibleUserMessageBody (>8 lines/600 chars folds with fade), hover timestamp + copy + revert row. |
| Assistant message meta row: hover copy button + timestamp with tooltip | `apps/web/src/components/chat/MessageCopyButton.tsx` | — | ❌ absent | S | showAssistantMeta withheld until turn settles; MessageCopyButton copies raw markdown; formatShortTimestamp/formatChatTimestampTooltip respect a timestampFormat setting. Vitre has no timestamps or copy affordances on any message. |

### model-picker

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Model picker: searchable combobox with provider-instance sidebar, favorites, highlights, shortcuts | `apps/web/src/components/chat/ModelPickerContent.tsx` | `crates/vitre-app/src/chat.rs` | 🟡 partial | XL | Vitre: ghost button labeled with current model, flat dropdown grouped by enabled+installed provider with checkmark, picks dispatch ThreadMetaUpdate immediately (Electron persists lazily on next turn). Missing: search with scoring, ModelPickerSidebar per-instance rail + favorites tab, 'new model' badges, keyboard jump shortcuts (modelPickerJumpCommandForIndex), provider instance icons/accents, disabled-model reasons, locked-provider mode for started threads (getStartedThreadModelChangeBlockReason), openModelPicker via /model and keybinding. |
| Provider traits picker (reasoning effort / ultrathink / provider option descriptors) | `apps/web/src/components/chat/TraitsPicker.tsx` | — | ❌ absent | L | Renders select/boolean ProviderOptionDescriptors per model capability, persists via composer draft store or ThreadMetaUpdate model options; applyClaudePromptEffortPrefix injects 'Ultrathink:' prefix at send. Vitre never sends model options. |

### navigation

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Rebindable keybindings from server config (keybindings.json, when-expression contexts, superseded-default migration, settings UI) | `packages/shared/src/keybindings.ts` | `crates/vitre-app/src/main.rs` | ❌ absent | XL | Vitre hard-codes 11 chords in main.rs (mod+s, shift-alt-f, mod+p, mod+shift+f, mod+shift+p, mod+shift+o, mod+j, mod+alt+b, mod+w, mod+shift+[ ]) matching Electron defaults, with a code comment deferring rebindability to 'the keymap work'. Electron: server-resolved ResolvedKeybindingsConfig, when contexts (editorFocus/terminalFocus/fileTreeFocus/rightPanelFocus/modelPickerOpen), resolveShortcutCommand, migration table. Also unported chords: mod+n chat.new, ctrl+tab thread cycling, mod+shift+e fileTree.toggleFocus, f2 rename, mod+o openFavorite, mod+i showCompletions, mod+d diff.toggle. |
| Right panel tabbed surfaces (per-thread file/diff/search/plan/browser/terminal tabs; mod+w close, close-others/to-right/all, mod+shift+[ ] and ctrl+tab cycling, dirty dots, persisted per thread) | `apps/web/src/rightPanelStore.ts` | `crates/vitre-app/src/chat/right_panel.rs` | 🟡 partial | XL | Done 2026-09-03 (slice 2.1): full store port (`vitre-state/src/right_panel.rs`, version-10 shape + §2.1 migration, 11 unit tests incl. round-trip), tab strip with middle-click close + context menu (Copy path/Close/Close others/Close to the right/Close all), add-menu, empty-state cards, mod+w close / mod+shift+[ ] cycling, per-thread persistence in `right-panel-state.json`. Still missing: ctrl+tab thread cycling, dirty-dot-per-tab, and content hosts for diff/terminal/search/browser/plan/graph surfaces (menu entries disabled with reasons until their slices land; file/files surfaces share one FilesPanel, so a file tab shows the tree too). Vitre extension: a `:home` pseudo-thread key so QuickSearch file-opens work from the home view. |
| Workspace search-and-replace panel (regex/case/whole-word/include-exclude globs, per-file and replace-all with stale-revision guard; search.toggle) | `apps/web/src/components/SearchPanel.tsx` | — | ❌ absent | XL | Distinct from QuickSearch content mode: a persistent right-panel surface, 1000-result cap, computeReplacements applies writes through projects.writeFile with baseRevision and toasts on stale. Vitre has no equivalent surface or command. |
| Files panel toggle (⌘J / rightPanel.toggle) with panel recreated per project | `apps/web/src/components/CommandPalette.tsx` | `crates/vitre-app/src/chat/right_panel.rs` | ✅ present | S | Done 2026-09-03 (slice 2.1): RightPanelToggle bound to mod+j *and* the mod+alt+b alias; toggle hides/shows the whole dock (surfaces retained, Electron semantics); open/closed state + surfaces persist across launches in `right-panel-state.json`; ensure_files_panel still recreates the FilesPanel when the open thread's cwd changes; QuickSearch/palette file opens force the panel open. |
| File-tree keyboard commands: fileTree.toggleFocus (mod+shift+e), fileTree.newFile/newDirectory/rename/search chords | `apps/web/src/components/files/fileTreeActionBus.ts` | — | ❌ absent | M | Electron routes these over the fileTreeActionBus into the mounted tree (focus, openSearch, startCreate at focused directory, startRename). Vitre exposes New file/folder only through palette rows and header buttons, always at workspace root; no focus/rename/search commands. Depends on tree focusability. |

### notifications

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Toast system: stacked/collapsible, thread-scoped routing, actions, copy button | `apps/web/src/components/ui/toast.tsx` | `crates/vitre-app/src/chat.rs` | 🟡 partial | M | Vitre uses gpui-component window.push_notification (Root notification layer must be rendered manually — chat.rs:2810 comment). Electron: 813-line Base-UI toast manager with buildVisibleToastLayout stacking/collapse, shouldRenderThreadScopedToast (toasts follow their thread), additional/secondary actions, copy-to-clipboard, loading toasts (stackedThreadToast helper). |
| Slow-RPC toast watchdog | `apps/web/src/components/SlowRpcRequestToastCoordinator.tsx` | — | ❌ absent | S | rpc/requestLatencyState.ts tracks in-flight request latency and toasts when the sidecar is slow/stuck. |
| Keybindings-update toast (upstream default keymap changed) | `apps/web/src/components/KeybindingsUpdateToast.logic.ts` | — | ❌ absent | S | Notifies when shipped defaults differ from user's stored keymap; pairs with keybindings persistence memory gotcha. |
| Provider update notifications (launch notification + sidebar pill) | `apps/web/src/components/ProviderUpdateLaunchNotification.tsx` | — | ❌ absent | M | Detects updatable CLI providers at launch, per-environment rows (ProviderUpdateEnvironmentRows), dismissal persistence (providerUpdateDismissal.ts), SidebarProviderUpdatePill.tsx. |
| Claude managed-binary install dialog | `apps/web/src/components/claude/ClaudeBinaryInstallDialog.tsx` | — | ❌ absent | M | claude.getBinaryStatus/installBinary RPCs + claudeBinaryInstallAtoms.ts progress; blocks Claude-provider threads until installed. |

### plan

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Plan sidebar: active plan progress + proposed-plan review (approve, copy, export) | `apps/web/src/components/PlanSidebar.tsx` | `crates/vitre-app/src/chat.rs` | 🟡 partial | M | Vitre render_plan_sidebar shows step glyphs (pending/in-progress/completed) only. Electron adds LatestProposedPlanState review with markdown body (ChatMarkdown), approve action, copy, download-as-markdown export (proposedPlan.ts), collapse; also a dockable 'plan' right-panel surface. |

### plan-mode

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Build/Plan interaction-mode toggle in composer footer | `apps/web/src/components/chat/ChatComposer.tsx` | — | ❌ absent | M | ComposerFooterModeControls: PencilRuler 'Plan' pill toggles ProviderInteractionMode plan/default (also via /plan and /default). Vitre echoes the thread's existing interaction_mode on turn start but exposes no UI to change it. |
| Runtime-mode selector (e.g. full access / restricted) in composer footer | `apps/web/src/components/chat/ChatComposer.tsx` | — | ❌ absent | M | runtimeModeConfig-driven Select with icon+description per RuntimeMode; sent on each ThreadTurnStart. Vitre hardcodes RuntimeMode::FullAccess at thread create and reuses view.runtime_mode on sends. |
| Proposed-plan card in timeline (Plan badge, collapse >900 chars, copy / download / save-to-workspace) | `apps/web/src/components/chat/ProposedPlanCard.tsx` | — | ❌ absent | L | deriveTimelineEntries emits proposed-plan rows; card offers Copy, Download as markdown, Save to workspace (projects.writeFile RPC with path dialog), expand/collapse with fade. Vitre's timeline has no proposed-plan row kind. |
| Plan follow-up composer: Implement / Refine split button + 'Implement in a new thread' | `apps/web/src/components/chat/ComposerPlanFollowUpBanner.tsx` | — | ❌ absent | L | When latest turn produced an actionable plan (hasActionableProposedPlan), banner appears atop composer; empty prompt → 'Implement' (sends buildPlanImplementationPrompt with sourceProposedPlan ref), text → 'Refine'; menu item creates a new thread (optionally worktree) titled via buildPlanImplementationThreadTitle. |

### plan-sidebar

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Tasks/plan sidebar showing active TodoWrite steps with status glyphs | `apps/web/src/components/PlanSidebar.tsx` | `crates/vitre-app/src/chat.rs` | 🟡 partial | M | Vitre render_plan_sidebar: TASKS badge + relative time header, explanation text, steps with pending/in-progress/completed glyphs and strikethrough — auto-shows whenever derive_active_plan_state finds one. Missing: composer footer toggle to show/hide (togglePlanSidebar), proposed-plan display mode (sidebarProposedPlan), width animation. |

### preview

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Thread preview mini player overlay above the composer | `apps/web/src/components/preview/ThreadPreviewMiniPlayer.tsx` | — | ❌ absent | XL | Rendered by ChatView when a preview tab is miniaturized; whole preview subsystem (right panel tabs, PanelLayoutControls, RightPanelMaximizeControl) is absent in Vitre — its right panel is the files panel toggle only. |
| Embedded browser webview host | `apps/desktop/src/preview/Manager.ts` | — | ❌ absent | XL | 3776-line main-process manager drives WebContentsView instances bounds-synced to renderer slots (browser/ElectronBrowserHost.tsx, BrowserSurfaceSlot.tsx); HostedBrowserWebview.tsx is the hosted fallback. HARD problem in GPUI: no webview element exists; needs native WKWebView child-view embedding + bounds sync, or a wry side-window. Gate the whole preview milestone on this spike. |
| Preview session lifecycle + browser chrome (URL bar, back/forward, reload, open-in-system-browser) | `apps/web/src/components/preview/PreviewChromeRow.tsx` | — | ❌ absent | L | openPreviewSession/closePreviewSession + preview.open/navigate/resize/refresh/close/list + subscribePreviewEvents; per-tab state in previewStateStore.ts; loading progress bar (useLoadingProgress), unreachable/error screens (PreviewUnreachable.tsx, errorCodeMessages.ts). |
| Discovered local dev servers (port discovery cards) | `apps/web/src/components/preview/useDiscoveredLocalServers.ts` | — | ❌ absent | M | subscribeDiscoveredLocalServers stream + PreviewLocalServerCard.tsx + PreviewEmptyState.tsx; openDiscoveredPort.ts one-click open; portDiscoveryState.ts. DiscoveredLocalServerTerminal type already in vitre-contracts generated.rs. |
| Device toolbar: viewport presets, manual resize handles, zoom, appearance override | `apps/web/src/browser/BrowserDeviceToolbar.tsx` | — | ❌ absent | L | Device presets + custom viewport (BrowserViewportResizeHandles.tsx, browserViewportLayout.ts), ZoomIndicator.tsx, PreviewMoreMenu.tsx (system/light/dark appearance, zoom in/out/reset, open in system browser). |
| Agent browser automation with visible ghost cursor + consent hosts | `apps/web/src/components/preview/PreviewAutomationHosts.tsx` | — | ❌ absent | L | previewAutomation.connect/respond/focusHost RPCs; MCP preview_* tools drive it; AgentBrowserCursor overlay animates agent pointer (agentBrowserCursorLogic.ts); readiness gating (previewAutomationOpenReadiness.ts); PlaywrightInjectedRuntime in desktop main. |
| Element pick/annotate mode (click element in page → composer mention) | `apps/desktop/src/preview/PickPreload.ts` | — | ❌ absent | L | preview-pick-preload injects picker UI (PickLabelPosition, PickedElementPayload); lib/previewAnnotation.ts turns picked element into composer annotation. Depends entirely on webview host. |
| Preview recording (frame capture → artifact) and screenshots | `apps/web/src/browser/browserRecording.ts` | — | ❌ absent | L | DesktopPreviewRecordingFrame/Artifact contracts; recording scope + conflict errors; consumed by preview MCP recording_start/stop; memory notes main-thread capture deadlock gotcha from Tauri port attempt. |
| Thread preview mini player (docked PiP while chatting) | `apps/web/src/components/preview/ThreadPreviewMiniPlayer.tsx` | — | ❌ absent | M | previewMiniPlayerStore.ts + previewMiniPlayerLayout.ts; desktop has preview-pip-preload.ts for the PiP surface. |
| Webview crash recovery + guest budget limits | `apps/web/src/browser/webviewCrashRecovery.ts` | — | ❌ absent | S | Auto-reload crashed guests; previewGuestBudget.ts caps live webviews. Only meaningful once a webview host exists. |

### preview-context

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Preview/element context chips and annotation cards in composer and sent messages | `apps/web/src/components/chat/ComposerPreviewAnnotationCards.tsx` | — | ❌ absent | XL | Element contexts (ComposerPendingElementContexts), preview annotations with cropped screenshot + style-change count, review comments (ComposerPendingReviewComments via appendReviewCommentsToPrompt) — all removable chips pre-send and parsed cards in UserTimelineRow. Depends on preview/diff-review subsystems Vitre lacks. |

### quick-search

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| QuickSearch (mod+p / mod+shift+f) and command palette (mod+shift+p) reachable from chat | `apps/web/src/components/QuickSearch.tsx` | `crates/vitre-app/src/palette/quick_search.rs` | ✅ present | M | Vitre: mode toggle/switch-in-place semantics, thread+file+content corpora, OpenThread/OpenFile(line) events, file opens reveal the files panel at position; palette actions include new thread / open project (most-recent-thread-or-new) / new file / new folder. Committed in M2 slices. |
| QuickSearch Open mode (⌘P): ranked threads + workspace filename results | `apps/web/src/components/QuickSearch.tsx` | `crates/vitre-app/src/palette/quick_search.rs` | ✅ present | M | rank_threads ports rankThreads (active, title-contains, newest-first, cap 8); files via projects.searchEntries with the over-fetch-80-show-15 directory-drop rule Electron uses. Missing vs Electron: multi-root search (useMultiRootComposerPathSearch), per-row root labels, and ignored-file dimming (Vitre drops the ignored flag). |
| QuickSearch Content mode (⌘⇧F): chat-message search + ripgrep file-content matches (min 2 chars, 100 fetched / 30 shown) | `apps/web/src/components/QuickSearch.tsx` | `crates/vitre-app/src/palette/quick_search.rs` | ✅ present | S | RPCs: orchestration.searchMessages (limit 15) + projects.searchContent (maxResults 100). Match highlighting converts wire UTF-16 matchStart/End to byte offsets (rank.rs). Errors surface as a header row, not an empty state (parity with the fixed ripgrep-spawn bug). |
| QuickSearch preview pane: read-only file preview with revealed/centred line, thread card, message card, dialog width transitions | `apps/web/src/components/QuickSearch.tsx` | `crates/vitre-app/src/palette/preview.rs` | ✅ present | S | Ports previewKind sizing (xl/2xl/3xl popup, w-72 / 55% pane), editor reuse across rows in the same file, generation-guarded reads, centre_on_line deferred until layout. 'Add a project to search.' / 'Open a chat to preview' fallbacks present. |
| QuickSearch activation semantics: same-shortcut closes, other-shortcut switches mode keeping query; result opens file with editor focus handoff | `apps/web/src/components/QuickSearch.tsx` | `crates/vitre-app/src/chat.rs` | ✅ present | S | toggle_quick_search + deferred-draw action shadowing replicate the toggle. OpenFile event opens the files panel and calls FilesPanel::reveal which focuses the editor (Electron's requestEditorFocus one-shot intent). Mode chips in the input suffix present; shortcut labels on the chips (Electron shows '⌘P'/'⇧⌘F' text) absent. |
| Quick search (threads + files + content) with preview pane | `apps/web/src/components/QuickSearch.tsx` | `crates/vitre-app/src/palette/quick_search.rs` | ✅ present | S | Ported in M2 slice 4 incl. preview pane (palette/preview.rs) and ranking (palette/rank.rs); verify draft-scope behavior once drafts exist. |

### right-panel

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Right-panel dock: multi-surface tab strip (plan/diff/files/file/browser/terminal/search/graph) | `apps/web/src/rightPanelStore.ts` | `crates/vitre-app/src/chat/right_panel.rs` | 🟡 partial | L | Done 2026-09-03 (slice 2.1): ordered surfaces per thread with version-10 persistence + migration, activate/close/close-others/close-to-right/close-all with Electron's exact active-tab fallback rules, tab strip + add-menu + empty-state cards ported from RightPanelTabs.tsx. Only files/file surfaces host real content so far; browser/terminal reconcile ops exist in the store but have no live sources yet; graph hidden (setting not surfaced). |
| Right panel resize handle, width persistence, sheet mode on narrow windows | `apps/web/src/components/preview/RightPanelResizeHandle.tsx` | `crates/vitre-app/src/chat/right_panel.rs` | 🟡 partial | S | Done 2026-09-03 (slice 2.1): the dock is a third resizable panel in the workspace group (default 540, min 360, max 70% viewport); width persists on drag end (fork `ResizablePanelEvent::Resized`) and restores across launches. Missing: sheet mode on narrow windows (RightPanelSheet.tsx) and the maximize control. |

### search

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Content search panel with replace-in-files | `apps/web/src/components/SearchPanel.tsx` | `crates/vitre-app/src/palette/quick_search.rs` | 🟡 partial | M | Vitre's cmd-shift-F palette does content search (projects.searchContent) with preview pane but is modal and read-only. Electron panel is a persistent right-panel surface with regex/caseSensitive/wholeWord toggles and replace (client-side regex mirroring ripgrep semantics, $1 patterns, writes via projects.writeFile) — replace logic in SearchPanel.logic.ts is pure and portable. |

### session-status

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Thread settled/snoozed awareness (banner + auto-wake tick) | `apps/web/src/components/ChatView.tsx` | — | ❌ absent | M | effectiveSettled/effectiveSnoozed drive an AlarmClock/CheckCircle banner in the composer stack and a snooze wake timer. Vitre has no snooze concept in the chat column. |

### settings

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Settings shell: routed sections, sidebar nav, settings search | `apps/web/src/components/settings/SettingsSidebarNav.tsx` | — | ❌ absent | L | Sections: General, Keybindings, Providers, Language Servers, Knowledge Graph, Source Control, Connections, Beta, Archive (routes/settings.*.tsx); SettingsSearch.tsx + settingsSearchIndex.ts fuzzy-jumps to rows. Vitre has zero settings UI; only sidebar prefs persist (crates/vitre-app/src/sidebar_prefs.rs). |
| General settings panel (theme, timestamps, auto-save, glass opacity, auto-compact, default model/traits, update channel) | `apps/web/src/components/settings/SettingsPanels.tsx` | — | ❌ absent | L | 2201 lines. Mix of ClientSettings (hooks/useSettings.ts, local) and ServerSettings (server.getSettings/updateSettings patch RPC). Includes desktop update controls and archive-all. Vitre needs a settings store split mirroring client vs server ownership. |
| Providers settings: instance cards, add-provider wizard, models, accent color | `apps/web/src/components/settings/ProviderInstanceCard.tsx` | — | ❌ absent | XL | AddProviderInstanceDialog wizard (per-driver auth: API key/OAuth/CLI), ProviderModelsSection custom models, ProviderAccentColorPicker, RedactedSensitiveText, refresh/status (providerStatus.ts); RPCs server.updateProvider/refreshProviders + ServerProviderUpdatedPayload events. |
| Keybindings editor UI (record chord, when-expressions, conflicts, reset) | `apps/web/src/components/settings/KeybindingsSettings.tsx` | — | ❌ absent | L | 1337 lines + logic.ts (pure, portable): key capture recorder, keybindingConflictLabels, custom-vs-default source, reset/remove rows; persists via server.upsertKeybinding/removeKeybinding. Memory gotcha: changed defaults never reach existing homes. |
| Runtime user keymap honoring (rebindable commands from ServerConfig) | `apps/web/src/keybindings.ts` | `crates/vitre-app/src/main.rs` | 🟡 partial | M | Vitre hardcodes Electron's DEFAULT_KEYBINDINGS chords (cmd-p/shift-f/shift-p/shift-o/j, cmd-s, shift-alt-f) and its palette resolves chips from the live gpui keymap — but never reads user keybindings from server.getConfig. Port = translate keybindings config into cx.bind_keys at startup + on subscribeServerConfig change. |
| Language servers settings | `apps/web/src/components/settings/LanguageServersSettings.tsx` | — | ❌ absent | M | Per-language server enable/status/config over lsp.serverStatus; 564 lines. |
| Source control settings (discovery, clone, publish) | `apps/web/src/components/settings/SourceControlSettings.tsx` | — | ❌ absent | M | server.discoverSourceControl + sourceControl.lookupRepository/cloneRepository/publishRepository; sourceControlPresentation.ts. |
| Connections settings: network exposure, pairing links + QR, authorized clients, Tailscale, WSL backend | `apps/web/src/components/settings/ConnectionsSettings.tsx` | — | ❌ absent | XL | 3396 lines, heaviest settings surface. Depends on desktop IPC (serverExposure, sshEnvironment, wsl methods in apps/desktop/src/ipc/methods/) not on sidecar RPC alone; QR rendering (ui/qr-code.tsx), pairing URL builders (settings/pairingUrls.ts), version-drift warnings. Mostly out of scope while Vitre is local-single-environment — defer, but catalog for parity. |
| Diagnostics settings: process metrics, resource history, traces, signal process | `apps/web/src/components/settings/DiagnosticsSettings.tsx` | — | ❌ absent | L | server.getTraceDiagnostics/getProcessDiagnostics/getProcessResourceHistory/signalProcess; 1357 lines with charts. |
| Beta settings (feature flags) | `apps/web/src/components/settings/BetaSettingsPanel.tsx` | — | ❌ absent | S | Simple ClientSettings-backed toggles. |
| Archived threads page (list, restore, delete) | `apps/web/src/routes/settings.archived.tsx` | — | ❌ absent | M | lib/archivedThreadsState.ts; pairs with archive actions in thread context menus (Vitre sidebar has thread actions but no archive browser). |

### shortcuts

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| New thread shortcut (mod+shift+o) starting in the contextual project | `apps/web/src/components/ChatView.tsx` | `crates/vitre-app/src/chat.rs` | ✅ present | S | Vitre NewThread action mirrors startNewThreadFromContext: active thread's project else first project; error notifications for no-project/no-provider; default model from project default_model_selection else first enabled+installed provider's default model. |

### sidebar

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Sidebar: project grouping, sort menus, thread previews, status pills, archive, rename, manual reorder | `apps/web/src/components/SidebarV2.tsx` | `crates/vitre-app/src/chat/sidebar.rs` | ✅ present | L | Vitre ports grouping modes + per-project overrides (SidebarPrefs file), status pill colors transcribed from resolveThreadStatusPill, unread/visit sync (sync_thread_visit), archive via ThreadArchive, project rename/grouping dialogs (project_actions.rs), drag reorder, Show more/less previews, resizable split. Not re-audited row-by-row here; Electron-only extras like snooze controls and environment sections may remain. |
| Sidebar extras: update pills, sign-in card, connection status dot, thread snooze | `apps/web/src/components/Sidebar.tsx` | `crates/vitre-app/src/chat/sidebar.rs` | 🟡 partial | M | Vitre sidebar has Electron-parity project grouping/sorting/prefs (sidebar_prefs.rs) and a sidecar-status footer line (chat.rs describe_status), but lacks SidebarUpdatePill/SidebarProviderUpdatePill, T3ConnectSidebarSignIn, ConnectionStatusDot per-environment health, and Sidebar.snooze.ts thread snoozing. |

### skills

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| $ skill trigger: search provider skills, insert token, render SkillInlineText chips | `apps/web/src/components/chat/SkillInlineText.tsx` | — | ❌ absent | M | searchProviderSkills over selectedProviderStatus.skills, description/scope subtitles; sent messages render skill tokens as inline chips (skills passed into MessagesTimeline). |

### slash-commands

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| / slash-command menu: built-in /model /plan /default + provider slash commands | `apps/web/src/components/chat/composerSlashCommandSearch.ts` | — | ❌ absent | L | Grouped sections when query empty; provider commands from selectedProviderStatus.slashCommands; parseStandaloneComposerSlashCommand runs /plan etc. on submit without sending a message. Vitre's placeholder advertises '/' but nothing handles it. |

### source-control

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Branch toolbar: branch selector, create/switch, env-mode (in-place vs worktree) | `apps/web/src/components/BranchToolbar.tsx` | — | ❌ absent | L | BranchToolbarBranchSelector + BranchToolbarEnvModeSelector (worktree mode with previous-worktree restore) + BranchToolbarEnvironmentSelector; RPCs vcs.listRefs/createRef/switchRef/createWorktree/removeWorktree, vcs.init, vcs.pull, subscribeVcsStatus. |
| Git actions: commit / push / create PR with staged progress + AI commit message | `apps/web/src/components/GitActionsControl.tsx` | — | ❌ absent | L | 2083 lines; git.runStackedAction composite actions (commit, push, commit_push, commit_push_pr) with stage strings ('Generating commit message...', 'Pushing to X...'); dialogs per action; GitActionsControl.logic.ts is pure and portable. |
| Pull-request thread dialog (review a PR as a thread) | `apps/web/src/components/PullRequestThreadDialog.tsx` | — | ❌ absent | M | git.resolvePullRequest + git.preparePullRequestThread; pullRequestReference.ts parses PR URLs/references. |
| Workspace roots control (multi-root add/remove/manage) | `apps/web/src/components/WorkspaceRootsControl.tsx` | — | ❌ absent | M | Multi-root threads feature (state/threadRoots.ts); Vitre files panel is single-root today (files.rs notes ChatApp recreates on root change). |
| Worktree cleanup prompts on thread close/archive | `apps/web/src/worktreeCleanup.ts` | — | ❌ absent | S | Offers vcs.removeWorktree when leaving worktree-mode threads. |

### terminal

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Terminal render grid (VT emulation, colors, cursor) | `apps/web/src/components/ThreadTerminalDrawer.tsx` | — | ❌ absent | XL | Electron uses @xterm/xterm + FitAddon; server owns the PTY and streams buffers. Native port needs a VT100/ANSI grid renderer (e.g. alacritty_terminal model + custom GPUI element) fed by terminal.attach buffer replay and subscribeTerminalEvents deltas; writeTerminalBuffer replays full buffer with ESC-c reset then appends slices. |
| Terminal session lifecycle (open/attach/restart/clear/close, metadata) | `apps/web/src/state/terminalSessions.ts` | — | ❌ absent | M | RPCs: terminal.open/attach/write/resize/clear/restart/close + subscribeTerminalEvents/subscribeTerminalMetadata (packages/contracts/src/rpc.ts). Rust types already generated in crates/vitre-contracts/src/generated.rs (terminal:operate scope, terminalId payloads). combineTerminalSessionState merges metadata summary + buffer state. |
| Keyboard input with passthrough/clear/navigation shortcut arbitration | `apps/web/src/keybindings.ts` | — | ❌ absent | M | isTerminalPassthroughShortcut / isTerminalClearShortcut / terminalNavigationShortcutData decide which chords go to the PTY vs the app; ctrl-L clear sends . GPUI focus-context equivalents needed. |
| Drawer resize + refit on container changes | `apps/web/src/components/ThreadTerminalDrawer.tsx` | — | ❌ absent | S | Pointer-drag height (min 180px, max 75% window), clamped persistence, refit on sidebar/right-panel toggles and splits, terminal.resize RPC on cols/rows change. |
| Selection actions: copy + add selection to chat context | `apps/web/src/lib/terminalContext.ts` | — | ❌ absent | M | Multi-click selection with 260ms delayed action popover; TerminalContextSelection flows into composer as context chip. Requires native selection model in the grid renderer. |
| Terminal links: URLs and file paths, incl. wrapped-line ranges | `apps/web/src/terminal-links.ts` | — | ❌ absent | M | extractTerminalLinks + resolveWrappedTerminalLinkRange handle links spanning wrapped buffer lines; path links open in editor (openWorkspaceFilePrimaryAction), localhost URLs open in preview (openTerminalLinkInPreview.ts). |
| Terminal tabs, groups and h/v splits with per-thread persistence | `apps/web/src/terminalUiStateStore.ts` | — | ❌ absent | M | Zustand persist keyed by scoped thread: terminalOpen, height, terminalIds, groups (MAX_TERMINALS_PER_GROUP), activeGroup, splitDirection; storage key t3code:terminal-state:v1 with migration. Also right-panel terminal surfaces mirror groups (rightPanelStore.openTerminal/splitTerminal/closeTerminal). |
| Terminal theme sync from app theme | `apps/web/src/components/ThreadTerminalDrawer.tsx` | — | ❌ absent | S | Builds xterm ITheme from computed CSS vars with transparent-color normalization; Vitre equivalent is mapping vitre.json theme tokens to grid palette. |
| Project scripts control (run scripts in terminal, auto-open preview URL) | `apps/web/src/components/ProjectScriptsControl.tsx` | — | ❌ absent | L | Scripts from project config (apps/web/src/projectScripts.ts, useT3ProjectFileScripts.ts); primary script button + menu, add/edit/delete dialogs, optional previewUrl opened on run, per-script keybindings (lib/projectScriptKeybindings.ts). Runs via terminal.open with command. |
| Running-subprocess indicators on threads | `apps/web/src/components/ThreadStatusIndicators.tsx` | — | ❌ absent | S | selectRunningSubprocessTerminalIds from terminal metadata drives sidebar/thread badges for live terminals. |

### terminal-context

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Terminal contexts: 'add to chat' selections become inline chips and render in sent messages | `apps/web/src/components/chat/ComposerPendingTerminalContexts.tsx` | — | ❌ absent | XL | TerminalContextSelection flows from ThreadTerminalDrawer into ComposerPromptEditor inline chips (TerminalContextInlineChip); appendTerminalContextsToPrompt serializes into the message; userMessageTerminalContexts parses them back into chips with tooltips. Requires the terminal subsystem, absent in Vitre. |

### thread-references

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Thread-reference chips: [#Title](t3code://thread/id) render as chips and navigate | `apps/web/src/components/ChatMarkdown.tsx` | — | ❌ absent | M | MarkdownThreadReferenceLink (line ~1151) + packages/shared/src/threadReferences.ts parse/collect tokens. Composer inserts them via the # trigger. Vitre has neither rendering nor insertion. |

### timeline

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Virtualized streaming message timeline with stick-to-bottom | `apps/web/src/components/chat/MessagesTimeline.tsx` | `crates/vitre-app/src/chat.rs` | ✅ present | M | Vitre uses gpui list + FollowMode::Tail with splice/remeasure on content-hash change (rebuild_timeline). Electron uses LegendList with maintainScrollAtEnd + maintainVisibleContentPosition. Interleaves messages and activities by created_at like Electron. |
| Anchor newly-sent user message to viewport top (anchoredEndSpace scroll mode) | `apps/web/src/components/chat/timelineScrollAnchoring.ts` | — | ❌ absent | L | Electron pins the just-sent user message near the top (CHAT_LIST_ANCHOR_OFFSET, onAnchorReady/onAnchorSizeChanged, TimelineScrollMode refs in ChatView). Vitre only tail-follows; no per-send anchoring. |
| Timeline minimap rail (hover strip, per-turn preview, click-to-jump, in-view highlighting) | `apps/web/src/components/chat/MessagesTimeline.tsx` | — | ❌ absent | L | TimelineMinimap (line ~633) overlays left gutter; items derived per user turn with compacted assistant preview; gutter-width-aware hit strip. Nothing comparable in Vitre. |
| Turn folding: collapsed prior turns labeled 'Worked for Xm' / 'You stopped after X' | `apps/web/src/components/chat/MessagesTimeline.logic.ts` | — | ❌ absent | L | deriveTurnFolds hides settled turns' work entries behind a TurnFoldTimelineRow toggle; interrupted latest turn stays expanded until next turn. Vitre renders every activity row flat, forever. |
| Work-log grouping with '+N previous tool calls' overflow toggle | `apps/web/src/components/chat/MessagesTimeline.logic.ts` | — | ❌ absent | M | MAX_VISIBLE_WORK_LOG_ENTRIES=1: only the latest work entry per group is visible with a WorkGroupToggleTimelineRow expander (scroll-delta compensation via flushSync). Vitre shows all activities uncollapsed. |
| Working indicator with self-ticking elapsed timer | `apps/web/src/components/chat/MessagesTimeline.tsx` | `crates/vitre-app/src/chat.rs` | 🟡 partial | S | Vitre shows three dots + static 'Working…' row. Electron shows 'Working for 12s' via WorkingTimer (shared visibility-aware second ticker, direct textContent writes) fed by deriveActiveWorkStartedAt. |
| Empty-thread placeholder and no-thread state | `apps/web/src/components/chat/MessagesTimeline.tsx` | `crates/vitre-app/src/chat.rs` | 🟡 partial | S | Vitre shows 'Select a thread to start chatting' when nothing is open, but an opened empty thread renders a blank list — Electron shows 'Send a message to start the conversation.' plus NoActiveThreadState for missing threads. |

### tool-calls

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Tool-call rows: icon by kind, heading+preview split, expandable mono detail | `apps/web/src/components/chat/MessagesTimeline.tsx` | `crates/vitre-app/src/chat.rs` | 🟡 partial | M | Vitre has icon-by-kind heuristic, summary text, tone colors (error/approval), click-to-expand mono detail (well-known payload keys command/detail/preview/text/path, 4000-char cap) with manual remeasure. Missing: heading/preview split (SimpleWorkEntryRow), success/fail/empty status glyph with tooltip, destructive styling for runtime.error, MCP payload JSON block, changed-file list in expansion, workspace-relative path formatting. |

### turn-diff

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Changed-files card under assistant messages with diff stats and open-turn-diff | `apps/web/src/components/chat/ChangedFilesTree.tsx` | — | ❌ absent | XL | useTurnDiffSummaries builds per-turn summaries; ChangedFilesCard/ChangedFilesTree render collapsible file tree with DiffStatLabel (+adds/−dels), per-file click opens the DiffPanel scoped to that turn (onOpenTurnDiff), auto-expands on latest turn, expansion persisted per thread in uiStateStore. Vitre has no diff surface at all. |

### updater

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Desktop auto-update UI (update pill, download/install progress, release notes, channels) | `apps/web/src/components/sidebar/SidebarUpdatePill.tsx` | — | ❌ absent | L | desktopUpdate.logic.ts state machine (download vs install vs error, arm64-on-Intel warning), state/desktopUpdate.ts over IPC; main-process apps/desktop/src/updates/DesktopUpdates.ts (884 lines, electron-updater, channels, releaseNotes.ts). Vitre needs Sparkle (macOS) or a custom updater — whole subsystem, not a port. |

### user-input

| Feature | Electron ref | Vitre ref | Status | Size | Notes |
|---|---|---|---|---|---|
| Pending user-input questionnaire: header + n/N chip, option rows, multi-select, auto-advance | `apps/web/src/components/chat/ComposerPendingUserInputPanel.tsx` | `crates/vitre-app/src/chat.rs` | 🟡 partial | M | Vitre: one question at a time, single-select auto-advances immediately (Electron waits 200ms with optimistic highlight), multi-select with Continue/Submit, ThreadUserInputRespond answers (label array vs single label). Missing: custom free-text answer typed in the composer (Electron routes the prompt editor to customAnswer), Previous-question button, number-key 1-9 selection (Vitre draws the number chips but has no key handler). |

