# @t3tools/vitre

**Vitre** — the Tauri 2 desktop app, evolved from T3 Code into its own product. The Electron app (`apps/desktop`) is
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
  `~/.vitre` by default (isolated from the Electron app's `~/.t3`; override
  with `VITRE_HOME`).
- **UI serving**: the window loads `vitre://app/`, a custom protocol
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
  Verified headlessly with `VITRE_PREVIEW_SELFTEST=1`
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
  `vitre://app` origin must be registered in the Clerk dashboard's
  allowed origins before sign-in works in the shell.
- **Connection catalog at rest (M3)**: AES-256-GCM with the key in the
  macOS keychain (`keyring` crate), mirroring Electron safeStorage;
  `setConnectionCatalog` returns `false` when the keychain is unavailable
  and the M1 plaintext file is migrated on first read. Release-default:
  dev (debug) binaries keep the plaintext file because their ad-hoc code
  signature changes every rebuild, which would make the keychain prompt on
  every read (`VITRE_SECURE_CATALOG=1/0` overrides). Non-macOS
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
pnpm dev:vitre
```

`dev:vitre` is a dev-runner mode (`scripts/dev-runner.ts`) that
starts the web dev server and this package's `dev` script in parallel with
pinned HOST/ports. The package script builds the shim, waits for the
prerequisites, and runs the shell with `VITE_DEV_SERVER_URL`,
`VITRE_SERVER_ENTRY`, and `VITRE_SHIM_PATH` set.

To run the pieces separately: start the web dev server with
`cd apps/web && HOST=127.0.0.1 ../../node_modules/.bin/vp dev` (not
`pnpm dev:web` — the dev runner deletes HOST for non-desktop modes, and
`HOST=localhost` binds IPv6-only while the proxy dials IPv4), then
`cd apps/desktop-tauri && pnpm dev`.

Useful overrides: `T3CODE_PORT` (fixed backend port), `VITRE_NODE`
(node binary), `VITRE_HOME` (state dir).

## Milestones

- **M1 (done)** — walking skeleton: threads/chat/terminal/dialogs/context
  menus work end-to-end; custom-protocol UI serving.
- **M2 (this)** — preview subsystem: Rust child-webview manager implementing
  `DesktopPreviewBridge`; first additive `apps/web` change
  (bounds-sync mode in `HostedBrowserWebview` + a no-`<webview>` fallback in
  `PreviewAutomationHosts`).
- **M3 (done)** — auth + parity: Clerk via the web provider under Tauri
  (additive `main.tsx` runtime detection), keychain-backed secrets, element
  picker/annotations, PiP, traffic-light inset. Updater deferred to a signed
  packaging pass.
- **M4 (this)** — macOS packaging (design below); Windows (WebView2 has real
  CDP), Linux, and WSL remain future platform passes.

## Packaging design (M4, macOS)

Goal: `pnpm --dir apps/desktop-tauri build:app` produces a standalone
`Vitre.app` (+ DMG) — ad-hoc signed (`signingIdentity: "-"`) for local
verification; real signing/notarization/updater are a later pass.

**Bundle layout** (`bundle.resources` list-form preserves directory
structure under `Contents/Resources/`):

- `Contents/MacOS/vitre` — the Tauri binary.
- `Contents/MacOS/node` — real Node sidecar via `bundle.externalBin`
  (`binaries/node-<target-triple>`, downloaded from nodejs.org at stage
  time, version pinned to the root `engines.node`). `bin.mjs` uses
  `node:sqlite`, so a real Node ≥ 24 is required; there is no Electron
  `ELECTRON_RUN_AS_NODE` trick here.
- `Contents/Resources/staged/backend/` — a staged production install
  mirroring the Electron artifact's `app/` layout so Node resolution works
  above `bin.mjs`: `apps/server/dist/` (bin.mjs + `client/` sibling),
  `package.json` (resolved server deps + fff native packages),
  `pnpm-workspace.yaml`, `node_modules/` from `vp install --prod` with
  `node-linker=hoisted` (no pnpm symlink farm for the bundler to copy),
  then `stripStagedSourcemaps` + `pruneClaudeSdkPlatformPackages` +
  `pruneStagedNodeModules` reused from `scripts/build-desktop-artifact.ts`.
  No asar → the Electron d.ts-rescue hook is unnecessary.
- `Contents/Resources/staged/shim.js` + `staged/preview-runtime.js` — the
  built injection scripts.

**Rust changes**: asset/config resolution moves behind a fallback chain —
`VITRE_*` env override (dev) → `resource_dir()/staged/...` (packaged);
node resolution: `VITRE_NODE` → sidecar next to `current_exe()` → `node`
on PATH. Clerk key for the CSP: runtime env → `option_env!` compile-time
bake (the staging script exports the repo `.env` key to `cargo build`;
the web client bakes `VITE_CLERK_PUBLISHABLE_KEY` at its own build).
`resource_dir()` requires an app handle, so config reading moves from
`main()` into `.setup()`. The `devtools` feature stays enabled in M4
builds for verification and is dropped when shipping.

**Build orchestration** (`scripts/build-vitre-app.ts` at the repo root;
run `pnpm dist:vitre:app`, or `--skip-build` to reuse dist outputs):
build inputs (`vp run --filter t3 build` for server dist + bundled
client, plus `build-shim.mjs` / `build-preview-runtime.mjs`), stage
`src-tauri/staged/` (gitignored), copy the Node sidecar into
`src-tauri/binaries/`, then run `tauri build` via the `@tauri-apps/cli`
devDependency. Prod serving needs no dev servers: the `vitre://app`
protocol proxies to the backend, which serves the bundled client.

Packaging gotchas learned the hard way:

- `bundle.resources`/`externalBin` live in `tauri.bundle.conf.json`
  (passed with `--config`) because tauri-build validates them on every
  cargo compile — keeping them in `tauri.conf.json` would make plain dev
  `cargo build` fail until staging ran.
- `entitlements.plist` (allow-jit, allow-unsigned-executable-memory,
  disable-library-validation) is mandatory even for ad-hoc signing: the
  hardened runtime is on by default and V8 dies at isolate init without
  the JIT entitlements ("Failed to reserve virtual memory for
  CodeRange").
- The window icon embedded by `tauri::generate_context!` must be an
  8-bit PNG (16-bit fails at window creation with "invalid icon ...
  pixel count"), and tauri-build does NOT watch `icons/` — touch
  `tauri.conf.json` after changing icons or the stale icon stays
  embedded.
- Verify with the packaged selftest (no node on PATH proves the
  sidecar): `env -i HOME="$HOME" PATH=/usr/bin:/bin
VITRE_PREVIEW_SELFTEST=1 …/Vitre.app/Contents/MacOS/vitre`.

**Icon**: `icons/icon.png` + `icons/icon.icns` are generated from
`scripts/generate-icon.swift` (a glazed-glass "V" over a translucent
frosted backdrop) — `swift scripts/generate-icon.swift
src-tauri/icons/icon.png`, then rebuild the iconset with
`sips`/`iconutil` at 16–1024.
