# Native editor tools

Implemented on `feat/vitre-rust-port`, September 15, 2026. These are
native editor features, not a claim of complete VS Code/Zed compatibility.

## Shortcuts

Use Ctrl instead of Cmd on Windows/Linux. Editor shortcuts require editor focus;
macOS may require Fn with function keys, depending on keyboard settings.

| Feature                    | Shortcut / entry point                               |
| -------------------------- | ---------------------------------------------------- |
| Rename symbol              | F2; editor context menu                              |
| Quick fixes and refactors  | Cmd+. on a diagnostic or selected code               |
| Insert snippet             | Cmd+Option+J; editor context menu                    |
| Next matching selection    | Cmd+D; selects the word first if nothing is selected |
| All matching selections    | Cmd+Shift+L                                          |
| Additional cursor          | Option-click; Cmd+Option+Up/Down                     |
| Snippet navigation         | Tab / Shift+Tab; Escape exits                        |
| Start / continue debugging | F5; editor context menu                              |
| Toggle breakpoint          | F9; click the breakpoint gutter                      |
| Pause / stop debugging     | F6 / Shift+F5                                        |
| Step over / into / out     | F10 / F11 / Shift+F11                                |

Cmd+D still toggles the diff outside the editor. The native client narrows only
T3's stock diff binding; explicit custom editor bindings are not rewritten.
Cmd+Shift+J retains its existing browser-preview shortcut.

## Language intelligence and refactors

Semantic highlighting layers server-provided symbol information over Tree-sitter.
Server legends and UTF-16 positions are normalized into native theme categories;
stale responses are discarded. Availability depends on the configured language
server. The bundled TypeScript service supports TS/JS/TSX/JSX; other semantic
services need an installed/configured server.

Document changes follow the server's negotiated full/incremental sync mode.
The incremental replacement range uses the previous buffer's UTF-16 extent,
including CRLF and emoji. This fixes a TypeScript server crash when snippets
or undo add/remove lines. Delayed semantic requests also reject old snapshots
before syncing, not just before painting.

Save the current file before requesting a refactor. Rename and text-edit code
actions show **Before / After** previews for each affected file. Applying checks
every file's revision before starting, then guards each individual write again.
Dirty/read-only buffers, invalid/overlapping edit ranges, truncated reads and
missing revision guards are refused. Applying saves to disk; it is not an editor
undo transaction. Use source control to review/revert a completed workspace edit.

Multi-file writes are not a filesystem transaction: a race or I/O error during
the writes can leave some files updated. The notification reports how many were
written; review those files before retrying. The preview is limited to 128 files.
Actions requiring arbitrary server commands or file creation/deletion/renaming
are visibly disabled, rather than silently applying only part of the action.
The bundled TypeScript server's edit-only quick fixes and refactors work even
when it attaches optional diagnostic/telemetry bookkeeping commands. Those
commands are never executed; the complete text edit is previewed and applied.

## Snippets and simultaneous editing

Built-ins cover common JS/TS/React, Rust, Python and Go constructs. Add project
snippets in `.vitre/snippets.json`:

```json
{
  "React element": {
    "prefix": "element",
    "scope": "tsx,javascript",
    "body": ["<${1:section}>", "  ${2:content}", "</$1>$0"]
  }
}
```

Supported syntax: numbered stops, defaults, mirrored stops, final `$0`, escaped
literals and choice placeholders (initial choice). Tab/Shift+Tab select stops;
edits to mirrored placeholders update together. Multi-line insertion inherits
indentation. Nested placeholders, variables, regex transformations and a choice
selection UI are not implemented; unsupported expressions produce an error.
Scope names use native grammar IDs, e.g. `tsx`, `typescript`, `javascript`, `rust`.

Multiple selections support typing, paste, deletion and atomic undo. Copy/cut
join selected text with newlines; paste repeats the clipboard at every cursor.
Ordinary navigation, explicit range edits and IME composition collapse to one
cursor. This is not full rectangular-selection or multi-cursor Vim support.

## Debugging

F5 opens a local executable launcher, or configuration choices from
`.vitre/launch.json`. Compile the program with debug symbols first. The launcher
does not install adapters or run build tasks. Start only configurations you trust:
an adapter and its target are executable programs running on your machine.

Example using Xcode's LLDB DAP adapter (adjust the adapter path for your system):

```json
{
  "configurations": [
    {
      "name": "Debug native app",
      "adapter": {
        "command": "/Applications/Xcode.app/Contents/Developer/usr/bin/lldb-dap",
        "args": []
      },
      "request": "launch",
      "program": "${workspaceFolder}/target/debug/my-program",
      "cwd": "${workspaceFolder}",
      "args": [],
      "stopOnEntry": true
    }
  ]
}
```

The native client supports stdio DAP adapters, `launch`/`attach`, workspace-folder
substitution, line breakpoints, stepping, stack navigation, scopes/variables,
output and expression evaluation. Breakpoints are session-local; filled markers
are adapter-verified, hollow markers are pending/unverified. Source navigation
stays inside the workspace. Stop sends `disconnect`, terminating launched targets
but requesting detach for attached targets. Transport frames, queues, timeouts
and output history are bounded; the adapter child is owned by the connection.

Not implemented: TCP/remote adapters, automatic adapter installation, VS Code
extension/configuration compatibility, build/test task discovery, persistent or
conditional breakpoints, watch expressions, multi-thread selection and breakpoint
rebasing after source edits. Restart debugging after changing executable code.
Only local macOS LLDB launch/step/stop has live acceptance evidence so far; other
adapters and attach need separate acceptance tests.

## Verification

The disposable native `VITRE_VERIFY_ICONS=languages` harness uses a fresh
`VITRE_HOME` whose basename starts with `vitre-polish-`. It exercises the real
bundled TypeScript server and an LLDB-backed, compiled C fixture. No user project
or live app database is used. Native screenshots are written to
`/tmp/vitre-ui-editor-*.png` and `/tmp/vitre-ui-languages-*.png`.

Focused tests cover wire mappings and the real language server, UTF-16 semantic
conversion, safe edit application, snippet configuration, simultaneous edits,
adjacent/linked snippet stops, overlapping matches, undo and DAP frame validation.
The final native pass includes cross-file rename and revision-conflict rejection,
const-to-let quick-fix resolution/preview/apply, snippets, multi-cursor shortcuts
and an LLDB breakpoint/step/variable/stop cycle. Before/After panes and the actual
snippet/quick-fix/debugger surfaces were inspected in native screenshots.
Workspace-wide tests are left to CI. CodeRabbit review was unavailable because
its CLI is not installed.
