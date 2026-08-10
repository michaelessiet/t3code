import { describe, expect, it } from "vite-plus/test";

import { languageServerEnv, rewriteAsarPath } from "./LspClient.ts";

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
