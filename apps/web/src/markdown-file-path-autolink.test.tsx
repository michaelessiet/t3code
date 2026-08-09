import { renderToStaticMarkup } from "react-dom/server";
import ReactMarkdown from "react-markdown";
import rehypeRaw from "rehype-raw";
import rehypeSanitize from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import { describe, expect, it } from "vite-plus/test";

import { remarkLinkifyFilePaths } from "./markdown-file-path-autolink";
import { CHAT_MARKDOWN_SANITIZE_SCHEMA } from "./markdown-sanitize-schema";
import { resolveWorkspaceFilePath } from "./workspaceFilePathIndex";

const WORKSPACE_FILES = new Set([
  "apps/web/src/components/ChatMarkdown.tsx",
  "apps/web/src/markdown-links.ts",
  "package.json",
  "docs/README.md",
  "apps/web/src/authStorage.ts",
  "apps/mobile/index.ts",
  "apps/web/index.ts",
]);

const BASENAMES = new Map<string, string | null>([
  ["ChatMarkdown.tsx", "apps/web/src/components/ChatMarkdown.tsx"],
  ["markdown-links.ts", "apps/web/src/markdown-links.ts"],
  ["package.json", "package.json"],
  ["README.md", "docs/README.md"],
  ["authStorage.ts", "apps/web/src/authStorage.ts"],
  ["index.ts", null],
]);

const CWD = "/Users/julius/project";

function resolve(candidate: string): string | null {
  return resolveWorkspaceFilePath(candidate, WORKSPACE_FILES, CWD, BASENAMES);
}

function renderMarkdown(markdown: string): string {
  return renderToStaticMarkup(
    <ReactMarkdown remarkPlugins={[remarkGfm, [remarkLinkifyFilePaths, { resolve }]]}>
      {markdown}
    </ReactMarkdown>,
  );
}

