import { describe, expect, it } from "vite-plus/test";

import {
  resolveMarkdownFileLinkMeta,
  resolveMarkdownFileLinkTarget,
  rewriteMarkdownFileUriHref,
} from "./markdown-links";

describe("rewriteMarkdownFileUriHref", () => {
  it("rewrites file uri hrefs into direct path hrefs", () => {
    expect(rewriteMarkdownFileUriHref("file:///Users/julius/project/src/main.ts#L42")).toBe(
      "/Users/julius/project/src/main.ts#L42",
    );
  });

  it("preserves encoded octets so file paths are decoded only once later", () => {
    expect(rewriteMarkdownFileUriHref("file:///Users/julius/project/file%2520name.md")).toBe(
      "/Users/julius/project/file%2520name.md",
    );
  });

  it("normalizes file uri hrefs for windows drive paths", () => {
    expect(
      rewriteMarkdownFileUriHref(
        "file:///D:/Programme/t3code/apps/web/src/components/chat/OpenInPicker.tsx#L69",
      ),
    ).toBe("D:/Programme/t3code/apps/web/src/components/chat/OpenInPicker.tsx#L69");
  });

  it("unwraps angle-bracketed file uri hrefs", () => {
    expect(
      rewriteMarkdownFileUriHref(" <file:///D:/Programme/t3code/apps/web/src/markdown-links.ts> "),
    ).toBe("D:/Programme/t3code/apps/web/src/markdown-links.ts");
  });
});

describe("resolveMarkdownFileLinkTarget", () => {
  it("resolves absolute posix file paths", () => {
    expect(resolveMarkdownFileLinkTarget("/Users/julius/project/AGENTS.md")).toBe(
      "/Users/julius/project/AGENTS.md",
    );
  });

  it("resolves relative file paths against cwd", () => {
    expect(resolveMarkdownFileLinkTarget("src/processRunner.ts:71", "/Users/julius/project")).toBe(
      "/Users/julius/project/src/processRunner.ts:71",
    );
  });

  it("does not treat filename line references as external schemes", () => {
    expect(resolveMarkdownFileLinkTarget("script.ts:10", "/Users/julius/project")).toBe(
      "/Users/julius/project/script.ts:10",
    );
  });

  it("resolves bare file names against cwd", () => {
    expect(resolveMarkdownFileLinkTarget("AGENTS.md", "/Users/julius/project")).toBe(
      "/Users/julius/project/AGENTS.md",
    );
  });

  it("maps #L line anchors to editor line suffixes", () => {
    expect(resolveMarkdownFileLinkTarget("/Users/julius/project/src/main.ts#L42C7")).toBe(
      "/Users/julius/project/src/main.ts:42:7",
    );
  });

  it("ignores external urls", () => {
    expect(resolveMarkdownFileLinkTarget("https://example.com/docs")).toBeNull();
  });

  it("does not double-decode file URLs", () => {
    expect(resolveMarkdownFileLinkTarget("file:///Users/julius/project/file%2520name.md")).toBe(
      "/Users/julius/project/file%20name.md",
    );
  });

  it("formats tooltip display paths relative to the cwd when possible", () => {
    expect(
      resolveMarkdownFileLinkMeta(
        "file:///C:/Users/mike/dev-stuff/t3code/apps/web/src/session-logic.ts#L501",
        "C:/Users/mike/dev-stuff/t3code",
      ),
    ).toMatchObject({
      displayPath: "t3code/apps/web/src/session-logic.ts:501",
      workspaceRelativePath: "apps/web/src/session-logic.ts",
    });
  });

  it("formats tooltip display paths relative to the cwd for slash-prefixed windows paths", () => {
    expect(
      resolveMarkdownFileLinkMeta(
        "/C:/Users/mike/dev-stuff/t3code/apps/web/src/components/chat/MessagesTimeline.virtualization.browser.tsx",
        "C:/Users/mike/dev-stuff/t3code",
      ),
    ).toMatchObject({
      displayPath:
        "t3code/apps/web/src/components/chat/MessagesTimeline.virtualization.browser.tsx",
      workspaceRelativePath:
        "apps/web/src/components/chat/MessagesTimeline.virtualization.browser.tsx",
    });
  });

  it("does not create a preview path for files outside the workspace", () => {
    expect(resolveMarkdownFileLinkMeta("/tmp/report.ts", "/repo/project")).toMatchObject({
      workspaceRelativePath: null,
    });
  });

  it("parses a line range into start and end lines", () => {
    expect(
      resolveMarkdownFileLinkMeta("/repo/project/src/authStorage.ts:65-79", "/repo/project"),
    ).toMatchObject({
      workspaceRelativePath: "src/authStorage.ts",
      displayPath: "project/src/authStorage.ts:65-79",
      line: 65,
      endLine: 79,
    });
  });

  it("resolves a relative path candidate carrying a line range", () => {
    expect(resolveMarkdownFileLinkMeta("src/authStorage.ts:65-79", "/repo/project")).toMatchObject({
      workspaceRelativePath: "src/authStorage.ts",
      line: 65,
      endLine: 79,
    });
  });

  it("drops an inverted or empty line range", () => {
    expect(
      resolveMarkdownFileLinkMeta("/repo/project/src/authStorage.ts:79-65", "/repo/project"),
    ).toMatchObject({ line: 79 });
    expect(
      resolveMarkdownFileLinkMeta("/repo/project/src/authStorage.ts:79-65", "/repo/project"),
    ).not.toHaveProperty("endLine");
    expect(
      resolveMarkdownFileLinkMeta("/repo/project/src/authStorage.ts:65-65", "/repo/project"),
    ).not.toHaveProperty("endLine");
  });

  it("normalizes slash-prefixed windows drive paths before resolving", () => {
    expect(
      resolveMarkdownFileLinkTarget(
        "/D:/Programme/t3code/apps/web/src/components/chat/OpenInPicker.tsx#L69",
      ),
    ).toBe("D:/Programme/t3code/apps/web/src/components/chat/OpenInPicker.tsx:69");
  });

  it("resolves angle-bracketed windows drive paths", () => {
    expect(
      resolveMarkdownFileLinkTarget(
        "</D:/Programme/t3code/apps/web/src/components/ChatMarkdown.tsx:1>",
      ),
    ).toBe("D:/Programme/t3code/apps/web/src/components/ChatMarkdown.tsx:1");
  });

  it("does not treat app routes as file links", () => {
    expect(resolveMarkdownFileLinkTarget("/chat/settings")).toBeNull();
  });
});
