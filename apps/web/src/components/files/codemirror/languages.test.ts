import { describe, expect, it } from "vite-plus/test";

import { editorLanguageIdForPath, languageExtensionForPath } from "./languages";

describe("editorLanguageIdForPath", () => {
  it("maps common source extensions to their language ids", () => {
    expect(editorLanguageIdForPath("src/index.ts")).toBe("typescript");
    expect(editorLanguageIdForPath("src/App.tsx")).toBe("tsx");
    expect(editorLanguageIdForPath("scripts/build.mjs")).toBe("javascript");
    expect(editorLanguageIdForPath("components/Button.jsx")).toBe("jsx");
    expect(editorLanguageIdForPath("package.json")).toBe("json");
    expect(editorLanguageIdForPath("styles/main.css")).toBe("css");
    expect(editorLanguageIdForPath("public/index.html")).toBe("html");
    expect(editorLanguageIdForPath("docs/guide.mdx")).toBe("markdown");
    expect(editorLanguageIdForPath("tools/run.py")).toBe("python");
    expect(editorLanguageIdForPath("src/main.rs")).toBe("rust");
    expect(editorLanguageIdForPath(".github/workflows/ci.yml")).toBe("yaml");
  });

  it("matches extensions case-insensitively", () => {
    expect(editorLanguageIdForPath("README.MD")).toBe("markdown");
    expect(editorLanguageIdForPath("src/Index.TS")).toBe("typescript");
  });

  it("handles Windows-style separators", () => {
    expect(editorLanguageIdForPath("src\\components\\App.tsx")).toBe("tsx");
  });

  it("falls back to plain for unknown or missing extensions", () => {
    expect(editorLanguageIdForPath("LICENSE")).toBe("plain");
    expect(editorLanguageIdForPath("bin/tool.exe")).toBe("plain");
    expect(editorLanguageIdForPath("src/archive.tar.gz")).toBe("plain");
  });

  it("does not treat dotfiles as having an extension", () => {
    expect(editorLanguageIdForPath(".gitignore")).toBe("plain");
    expect(editorLanguageIdForPath("config/.env")).toBe("plain");
  });
});

describe("languageExtensionForPath", () => {
  it("returns a non-empty extension for supported languages", () => {
    expect(languageExtensionForPath("src/index.ts")).not.toEqual([]);
    expect(languageExtensionForPath("notes.md")).not.toEqual([]);
  });

  it("returns an empty extension for plain files", () => {
    expect(languageExtensionForPath("LICENSE")).toEqual([]);
  });
});
