import { renderToStaticMarkup } from "react-dom/server";
import ReactMarkdown from "react-markdown";
import rehypeRaw from "rehype-raw";
import rehypeSanitize from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import { describe, expect, it } from "vite-plus/test";

import { remarkLinkifyFilePaths } from "./markdown-file-path-autolink";
import { CHAT_MARKDOWN_SANITIZE_SCHEMA } from "./markdown-sanitize-schema";

const WORKSPACE_FILES = new Set([
  "apps/web/src/components/ChatMarkdown.tsx",
  "apps/web/src/markdown-links.ts",
  "package.json",
  "docs/README.md",
]);

const CWD = "/Users/julius/project";

function resolve(candidate: string): string | null {
  const match = candidate.match(/^(.*?)((?::\d+){0,2})$/);
  const path = match?.[1] ?? candidate;
  const position = match?.[2] ?? "";
  const relativePath = path.startsWith(`${CWD}/`) ? path.slice(CWD.length + 1) : path;
  if (!WORKSPACE_FILES.has(relativePath)) return null;
  return `${CWD}/${relativePath}${position}`;
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

  it("leaves paths the workspace cannot confirm as plain text", () => {
    const html = renderMarkdown("I changed apps/web/src/imaginary/Nope.tsx:4 for you.");

    expect(html).not.toContain("<a");
    expect(html).toContain("apps/web/src/imaginary/Nope.tsx:4");
  });

  it("leaves prose that merely looks path-shaped alone", () => {
    const html = renderMarkdown("Use a fallback, e.g. when the value is 1.2.3 or v2.0.");

    expect(html).not.toContain("<a");
  });

  it("does not linkify inside inline code", () => {
    const html = renderMarkdown("Run `pnpm test apps/web/src/markdown-links.ts` locally.");

    expect(html).toContain("<code>pnpm test apps/web/src/markdown-links.ts</code>");
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
});
