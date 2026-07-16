import { describe, expect, it } from "vite-plus/test";

import { completionAnchor, completionKindToType } from "./lspBridge";

describe("completionAnchor", () => {
  it("anchors at the start of the trailing word only", () => {
    // "foo.ba|" → anchor must cover "ba", not "foo.ba" (a from that spans
    // the receiver makes CM filter out every member suggestion).
    expect(completionAnchor("foo.ba", 6, false)).toEqual({ from: 4 });
  });

  it("activates at the cursor right after member access", () => {
    expect(completionAnchor("foo.", 4, false)).toEqual({ from: 4 });
  });

  it("activates inside import-path strings", () => {
    expect(completionAnchor('import { x } from "', 19, false)).toEqual({ from: 19 });
    expect(completionAnchor('import { x } from "effect/', 26, false)).toEqual({ from: 26 });
  });

  it("does not activate mid-whitespace unless explicit", () => {
    expect(completionAnchor("const a = ", 10, false)).toBe(null);
    expect(completionAnchor("const a = ", 10, true)).toEqual({ from: 10 });
  });
});

describe("completionKindToType", () => {
  it("maps common LSP kinds", () => {
    expect(completionKindToType(2)).toBe("function");
    expect(completionKindToType(5)).toBe("property");
    expect(completionKindToType(7)).toBe("class");
    expect(completionKindToType(undefined)).toBe(undefined);
  });
});
