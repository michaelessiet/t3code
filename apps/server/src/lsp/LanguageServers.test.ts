import { LSP_SUPPORTED_EXTENSIONS } from "@t3tools/shared/lspSupport";
import { describe, expect, it } from "vite-plus/test";

import { languageBindingForPath } from "./LanguageServers.ts";

describe("LanguageServers", () => {
  it("stays in parity with the shared LSP_SUPPORTED_EXTENSIONS set", () => {
    for (const extension of LSP_SUPPORTED_EXTENSIONS) {
      expect(languageBindingForPath(`file${extension}`)).not.toBe(null);
    }
    // The reverse direction: anything the server binds must be advertised.
    const probed = [
      ".ts",
      ".mts",
      ".cts",
      ".tsx",
      ".js",
      ".mjs",
      ".cjs",
      ".jsx",
      ".rs",
      ".py",
      ".pyi",
      ".go",
    ];
    for (const extension of probed) {
      expect(LSP_SUPPORTED_EXTENSIONS.has(extension)).toBe(
        languageBindingForPath(`file${extension}`) !== null,
      );
    }
  });
});
