// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { afterEach, describe, expect, it } from "vite-plus/test";

import ChatMarkdown, {
  CHAT_MARKDOWN_REHYPE_PLUGINS,
  CHAT_MARKDOWN_REMARK_PLUGINS,
  MemoizedChatMarkdownBlock,
  splitMarkdownIntoBlocks,
} from "./ChatMarkdown";

function renderWholeMarkdown(text: string): string {
  return renderToStaticMarkup(
    <ReactMarkdown
      remarkPlugins={CHAT_MARKDOWN_REMARK_PLUGINS}
      rehypePlugins={CHAT_MARKDOWN_REHYPE_PLUGINS}
    >
      {text}
    </ReactMarkdown>,
  );
}

function renderBlockMarkdown(text: string): string {
  const blocks = splitMarkdownIntoBlocks(text);
  if (blocks === null) return renderWholeMarkdown(text);
  return blocks
    .map((block) =>
      renderToStaticMarkup(
        <MemoizedChatMarkdownBlock
          content={block.content}
          start={block.start}
          components={undefined}
          remarkPlugins={CHAT_MARKDOWN_REMARK_PLUGINS}
          urlTransform={undefined}
        />,
      ),
    )
    .join("\n");
}

describe("splitMarkdownIntoBlocks", () => {
  it("splits paragraphs at blank-line boundaries with document offsets", () => {
    const text = "alpha one\n\nbeta two\n\ngamma three";
    const blocks = splitMarkdownIntoBlocks(text);
    expect(blocks).not.toBeNull();
    expect(blocks?.map((block) => block.content)).toEqual(["alpha one", "beta two", "gamma three"]);
    for (const block of blocks ?? []) {
      expect(text.slice(block.start, block.start + block.content.length)).toBe(block.content);
    }
  });

  it("keeps fenced code containing blank lines in a single block", () => {
    const text = "Intro.\n\n```ts\nconst a = 1;\n\nconst b = 2;\n```\n\nTail.";
    const blocks = splitMarkdownIntoBlocks(text);
    expect(blocks?.map((block) => block.content)).toEqual([
      "Intro.",
      "```ts\nconst a = 1;\n\nconst b = 2;\n```",
      "Tail.",
    ]);
  });

  it("keeps an unclosed fence open through trailing blank lines", () => {
    const text = "para\n\n```python\nprint('hi')\n\nprint('bye')";
    const blocks = splitMarkdownIntoBlocks(text);
    expect(blocks?.map((block) => block.content)).toEqual([
      "para",
      "```python\nprint('hi')\n\nprint('bye')",
    ]);
  });

  it("does not close a backtick fence with a tilde fence or a shorter run", () => {
    const text = "````\n```\ninner\n\n```\n````\n\nafter";
    const blocks = splitMarkdownIntoBlocks(text);
    expect(blocks?.map((block) => block.content)).toEqual([
      "````\n```\ninner\n\n```\n````",
      "after",
    ]);
  });

  it("treats a backtick 'fence' with backticks in the info string as text", () => {
    const text = "``` a `span` ```\n\nafter";
    const blocks = splitMarkdownIntoBlocks(text);
    expect(blocks?.map((block) => block.content)).toEqual(["``` a `span` ```", "after"]);
  });

  it("merges blank-separated list items into one block (loose lists)", () => {
    const text = "1. first\n\n2. second\n\n3. third\n\nAfter the list.";
    const blocks = splitMarkdownIntoBlocks(text);
    expect(blocks?.map((block) => block.content)).toEqual([
      "1. first\n\n2. second\n\n3. third",
      "After the list.",
    ]);
  });

  it("merges indented continuations (list item paragraphs, indented code)", () => {
    const text = "- item\n\n  item continuation\n\nplain para";
    const blocks = splitMarkdownIntoBlocks(text);
    expect(blocks?.map((block) => block.content)).toEqual([
      "- item\n\n  item continuation",
      "plain para",
    ]);

    const indentedCode = "    line one\n\n    line two\n\npara";
    expect(splitMarkdownIntoBlocks(indentedCode)?.map((block) => block.content)).toEqual([
      "    line one\n\n    line two",
      "para",
    ]);
  });

  it("keeps completed block starts stable as streamed text grows", () => {
    const full = "alpha\n\n- a\n\n- b\n\n```\ncode\n\nmore\n```\n\ntail paragraph";
    const fullBlocks = splitMarkdownIntoBlocks(full);
    for (let end = 1; end <= full.length; end += 1) {
      const prefixBlocks = splitMarkdownIntoBlocks(full.slice(0, end));
      expect(prefixBlocks).not.toBeNull();
      // A block surviving to the full split keeps its start offset and only
      // ever grows (list keys stay stable while streaming appends; boundary
      // merges extend a block in place).
      for (const block of prefixBlocks ?? []) {
        const match = fullBlocks?.find((candidate) => candidate.start === block.start);
        if (match) {
          expect(match.content.startsWith(block.content)).toBe(true);
        }
      }
    }
  });

  describe("conservative whole-text fallbacks", () => {
    it.each([
      ["link reference definition", "[ref]: https://example.com\n\nSee [docs][ref]."],
      ["definition after use", "Uses a [linked][ref] label.\n\n[ref]: https://example.com"],
      ["footnote definition", "Note[^1].\n\n[^1]: the footnote body"],
      ["definition inside a blockquote", "> [ref]: https://example.com/q\n\nA [link][ref]."],
      ["definition as a list item", "- [ref]: https://example.com"],
      ["html block", "<details>\n<summary>More</summary>\n\nhidden body\n\n</details>"],
      ["inline raw html", "Some <em>raw</em> html.\n\nSecond paragraph."],
      ["html comment", "before\n\n<!-- note -->\n\nafter"],
    ])("falls back on %s", (_name, text) => {
      expect(splitMarkdownIntoBlocks(text)).toBeNull();
    });

    it.each([
      ["autolinks", "<https://example.com>\n\npara"],
      ["comparisons without tags", "compare 1 < 2 and 4 <5\n\npara"],
      [
        "definition-like text inside a fence",
        "```\n[ref]: https://example.com\n<div>\n```\n\npara",
      ],
      ["task list items with colons", "- [ ] item: with colon\n\npara"],
    ])("does not fall back on %s", (_name, text) => {
      expect(splitMarkdownIntoBlocks(text)).not.toBeNull();
    });
  });
});

