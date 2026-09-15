import { describe, expect, it } from "@effect/vitest";

import { isProjectIdDirectoryName } from "./graphStoreKey.ts";

describe("native graph project directories", () => {
  it("accepts existing Vitre timestamp IDs and web UUIDs", () => {
    expect(isProjectIdDirectoryName("vitre-project-1789398925123456789")).toBe(true);
    expect(isProjectIdDirectoryName("6f1f9a4c-4f77-4b9e-9f3a-1d2e3f4a5b6c")).toBe(true);
  });

  it.each([
    "vitre-project-1",
    "vitre-project-1789398925123456789/..",
    "../vitre-project-1789398925123456789",
    "vitre-project-1789398925123456789\\child",
    "vitre-project-1789398925123456789\u0000",
    "vitre-project-1789398925123456789\n",
    "project-1789398925123456789",
    "vitre-project-" + "1".repeat(100),
  ])("rejects non-client IDs and path manipulation: %j", (id) => {
    expect(isProjectIdDirectoryName(id)).toBe(false);
  });
});
