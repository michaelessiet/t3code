import { describe, expect, it } from "vite-plus/test";

import {
  detectComposerTrigger,
  serializeComposerFileLink,
  serializeComposerMentionPath,
} from "./composerTrigger.ts";

describe("serializeComposerMentionPath", () => {
  it("keeps simple mention paths unquoted", () => {
    expect(serializeComposerMentionPath("src/index.ts")).toBe("src/index.ts");
  });

  it("quotes mention paths containing whitespace", () => {
    expect(serializeComposerMentionPath("docs/My File.md")).toBe('"docs/My File.md"');
  });

  it("escapes quoted mention path content", () => {
    expect(serializeComposerMentionPath('docs/My "File".md')).toBe('"docs/My \\"File\\".md"');
  });
});

describe("serializeComposerFileLink", () => {
  it("uses the basename as the markdown label", () => {
    expect(serializeComposerFileLink("path/to/package.json")).toBe(
      "[package.json](path/to/package.json)",
    );
  });

  it("encodes markdown-sensitive destination characters", () => {
    expect(serializeComposerFileLink("docs/My File (draft).md")).toBe(
      "[My File (draft).md](docs/My%20File%20%28draft%29.md)",
    );
  });

  it("supports windows paths", () => {
    expect(serializeComposerFileLink("C:\\repo\\src\\index.ts")).toBe(
      "[index.ts](C:%5Crepo%5Csrc%5Cindex.ts)",
    );
  });

  it("preserves paths that legitimately start with an at sign", () => {
    expect(serializeComposerFileLink("@scope/package.json")).toBe(
      "[package.json](@scope/package.json)",
    );
  });
});

describe("detectComposerTrigger thread references", () => {
  it("detects a # thread trigger mid-sentence", () => {
    const text = "see #auth";
    expect(detectComposerTrigger(text, text.length)).toEqual({
      kind: "thread",
      query: "auth",
      rangeStart: 4,
      rangeEnd: text.length,
    });
  });

  it("opens the trigger on a bare # after other text", () => {
    const text = "see #";
    expect(detectComposerTrigger(text, text.length)).toEqual({
      kind: "thread",
      query: "",
      rangeStart: 4,
      rangeEnd: text.length,
    });
  });

  it("does not treat markdown heading prefixes as thread triggers", () => {
    expect(detectComposerTrigger("#", 1)).toBeNull();
    expect(detectComposerTrigger("##", 2)).toBeNull();
    expect(detectComposerTrigger("intro\n#", 7)).toBeNull();
  });

  it("arms the trigger once a heading-position # gains a query", () => {
    const text = "#auth";
    expect(detectComposerTrigger(text, text.length)).toEqual({
      kind: "thread",
      query: "auth",
      rangeStart: 0,
      rangeEnd: text.length,
    });
  });
});