const EQUIVALENCE_CORPUS: Record<string, string> = {
  "headings and paragraphs": "# Title\n\nIntro with **bold** and _em_.\n\n## Section\n\nMore text.",
  "nested lists": "- one\n  - one.a\n  - one.b\n- two\n\nAfter list.",
  "loose ordered list": "1. first\n\n2. second\n\n3. third",
  "list after paragraph": "Steps:\n\n1. alpha\n2. beta\n\nDone.",
  table: "Before.\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\nAfter.",
  "fenced code with blank lines": "Text.\n\n```ts\nconst a = 1;\n\nconst b = 2;\n```\n\nTail.",
  "tilde fence containing backtick fence": "~~~\n```\ninner\n```\n\nstill code\n~~~\n\npara",
  "unclosed streaming fence": "para\n\n```python\nprint('hi')\n\nprint('bye')",
  blockquotes: "> quote line one\n> line two\n\n> separate quote\n\nplain",
  links: "See [docs](https://example.com/a) and `code`.\n\nAnother [link](https://example.com/b).",
  "setext headings": "Title\n=====\n\nBody\n----\n\ntext",
  "thematic break": "before\n\n---\n\nafter",
  "indented code chunks": "    line one\n\n    line two\n\npara",
  "task list": "- [ ] open\n- [x] done\n\npara",
  strikethrough: "Some ~~gone~~ text.\n\nNext block.",
  "reference definitions (fallback)": "[ref]: https://example.com\n\nSee [docs][ref].",
  "raw html (fallback)": "<details>\n<summary>More</summary>\n\nhidden\n\n</details>",
};

describe("block rendering matches whole-text rendering", () => {
  it.each(Object.entries(EQUIVALENCE_CORPUS))("%s", (_name, text) => {
    expect(renderBlockMarkdown(text)).toBe(renderWholeMarkdown(text));
  });

  it("actually exercises multi-block rendering for the non-fallback corpus", () => {
    const multiBlock = Object.entries(EQUIVALENCE_CORPUS).filter(
      // Blank-separated list items intentionally merge into a single block.
      ([name]) => !name.includes("fallback") && name !== "loose ordered list",
    );
    for (const [, text] of multiBlock) {
      expect((splitMarkdownIntoBlocks(text) ?? []).length).toBeGreaterThan(1);
    }
  });

  it("matches whole-text rendering at every streaming prefix", () => {
    const full =
      "# Streaming\n\nIntro paragraph.\n\n- [ ] task one\n- [x] task two\n\n" +
      "```ts\nconst a = 1;\n\nconst b = 2;\n```\n\n" +
      "| a | b |\n| - | - |\n| 1 | 2 |\n\n1. first\n\n2. second\n\n> quoted\n\nDone.";
    for (let end = 1; end <= full.length; end += 3) {
      const prefix = full.slice(0, end);
      expect(renderBlockMarkdown(prefix)).toBe(renderWholeMarkdown(prefix));
    }
  });
});

