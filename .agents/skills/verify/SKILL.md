# Verify web UI changes

Runtime verification recipe for `apps/web` changes. For environment setup (isolated home dir, `vp run dev`, pairing tokens), follow [test-t3-app](../test-t3-app/SKILL.md) first.

## Drive the UI

Preferred: the t3-code preview MCP tools (`preview_open` with the pairing URL, then `preview_snapshot`/`preview_click`).

When preview MCP half-fails (open/status work, evaluate/snapshot time out), fall back to the repo's Electron binary as a driver:

- Binary: `node_modules/.pnpm/electron@<ver>/node_modules/electron/dist/Electron.app/Contents/MacOS/Electron`
- Launch with `env -u ELECTRON_RUN_AS_NODE` (that var is set in agent shells and silently turns Electron into plain Node).
- Main script pattern: hidden BrowserWindow (`paintWhenInitiallyHidden: true`, `backgroundThrottling: false`) on the pairing URL + tiny HTTP server exposing `/eval` (executeJavaScript), `/click` (sendInputEvent, CSS px), `/shot` (capturePage PNG), `/url`. Drive with curl; keep one process alive so the single-use pairing token isn't burned per step.

## Getting a chat thread with known assistant markdown

Do NOT seed `projection_threads`/`projection_thread_messages` directly for chat-surface tests: the event-sourced projector deletes projection rows with no backing `orchestration_events` — at startup for the thread list, and again the moment the thread is opened. Seeded threads render in the sidebar then show "Send a message to start the conversation", and sends into them fail (`thread.meta.update` rejects the unknown thread id).

Instead create a real turn: register the repo as a project, open a draft (`Create new thread in <project>` aria-label on the home page), pick a Claude model in the model picker (Codex CLI usually isn't installed; Claude is), and send:

> Reply with EXACTLY the following text as your entire reply, verbatim, no tools, no preamble: <markdown under test>

Haiku echoes it in ~20s and the message is event-backed and durable.

Direct projection seeding is still fine for `projection_projects` (survives) and for surfaces that aren't event-sourced.

## Useful assertions from /eval

- Chat file-link chips: `a[data-markdown-copy]` / `a.chat-markdown-file-link` (the `data-file-path-autolink` attribute does not survive into the chip component; it only exists pre-resolution).
- Editor reveal: `.cm-reveal-line` count = highlighted lines; `.cm-content` textContent for which file is open; compare first revealed element's `getBoundingClientRect().top` against `.cm-scroller` midpoint for scroll centering.
