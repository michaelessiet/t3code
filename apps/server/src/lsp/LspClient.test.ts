// @effect-diagnostics nodeBuiltinImport:off
// @effect-diagnostics globalTimers:off
// @effect-diagnostics globalDate:off
// LspClient is deliberately promise-based (see its module doc); its lifecycle
// tests poll real child processes with plain timers for the same reason.
import * as NodeFS from "node:fs";
import * as NodeOS from "node:os";
import * as NodePath from "node:path";

import { describe, expect, it } from "vite-plus/test";

import type { LanguageServerConfig } from "./LanguageServers.ts";
import { languageServerEnv, LspClient, LspClientError, rewriteAsarPath } from "./LspClient.ts";

describe("rewriteAsarPath", () => {
  it("rewrites a packaged app.asar path to its app.asar.unpacked twin", () => {
    // tsserver sets process.noAsar = true before serving requests, so a child
    // spawned with an app.asar path loads zero default libs and reports every
    // ambient global (Pick, JSON, Error, ...) as "Cannot find name".
    expect(
      rewriteAsarPath(
        "/Applications/T3 Code.app/Contents/Resources/app.asar/node_modules/@vtsls/language-server/bin/vtsls.js",
      ),
    ).toBe(
      "/Applications/T3 Code.app/Contents/Resources/app.asar.unpacked/node_modules/@vtsls/language-server/bin/vtsls.js",
    );
  });

  it("leaves dev paths without app.asar untouched", () => {
    const devPath = "/repo/apps/server/node_modules/@vtsls/language-server/bin/vtsls.js";
    expect(rewriteAsarPath(devPath)).toBe(devPath);
  });

  it("does not rewrite path segments that merely contain app.asar", () => {
    const lookalike = "/repo/app.asar.unpacked/node_modules/pkg/index.js";
    expect(rewriteAsarPath(lookalike)).toBe(lookalike);
  });
});

describe("languageServerEnv", () => {
  it("strips Node watch-mode internals the dev runner injects", () => {
    const env = languageServerEnv({
      PATH: "/usr/bin",
      WATCH_REPORT_DEPENDENCIES: "1",
    });
    expect(env.WATCH_REPORT_DEPENDENCIES).toBeUndefined();
    expect(env.PATH).toBe("/usr/bin");
  });

  it("leaves an environment without watch internals untouched", () => {
    const base = { PATH: "/usr/bin", NODE_ENV: "production" };
    expect(languageServerEnv(base)).toEqual(base);
  });
});

const fakeConfig = (
  args: ReadonlyArray<string>,
  initializeTimeoutMs: number,
): LanguageServerConfig => ({
  serverId: "fake",
  displayName: "fake-server",
  command: process.execPath,
  args,
  bundled: false,
  initializeTimeoutMs,
});

const startClient = (
  config: LanguageServerConfig,
  onExit: (detail: string) => void = () => {},
): Promise<LspClient> =>
  LspClient.start({
    workspaceRoot: NodeOS.tmpdir(),
    config,
    onDiagnostics: () => {},
    onExit,
  });

const processAlive = (pid: number): boolean => {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
};

const waitFor = async (predicate: () => boolean, timeoutMs = 10_000): Promise<boolean> => {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) return true;
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  return predicate();
};

const readPid = async (pidFile: string): Promise<number> => {
  await waitFor(() => NodeFS.existsSync(pidFile));
  return Number(NodeFS.readFileSync(pidFile, "utf8"));
};

/** A process that writes its pid, then never speaks LSP: a hung initialize. */
const silentServerScript = (pidFile: string): string =>
  `require("node:fs").writeFileSync(${JSON.stringify(pidFile)}, String(process.pid)); setInterval(() => {}, 1000);`;

/** A server that answers initialize with a JSON-RPC error, then stays alive. */
const initErrorServerScript = (pidFile: string): string =>
  `
const fs = require("node:fs");
fs.writeFileSync(${JSON.stringify(pidFile)}, String(process.pid));
let buffer = Buffer.alloc(0);
process.stdin.on("data", (chunk) => {
  buffer = Buffer.concat([buffer, chunk]);
  const separator = buffer.indexOf("\\r\\n\\r\\n");
  if (separator === -1) return;
  const length = Number(/Content-Length: (\\d+)/.exec(buffer.slice(0, separator).toString())[1]);
  if (buffer.length < separator + 4 + length) return;
  const message = JSON.parse(buffer.slice(separator + 4, separator + 4 + length).toString());
  if (message.method !== "initialize") return;
  const body = JSON.stringify({
    jsonrpc: "2.0",
    id: message.id,
    error: { code: -32603, message: "init exploded" },
  });
  process.stdout.write("Content-Length: " + Buffer.byteLength(body) + "\\r\\n\\r\\n" + body);
});
setInterval(() => {}, 1000);
`.trim();

describe("LspClient.start process lifecycle", () => {
  it("kills the spawned child when initialize times out", async () => {
    const dir = NodeFS.mkdtempSync(NodePath.join(NodeOS.tmpdir(), "t3code-lsp-client-"));
    const pidFile = NodePath.join(dir, "pid");
    const error: unknown = await startClient(
      fakeConfig(["-e", silentServerScript(pidFile)], 750),
    ).catch((cause: unknown) => cause);
    expect(error).toBeInstanceOf(LspClientError);
    expect((error as LspClientError).failure.kind).toBe("timed_out");
    const pid = await readPid(pidFile);
    expect(await waitFor(() => !processAlive(pid))).toBe(true);
  }, 20_000);

  it("kills the spawned child when the server answers initialize with an error", async () => {
    const dir = NodeFS.mkdtempSync(NodePath.join(NodeOS.tmpdir(), "t3code-lsp-client-"));
    const pidFile = NodePath.join(dir, "pid");
    const error: unknown = await startClient(
      fakeConfig(["-e", initErrorServerScript(pidFile)], 15_000),
    ).catch((cause: unknown) => cause);
    expect(error).toBeInstanceOf(LspClientError);
    expect((error as LspClientError).failure.kind).toBe("request_failed");
    const pid = await readPid(pidFile);
    expect(await waitFor(() => !processAlive(pid))).toBe(true);
  }, 20_000);

  it("rejects promptly when the server exits during initialize", async () => {
    let exitDetail: string | null = null;
    // The 30s initialize budget must not be what unblocks this: the exit
    // handler fails the in-flight initialize as soon as the child dies, so
    // the test finishing inside its own timeout proves the fail-fast path.
    const error: unknown = await startClient(
      fakeConfig(["-e", "process.exit(7)"], 30_000),
      (detail) => {
        exitDetail = detail;
      },
    ).catch((cause: unknown) => cause);
    expect(error).toBeInstanceOf(LspClientError);
    expect(exitDetail).toContain("7");
  }, 15_000);
});
