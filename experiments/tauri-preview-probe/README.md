# tauri-preview-probe

Standalone viability probe (WS5 of the Aug 2026 size/memory plan): can T3 Code's
preview subsystem run on **Tauri 2 / WKWebView** without Chromium or CDP?

The preview subsystem depends on five Chromium capabilities today
(`apps/desktop/src/preview/Manager.ts`): CDP `Runtime.evaluate` with results
(InjectedScript automation), DOM snapshot + element targeting, navigation
control/events, ~12fps frame capture (recording/PiP), and CDP Network-domain
observation. This app exercises a WKWebView equivalent of each against a
built-in localhost server (animated canvas + periodic fetch traffic), across
**3 child webviews in one window** (the multiwebview shape the preview host
needs), and prints a PROBE REPORT.

## Running

```sh
cargo run --release          # opens a window, prints the report to stdout, exits
```

Memory sampling: WebContent processes are launchd XPC services, not children,
so sample externally while the app idles after the report — diff
`ps -axo pid=,rss=,comm= | grep com.apple.WebKit` before launch vs after.

## Results (2026-08-21, macOS, Apple Silicon, release build)

All five probes **PASS**:

| Capability                  | Verdict         | Notes                                                                                                                                                                                                                                                                      |
| --------------------------- | --------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Multi-webview hosting       | **GO** (caveat) | `Window::add_child` works; requires Tauri's `unstable` feature flag                                                                                                                                                                                                        |
| JS eval with results        | **GO** (caveat) | `Webview::eval` is fire-and-forget; results round-trip through a `probe://` custom protocol the injected runtime fetches (CORS headers required). Equivalent of `Runtime.evaluate` must be built as an IPC bridge — done here, reliable                                    |
| DOM snapshot + targeting    | **GO**          | Injected runtime serializes elements with layout rects; playwright-core's InjectedScript could be injected wholesale via `initialization_script`, same as the CDP path                                                                                                     |
| Navigation control + events | **GO**          | `Webview::navigate` + `on_page_load` Started/Finished events                                                                                                                                                                                                               |
| Frame capture               | **GO**          | `takeSnapshotWithConfiguration:` via objc2 dynamic messaging: **11.2fps sustained vs 12fps target**, ~3MB TIFF/frame at 435×435 (re-encode to JPEG needed for recording)                                                                                                   |
| Network observation         | **PARTIAL**     | No CDP Network equivalent. fetch/XHR monkeypatch works (33 events captured) but sees **no response bodies, no subresource/navigation requests, no WebSocket frames**. Fuller fidelity needs a local proxy (`WKWebsiteDataStore.proxyConfigurations`, macOS 14+) — unprobed |

### Memory — window + 3 animated-canvas guests, each polling fetch every 500ms

| Process               | Tauri/WKWebView        | Electron (measured Aug 2026 harness) |
| --------------------- | ---------------------- | ------------------------------------ |
| Per hidden/idle guest | **~65MB** (WebContent) | **~91MB** (Tab) + ~6MB GPU           |
| GPU process           | 42MB                   | 68MB baseline, 96MB with 5 guests    |
| Networking            | 21MB                   | (in-process)                         |
| App/main process      | 109MB\*                | main + host renderer ≈ 84MB+         |
| **Total, 3 guests**   | **~367MB**             | **~425MB+** (extrapolated)           |

\* inflated by the 56-frame TIFF snapshot loop churning ~3MB buffers; idle it is lower.

### Disk

Release binary: **4.0MB** (opt-level "s", lto, strip) using the OS-provided
WebKit — vs the ~262MB Electron framework shipped today.

## Go/no-go read

**Technically viable on macOS.** Every hard capability passes, including the
two considered most at-risk (eval round-trips and 12fps capture). But:

- **The memory win is modest** (~26MB/guest, ~15% total): WebContent guests are
  cheaper than Chromium tabs but not free, and the WS3 hidden-guest LRU budget
  (PR #26) already removes most idle-guest cost inside Electron.
- **The step-change is disk** (~262MB → ~4MB runtime), which stacks with
  WS1/WS2/WS4a only if the _entire app_ migrates — the preview subsystem alone
  can't ship as Tauri while the shell stays Electron.
- **Real costs of a migration**: rebuild Manager.ts's CDP layer per platform
  (Windows/wry = WebView2 which _does_ speak CDP; Linux = WebKitGTK — a third
  capability matrix), accept degraded network observation on macOS or build a
  proxy, and re-verify `<webview>`-equivalent behaviors (crash recovery,
  offscreen automation, PiP) that this probe did not cover.

**Recommendation: no-go for now.** Take the WS1–WS4 disk wins and the WS3
memory fix first; revisit Tauri only if a ~250MB install-size step-change is
worth a per-platform automation rewrite.
