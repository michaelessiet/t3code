# @t3tools/desktop-tauri

Parallel **Tauri 2** shell for T3 Code. The Electron app (`apps/desktop`) is
untouched and remains the shipping product; this project exists so the Tauri
migration can be evaluated (and abandoned) without risk. Groundwork:
`experiments/tauri-preview-probe` (PR #27) proved the preview subsystem's hard
capabilities on WKWebView.

## Architecture (milestone 1 — walking skeleton)

- **Backend**: the real Node server (`apps/server/dist/bin.mjs`), spawned with
  the bootstrap envelope piped to stdin — the server's existing
  `--bootstrap-fd 0` path that the Electron app already uses for WSL
  (`DesktopBackendConfiguration.ts`). Port scan from 3773, readiness poll on
  `/.well-known/t3/environment`, restart with backoff. State lives in
  `~/.t3-tauri` by default (isolated from the Electron app's `~/.t3`; override
  with `T3CODE_TAURI_HOME`).
- **UI serving**: the window loads `t3code-tauri://app/`, a custom protocol
  that proxies to the target origin and stamps the same CSP shape as
  Electron's `ElectronProtocol.ts` — the Vite dev server in dev, the backend's
  static client serving in prod. Stable renderer origin, independent of the
  backend port.
- **desktopBridge**: `shim/src/index.ts` (typed `satisfies DesktopBridge`
  against `@t3tools/contracts`) is bundled to a single IIFE and injected as an
  initialization script. Sync methods read a seed blob generated after backend
  readiness; async methods are Tauri commands (`src-tauri/src/bridge.rs`);
  events arrive as Tauri events. The local bearer-token exchange
  (`/oauth/token`) happens in the shim itself — the backend serves
  `access-control-allow-origin: *`.
- **Deliberately absent in M1**: `preview` (the preview UI self-hides),
  Clerk cloud auth (run the web dev server without `VITE_CLERK_PUBLISHABLE_KEY`),
  updater / SSH / WSL / server exposure (inert typed stubs), safeStorage
  encryption for the connection catalog (plaintext file — M3), Electron's
  traffic-light inset (default position for now).

## Running (dev, macOS)

```sh
# 1. Server bundle (rebuild after server changes):
pnpm --filter @t3tools/server build   # or the repo's build:bundle flow

# 2. Web dev server, IPv4-bound with a pinned HMR host, no Clerk env.
#    (Not `pnpm dev:web`: scripts/dev-runner.ts deletes HOST for non-desktop
#    modes, and HOST=localhost binds IPv6-only while the proxy dials IPv4.)
cd apps/web && HOST=127.0.0.1 ../../node_modules/.bin/vp dev   # http://127.0.0.1:5733

# 3. The Tauri shell:
cd apps/desktop-tauri && pnpm dev
```

`pnpm dev` builds the shim, checks the prerequisites, and runs `cargo run`
with `VITE_DEV_SERVER_URL`, `T3CODE_TAURI_SERVER_ENTRY`, and
`T3CODE_TAURI_SHIM_PATH` set. Useful overrides: `T3CODE_PORT` (fixed backend
port), `T3CODE_TAURI_NODE` (node binary), `T3CODE_TAURI_HOME` (state dir).

## Milestones

- **M1 (this)** — walking skeleton: threads/chat/terminal/dialogs/context
  menus work end-to-end; custom-protocol UI serving.
- **M2 — preview subsystem**: Rust child-webview manager (`Window::add_child`,
  `unstable` feature) implementing `DesktopPreviewBridge` with the probe's
  patterns (injected runtime, custom-protocol IPC, `takeSnapshotWithConfiguration`
  capture). Needs a small additive `apps/web` host component that syncs bounds
  instead of rendering `<webview>` tags.
- **M3 — auth + parity**: Clerk via the web provider under Tauri (additive
  `main.tsx` runtime detection), keychain-backed secrets, updater
  (tauri-plugin-updater), recording/PiP, traffic-light inset.
- **M4 — packaging + platforms**: Tauri bundler with a Node sidecar (or
  WS2-style download-on-first-run runtime) + server dist + pruned
  node_modules resources; Windows (WebView2 has real CDP) and Linux; WSL.
