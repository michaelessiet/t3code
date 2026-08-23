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
- **Preview subsystem (M2)**: `src-tauri/src/preview.rs` implements
  `DesktopPreviewBridge` with native child webviews (`Window::add_child`,
  tauri `unstable` feature) instead of renderer `<webview>` tags. The
  renderer's `HostedBrowserWebview` detects the optional
  `setTabBounds` bridge method and switches into **bounds-sync mode**: a
  placeholder div reports its on-screen rect and the shell positions the
  child webview over it (page zoom reproduces the panel's downscale).
  WKWebView has no CDP, so the Electron main-process automation is
  reproduced with the WS5 probe's patterns: an injected runtime
  (Playwright's InjectedScript — identical extraction to
  `PlaywrightInjectedRuntime.ts` — plus `shim/preview-runtime.src.js`,
  built by `scripts/build-preview-runtime.mjs`), eval-with-results over the
  `t3preview://` custom protocol, `takeSnapshotWithConfiguration:` capture
  via objc2, and NSAppearance for `prefers-color-scheme` emulation.
  Verified headlessly with `T3CODE_TAURI_PREVIEW_SELFTEST=1`
  (`src-tauri/src/selftest.rs`).
- **Element picker + annotations (M3)**: apps/desktop's `PickPreload.ts`
  annotation studio is bundled **unchanged** into the injected preview
  runtime — `scripts/build-preview-runtime.mjs` compiles it with `electron`
  aliased to `shim/picker/electron-adapter.ts`, so react-grab element
  capture, the closed-shadow-DOM overlay, and the generated annotation CSS
  stay byte-identical across shells. The shell arms/cancels sessions by
  evaling `__t3pPickerDispatch(channel, ...)` (GuestProtocol channel names
  preserved); the guest submits over `t3preview://` (`kind: "pick"`), and
  preview.rs crops the screenshot natively via `WKSnapshotConfiguration.rect`.
  Picks settle null on navigation, tab close, cancel, or Escape — same as
  Electron.
- **Clerk cloud auth (M3)**: runs web-side via the standard-browser
  `@clerk/react` provider — `apps/web/src/main.tsx` routes Tauri there
  (`isTauri` in `env.ts`; `@clerk/electron` needs the Electron preload
  bridge). The shell adds the Clerk frontend-API origin (decoded from
  `VITE_CLERK_PUBLISHABLE_KEY` / `T3CODE_CLERK_PUBLISHABLE_KEY` in the
  environment) to the CSP `script-src` in `protocol.rs`. NOTE: the
  `t3code-tauri://app` origin must be registered in the Clerk dashboard's
  allowed origins before sign-in works in the shell.
- **Connection catalog at rest (M3)**: AES-256-GCM with the key in the
  macOS keychain (`keyring` crate), mirroring Electron safeStorage;
  `setConnectionCatalog` returns `false` when the keychain is unavailable
  and the M1 plaintext file is migrated on first read. Release-default:
  dev (debug) binaries keep the plaintext file because their ad-hoc code
  signature changes every rebuild, which would make the keychain prompt on
  every read (`T3CODE_TAURI_SECURE_CATALOG=1/0` overrides). Non-macOS
  stays plaintext until the M4 platform pass.
- **Picture-in-picture (M3)**: per-tab always-on-top `WebviewWindow` showing
  the live preview — the recording frame loop in `preview.rs` is a shared
  consumer loop (recording broadcasts `t3code://preview-frame`; PiP evals
  JPEG frames into the window's `<img>` and refits the window aspect to the
  content), mirroring Electron's screencast consumer set.
- **Known gaps after M3**: copy-artifact-to-clipboard,
  `LoadFailed` nav status (WKWebView load failures surface as a stuck
  Loading state), network capture is fetch/XHR only (no CDP Network
  domain), `automation.evaluate` cannot report syntax errors
  (fire-and-forget eval times out instead), strict-CSP guest pages may
  block the runtime's `fetch` posts, and native child webviews always
  render **above** the app UI — dialogs/menus that overlap the preview
  panel are occluded until the preview hides.
- **Deferred to M4**: updater (tauri-plugin-updater needs signed packaged
  bundles + an update endpoint; the typed disabled-state stub is already
  contract-correct), SSH / WSL / server exposure (inert typed stubs),
  Windows/Linux keychain equivalents (DPAPI / secret-service).

## Running (dev, macOS)

```sh
# One-time / after server changes — build the server bundle:
pnpm build:bundle

# Everything else, from the repo root (mirrors pnpm dev:desktop):
pnpm dev:desktop-tauri
```

`dev:desktop-tauri` is a dev-runner mode (`scripts/dev-runner.ts`) that
starts the web dev server and this package's `dev` script in parallel with
pinned HOST/ports. The package script builds the shim, waits for the
prerequisites, and runs the shell with `VITE_DEV_SERVER_URL`,
`T3CODE_TAURI_SERVER_ENTRY`, and `T3CODE_TAURI_SHIM_PATH` set.

To run the pieces separately: start the web dev server with
`cd apps/web && HOST=127.0.0.1 ../../node_modules/.bin/vp dev` (not
`pnpm dev:web` — the dev runner deletes HOST for non-desktop modes, and
`HOST=localhost` binds IPv6-only while the proxy dials IPv4), then
`cd apps/desktop-tauri && pnpm dev`.

Useful overrides: `T3CODE_PORT` (fixed backend port), `T3CODE_TAURI_NODE`
(node binary), `T3CODE_TAURI_HOME` (state dir).

## Milestones

- **M1 (done)** — walking skeleton: threads/chat/terminal/dialogs/context
  menus work end-to-end; custom-protocol UI serving.
- **M2 (this)** — preview subsystem: Rust child-webview manager implementing
  `DesktopPreviewBridge`; first additive `apps/web` change
  (bounds-sync mode in `HostedBrowserWebview` + a no-`<webview>` fallback in
  `PreviewAutomationHosts`).
- **M3 — auth + parity**: Clerk via the web provider under Tauri (additive
  `main.tsx` runtime detection), keychain-backed secrets, updater
  (tauri-plugin-updater), element picker/annotations, PiP, traffic-light
  inset.
- **M4 — packaging + platforms**: Tauri bundler with a Node sidecar (or
  WS2-style download-on-first-run runtime) + server dist + pruned
  node_modules resources; Windows (WebView2 has real CDP) and Linux; WSL.