describe("ChatMarkdown streaming output matches settled output", () => {
  function renderChatMarkdown(text: string, isStreaming: boolean): string {
    // React useId values (Base UI tooltip wiring) depend on tree position and
    // differ between any two renders; they carry no visual output.
    return renderToStaticMarkup(
      <ChatMarkdown text={text} cwd={undefined} isStreaming={isStreaming} />,
    ).replaceAll(/_R_[0-9a-z]+_/g, "_R_x_");
  }

  const corpus: Record<string, string> = {
    "headings, lists, quotes":
      "# Title\n\nIntro paragraph.\n\n- one\n  - nested\n- two\n\n> quote\n\nDone.",
    "task lists keep document-global marker offsets":
      "Intro para.\n\n- [ ] first\n- [x] second\n\nMiddle para.\n\n- [ ] third task",
    "links and emphasis": "See [docs](https://example.com/a).\n\nMore **bold** text with `code`.",
    "reference definitions fall back": "[ref]: https://example.com\n\nUses [docs][ref] afterwards.",
  };

  it.each(Object.entries(corpus))("%s", (_name, text) => {
    expect(renderChatMarkdown(text, true)).toBe(renderChatMarkdown(text, false));
  });

  it("emits marker offsets relative to the full text from later blocks", () => {
    const text = "Intro para.\n\n- [ ] first\n- [x] second\n\nMiddle.\n\n- [ ] third task";
    const streaming = renderChatMarkdown(text, true);
    const expectedThirdOffset = text.indexOf("[ ] third task");
    expect(streaming).toContain(`data-task-marker-offset="${expectedThirdOffset}"`);
    expect(renderChatMarkdown(text, false)).toContain(
      `data-task-marker-offset="${expectedThirdOffset}"`,
    );
  });
});

describe("completed blocks do not re-parse while the trailing block grows", () => {
  let parseCount = 0;
  function remarkCountParses() {
    return () => {
      parseCount += 1;
    };
  }
  const COUNTING_REMARK_PLUGINS = [remarkGfm, remarkCountParses];

  function BlockHarness({ text }: { text: string }) {
    const blocks = splitMarkdownIntoBlocks(text);
    if (blocks === null) throw new Error("expected block rendering for harness text");
    return (
      <>
        {blocks.map((block) => (
          <MemoizedChatMarkdownBlock
            key={block.start}
            content={block.content}
            start={block.start}
            components={undefined}
            remarkPlugins={COUNTING_REMARK_PLUGINS}
            urlTransform={undefined}
          />
        ))}
      </>
    );
  }

  let root: Root | null = null;
  let container: HTMLElement | null = null;

  afterEach(async () => {
    await act(async () => {
      root?.unmount();
    });
    container?.remove();
    root = null;
    container = null;
  });

  it("re-parses only the trailing block per streaming commit", async () => {
    (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    parseCount = 0;

    const first = "alpha one\n\nbeta two\n\ngamma thr";
    await act(async () => {
      root?.render(<BlockHarness text={first} />);
    });
    expect(parseCount).toBe(3);

    // Trailing block grows: only that block re-parses.
    const second = `${first}ee grows`;
    await act(async () => {
      root?.render(<BlockHarness text={second} />);
    });
    expect(parseCount).toBe(4);

    // A blank line completes the trailing block unchanged and opens a new
    // trailing block: only the new block parses.
    const third = `${second}\n\ndelta new`;
    await act(async () => {
      root?.render(<BlockHarness text={third} />);
    });
    expect(parseCount).toBe(5);

    // Re-render with identical text: nothing re-parses.
    await act(async () => {
      root?.render(<BlockHarness text={third} />);
    });
    expect(parseCount).toBe(5);
  });
});
