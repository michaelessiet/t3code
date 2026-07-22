/**
 * Minimal markdown-to-DOM renderer for LSP tooltip content (hover docs,
 * signature help). Handles the subset language servers actually emit —
 * fenced code blocks, inline code, bold/italic, links, paragraphs — and
 * syntax-highlights code with the editor's own HighlightStyle so tooltip
 * code matches the buffer. A full markdown pipeline (react-markdown) is
 * deliberately avoided: tooltips are imperative DOM owned by CodeMirror.
 */
import { javascript } from "@codemirror/lang-javascript";
import { highlightCode } from "@lezer/highlight";

import { editorHighlightStyle } from "./theme";

const tsParser = javascript({ typescript: true }).language.parser;

/**
 * Append `code` to `parent` as syntax-highlighted spans, optionally marking
 * the [emphasisFrom, emphasisTo) character range (active parameter).
 */
export function appendHighlightedCode(
  parent: HTMLElement,
  code: string,
  emphasisFrom = -1,
  emphasisTo = -1,
): void {
  let offset = 0;
  const put = (text: string, classes: string) => {
    // Split emitted spans on the emphasis boundaries so the active-parameter
    // range can carry an extra class without breaking token classes.
    let localStart = 0;
    while (localStart < text.length) {
      const absolute = offset + localStart;
      const inEmphasis = absolute >= emphasisFrom && absolute < emphasisTo;
      const boundary = inEmphasis
        ? Math.min(emphasisTo - offset, text.length)
        : absolute < emphasisFrom
          ? Math.min(emphasisFrom - offset, text.length)
          : text.length;
      const segment = text.slice(localStart, boundary);
      if (segment.length > 0) {
        const span = document.createElement("span");
        if (classes.length > 0) span.className = classes;
        if (inEmphasis) span.classList.add("cm-lsp-active-param");
        span.textContent = segment;
        parent.appendChild(span);
      }
      localStart = boundary;
    }
    offset += text.length;
  };
  highlightCode(code, tsParser.parse(code), editorHighlightStyle, put, () => {
    parent.appendChild(document.createElement("br"));
    offset += 1;
  });
}

const INLINE_PATTERN =
  /`([^`]+)`|\*\*([^*]+)\*\*|\*([^*\n]+)\*|_([^_\n]+)_|\[([^\]]*)\]\(([^)]+)\)/g;

function appendInlineMarkdown(parent: HTMLElement, text: string): void {
  let last = 0;
  for (const match of text.matchAll(INLINE_PATTERN)) {
    if (match.index > last) parent.append(text.slice(last, match.index));
    const [, code, bold, italicStar, italicUnderscore, linkText, linkTarget] = match;
    if (code !== undefined) {
      const inline = document.createElement("code");
      appendHighlightedCode(inline, code);
      parent.appendChild(inline);
    } else if (bold !== undefined) {
      parent.appendChild(Object.assign(document.createElement("strong"), { textContent: bold }));
    } else if (italicStar !== undefined || italicUnderscore !== undefined) {
      parent.appendChild(
        Object.assign(document.createElement("em"), {
          textContent: italicStar ?? italicUnderscore ?? "",
        }),
      );
    } else if (linkTarget !== undefined) {
      // Tooltip links are informational: `file://` targets from JSDoc @see
      // tags aren't navigable here, so render the text with the target as a
      // hover title instead of a dead (and often enormous) URL.
      const link = document.createElement("span");
      link.className = "cm-lsp-md-link";
      link.textContent = linkText !== undefined && linkText.length > 0 ? linkText : linkTarget;
      link.title = linkTarget;
      parent.appendChild(link);
    }
    last = match.index + match[0].length;
  }
  if (last < text.length) parent.append(text.slice(last));
}

/** Render tooltip markdown into a detached element. */
export function renderTooltipMarkdown(markdown: string): HTMLElement {
  const root = document.createElement("div");
  root.className = "cm-lsp-markdown";
  const lines = markdown.split("\n");
  let paragraph: Array<string> = [];
  let index = 0;

  const flushParagraph = () => {
    const text = paragraph.join(" ").trim();
    paragraph = [];
    if (text.length === 0) return;
    const block = document.createElement("div");
    block.className = "cm-lsp-md-paragraph";
    appendInlineMarkdown(block, text);
    root.appendChild(block);
  };

  while (index < lines.length) {
    const line = lines[index]!;
    const fence = /^```(\w*)\s*$/.exec(line);
    if (fence !== null) {
      flushParagraph();
      const codeLines: Array<string> = [];
      index += 1;
      while (index < lines.length && !/^```\s*$/.test(lines[index]!)) {
        codeLines.push(lines[index]!);
        index += 1;
      }
      index += 1; // closing fence
      const pre = document.createElement("pre");
      pre.className = "cm-lsp-md-code";
      appendHighlightedCode(pre, codeLines.join("\n"));
      root.appendChild(pre);
      continue;
    }
    if (line.trim().length === 0) {
      flushParagraph();
    } else {
      paragraph.push(line);
    }
    index += 1;
  }
  flushParagraph();
  return root;
}
