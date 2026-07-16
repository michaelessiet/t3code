import { describe, expect, it } from "vite-plus/test";

import { fileContentRevision } from "./fileRevision.ts";

describe("fileContentRevision", () => {
  it("changes for same-length edits", () => {
    expect(fileContentRevision("nodeVersion")).not.toBe(fileContentRevision("nodeVeasdrs"));
  });

  it("keeps identical contents stable", () => {
    expect(fileContentRevision("contents")).toBe(fileContentRevision("contents"));
  });

  it("prefixes the token with the content length", () => {
    expect(fileContentRevision("abc").startsWith("3:")).toBe(true);
    expect(fileContentRevision("")).toBe(`0:${(2_166_136_261 >>> 0).toString(36)}`);
  });
});
