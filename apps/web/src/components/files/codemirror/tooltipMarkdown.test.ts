// @vitest-environment happy-dom
import { describe, expect, it } from "vite-plus/test";

import { appendHighlightedCode, renderTooltipMarkdown } from "./tooltipMarkdown";

describe("renderTooltipMarkdown", () => {
  it("renders fenced code blocks with syntax highlight spans", () => {
    const dom = renderTooltipMarkdown('```typescript\nconst a = "x";\n```\nSome docs.');
    const pre = dom.querySelector("pre.cm-lsp-md-code");
    expect(pre).not.toBe(null);
    expect(pre!.textContent).toBe('const a = "x";');
    // The keyword and string tokens must carry highlight classes.
    expect(pre!.querySelectorAll("span[class]").length).toBeGreaterThan(0);
    expect(dom.textContent).toContain("Some docs.");
  });

  it("renders bold, italics, and inline code", () => {
    const dom = renderTooltipMarkdown("**When to use** with *care* and `Effect.succeed(1)`.");
    expect(dom.querySelector("strong")?.textContent).toBe("When to use");
    expect(dom.querySelector("em")?.textContent).toBe("care");
    expect(dom.querySelector("code")?.textContent).toBe("Effect.succeed(1)");
    expect(dom.textContent).not.toContain("**");
  });

  it("renders links as titled text instead of raw urls", () => {
    const dom = renderTooltipMarkdown("*@see* — [fail](file:///huge/path/Effect.d.ts#L1947) docs");
    const link = dom.querySelector(".cm-lsp-md-link");
    expect(link?.textContent).toBe("fail");
    expect(link?.getAttribute("title")).toContain("file:///huge/path");
    expect(dom.textContent).not.toContain("file:///huge/path");
  });

  it("splits paragraphs on blank lines", () => {
    const dom = renderTooltipMarkdown("first\n\nsecond");
    const paragraphs = dom.querySelectorAll(".cm-lsp-md-paragraph");
    expect(paragraphs).toHaveLength(2);
  });
});

describe("appendHighlightedCode", () => {
  it("marks the emphasized range while preserving token classes", () => {
    const parent = document.createElement("div");
    // Emphasize "value: boolean" inside the signature.
    const code = "charAt(value: boolean): string";
    const from = code.indexOf("value: boolean");
    appendHighlightedCode(parent, code, from, from + "value: boolean".length);
    const emphasized = [...parent.querySelectorAll(".cm-lsp-active-param")]
      .map((node) => node.textContent)
      .join("");
    expect(emphasized).toBe("value: boolean");
    expect(parent.textContent).toBe(code);
  });
});
