import { describe, expect, it } from "vite-plus/test";

import { languageServerEnv } from "./LspClient.ts";

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
