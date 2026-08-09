import { describe, expect, it } from "vite-plus/test";

import {
  buildThreadReferenceUrl,
  collectThreadReferences,
  parseThreadReferenceUrl,
  serializeComposerThreadReference,
} from "./threadReferences.ts";

describe("serializeComposerThreadReference", () => {
  it("serializes a title and encoded thread id", () => {
    expect(
      serializeComposerThreadReference({ threadId: "thr_abc123", title: "Fix auth bug" }),
    ).toBe("[#Fix auth bug](t3code://thread/thr_abc123)");
  });

  it("escapes markdown-sensitive label characters", () => {
    expect(serializeComposerThreadReference({ threadId: "t1", title: "Fix [auth] bug" })).toBe(
      "[#Fix \\[auth\\] bug](t3code://thread/t1)",
    );
  });

  it("percent-encodes thread ids", () => {
    expect(serializeComposerThreadReference({ threadId: "id with space", title: "T" })).toBe(
      "[#T](t3code://thread/id%20with%20space)",
    );
  });
});

describe("parseThreadReferenceUrl", () => {
  it("round-trips build/parse", () => {
    expect(parseThreadReferenceUrl(buildThreadReferenceUrl("id with space"))).toBe("id with space");
  });

  it("rejects other destinations", () => {
    expect(parseThreadReferenceUrl("https://example.com")).toBeNull();
    expect(parseThreadReferenceUrl("src/Chat.tsx")).toBeNull();
    expect(parseThreadReferenceUrl("t3code://thread/")).toBeNull();
    expect(parseThreadReferenceUrl(undefined)).toBeNull();
  });
});

describe("collectThreadReferences", () => {
  it("collects references with source ranges", () => {
    const source = "[#Fix auth bug](t3code://thread/thr_abc123)";
    const text = `See ${source} for context`;

    expect(collectThreadReferences(text)).toEqual([
      {
        threadId: "thr_abc123",
        title: "Fix auth bug",
        source,
        start: 4,
        end: 4 + source.length,
      },
    ]);
  });

  it("collects a reference at the end of the text", () => {
    const references = collectThreadReferences("See [#T](t3code://thread/t1)");
    expect(references).toHaveLength(1);
    expect(references[0]?.threadId).toBe("t1");
  });

  it("unescapes label characters and decodes thread ids", () => {
    const source = "[#Fix \\[auth\\] bug](t3code://thread/id%20with%20space)";
    expect(collectThreadReferences(`${source} `)).toEqual([
      {
        threadId: "id with space",
        title: "Fix [auth] bug",
        source,
        start: 0,
        end: source.length,
      },
    ]);
  });

  it("ignores plain markdown links and file links", () => {
    expect(collectThreadReferences("[Chat.tsx](src/Chat.tsx) [docs](https://a.b)")).toEqual([]);
  });
});