describe("remarkLinkifyFilePaths", () => {
  it("links a bare workspace path written in prose", () => {
    const html = renderMarkdown("The renderer lives in apps/web/src/markdown-links.ts today.");

    expect(html).toContain(
      '<a href="/Users/julius/project/apps/web/src/markdown-links.ts" data-file-path-autolink="true">apps/web/src/markdown-links.ts</a>',
    );
    expect(html).toContain("The renderer lives in ");
    expect(html).toContain(" today.");
  });

  it("carries a line and column suffix into the link target", () => {
    const html = renderMarkdown(
      "See apps/web/src/components/ChatMarkdown.tsx:1386:12 for the fix.",
    );

    expect(html).toContain(
      'href="/Users/julius/project/apps/web/src/components/ChatMarkdown.tsx:1386:12"',
    );
    expect(html).toContain(">apps/web/src/components/ChatMarkdown.tsx:1386:12</a>");
  });

  it("links an absolute path that resolves inside the workspace", () => {
    const html = renderMarkdown("Open /Users/julius/project/package.json to check.");

    expect(html).toContain('href="/Users/julius/project/package.json"');
  });

  it("links a bare filename that exists at the workspace root", () => {
    const html = renderMarkdown("Bump the version in package.json first.");

    expect(html).toContain('href="/Users/julius/project/package.json"');
  });

  it("carries a line range suffix into the link target", () => {
    const html = renderMarkdown("The cleanup spans apps/web/src/authStorage.ts:65-79 today.");

    expect(html).toContain('href="/Users/julius/project/apps/web/src/authStorage.ts:65-79"');
    expect(html).toContain(">apps/web/src/authStorage.ts:65-79</a>");
  });

  it("links a unique basename mentioned in prose", () => {
    const html = renderMarkdown("The shim sits in authStorage.ts today.");

    expect(html).toContain('href="/Users/julius/project/apps/web/src/authStorage.ts"');
    expect(html).toContain(">authStorage.ts</a>");
  });

  it("leaves paths the workspace cannot confirm as plain text", () => {
    const html = renderMarkdown("I changed apps/web/src/imaginary/Nope.tsx:4 for you.");

    expect(html).not.toContain("<a");
    expect(html).toContain("apps/web/src/imaginary/Nope.tsx:4");
  });

  it("leaves prose that merely looks path-shaped alone", () => {
    const html = renderMarkdown("Use a fallback, e.g. when the value is 1.2.3 or v2.0.");

    expect(html).not.toContain("<a");
  });

  it("links an inline code span whose whole content is a workspace path", () => {
    const html = renderMarkdown("Confine the shim to `apps/web/src/authStorage.ts` for now.");

    expect(html).toContain(
      '<a href="/Users/julius/project/apps/web/src/authStorage.ts" data-file-path-autolink="inline-code">apps/web/src/authStorage.ts</a>',
    );
    expect(html).not.toContain("<code>");
  });

  it("links an inline code span carrying a line number", () => {
    const html = renderMarkdown("The store lives at `apps/web/src/authStorage.ts:12`.");

    expect(html).toContain('href="/Users/julius/project/apps/web/src/authStorage.ts:12"');
    expect(html).toContain('data-file-path-autolink="inline-code"');
  });

  it("links an inline code span carrying a line range", () => {
    const html = renderMarkdown("Both stores clear here (`apps/web/src/authStorage.ts:65-79`).");

    expect(html).toContain('href="/Users/julius/project/apps/web/src/authStorage.ts:65-79"');
    expect(html).toContain(">apps/web/src/authStorage.ts:65-79</a>");
  });

  it("normalizes an en-dash line range to a hyphen in the target", () => {
    const html = renderMarkdown("See `apps/web/src/authStorage.ts:65–79` for the cleanup.");

    expect(html).toContain('href="/Users/julius/project/apps/web/src/authStorage.ts:65-79"');
  });

  it("links an inline code basename that is unique in the workspace", () => {
    const html = renderMarkdown("Confining the shim to `authStorage.ts` matches the surface.");

    expect(html).toContain(
      '<a href="/Users/julius/project/apps/web/src/authStorage.ts" data-file-path-autolink="inline-code">authStorage.ts</a>',
    );
  });

  it("leaves an ambiguous inline code basename as plain code", () => {
    const html = renderMarkdown("Start from `index.ts` and follow the imports.");

    expect(html).toContain("<code>index.ts</code>");
    expect(html).not.toContain("<a");
  });

  it("leaves inline code that is not just a path alone", () => {
    const html = renderMarkdown("Run `pnpm test apps/web/src/markdown-links.ts` locally.");

    expect(html).toContain("<code>pnpm test apps/web/src/markdown-links.ts</code>");
    expect(html).not.toContain("<a");
  });

  it("leaves code-shaped inline spans that name no workspace file alone", () => {
    const html = renderMarkdown("Wrap it in `Promise.all` before returning.");

    expect(html).toContain("<code>Promise.all</code>");
    expect(html).not.toContain("<a");
  });

  it("does not linkify inside fenced code", () => {
    const html = renderMarkdown("```\nimport x from apps/web/src/markdown-links.ts\n```");

    expect(html).not.toContain("<a");
  });

  it("leaves the label of an explicit markdown link untouched", () => {
    const html = renderMarkdown(
      "[apps/web/src/markdown-links.ts](./apps/web/src/markdown-links.ts)",
    );

    expect(html).toContain(
      '<a href="./apps/web/src/markdown-links.ts">apps/web/src/markdown-links.ts</a>',
    );
    expect(html).not.toContain("data-file-path-autolink");
  });

  it("does not split a url that contains a workspace path", () => {
    const html = renderMarkdown(
      "Compare github.com/org/repo/blob/main/apps/web/src/markdown-links.ts and the local copy.",
    );

    expect(html).not.toContain("data-file-path-autolink");
    expect(html).toContain("github.com/org/repo/blob/main/apps/web/src/markdown-links.ts");
  });

  it("keeps sentence punctuation outside the link", () => {
    const html = renderMarkdown("Everything routes through apps/web/src/markdown-links.ts.");

    expect(html).toContain('href="/Users/julius/project/apps/web/src/markdown-links.ts"');
    expect(html).toContain(">apps/web/src/markdown-links.ts</a>.");
  });

  it("keeps a wrapping parenthesis outside the link", () => {
    const html = renderMarkdown("The helper (apps/web/src/markdown-links.ts) resolves hrefs.");

    expect(html).toContain(">apps/web/src/markdown-links.ts</a>)");
  });

  it("links several paths in one paragraph", () => {
    const html = renderMarkdown(
      "apps/web/src/markdown-links.ts feeds apps/web/src/components/ChatMarkdown.tsx directly.",
    );

    expect(html.match(/data-file-path-autolink/g)).toHaveLength(2);
  });

  it("links paths inside emphasis and list items", () => {
    const html = renderMarkdown("- **docs/README.md** covers setup");

    expect(html).toContain('href="/Users/julius/project/docs/README.md"');
    expect(html).toContain("<strong>");
  });
});

/**
 * The chat renderer sanitizes before rendering, so an attribute that is not
 * allowlisted disappears and autolinked anchors silently degrade into external
 * links. Exercise the real schema rather than the bare remark pipeline.
 */
describe("remarkLinkifyFilePaths through the chat sanitize pipeline", () => {
  function renderSanitizedMarkdown(markdown: string): string {
    return renderToStaticMarkup(
      <ReactMarkdown
        remarkPlugins={[remarkGfm, [remarkLinkifyFilePaths, { resolve }]]}
        rehypePlugins={[rehypeRaw, [rehypeSanitize, CHAT_MARKDOWN_SANITIZE_SCHEMA]]}
      >
        {markdown}
      </ReactMarkdown>,
    );
  }

  it("keeps the autolink marker and target after sanitizing", () => {
    const html = renderSanitizedMarkdown("Look at apps/web/src/markdown-links.ts:12 for context.");

    expect(html).toContain('data-file-path-autolink="true"');
    expect(html).toContain('href="/Users/julius/project/apps/web/src/markdown-links.ts:12"');
  });

  it("keeps the inline-code marker after sanitizing", () => {
    const html = renderSanitizedMarkdown(
      "Both stores clear in `apps/web/src/authStorage.ts:65-79`.",
    );

    expect(html).toContain('data-file-path-autolink="inline-code"');
    expect(html).toContain('href="/Users/julius/project/apps/web/src/authStorage.ts:65-79"');
  });
});
