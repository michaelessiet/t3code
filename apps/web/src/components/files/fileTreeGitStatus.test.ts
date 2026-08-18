import type { VcsFileStatusEntry } from "@t3tools/contracts";
import { describe, expect, it } from "vite-plus/test";

import { buildFileTreeGitDecorations } from "./fileTreeGitStatus";

function statuses(...entries: ReadonlyArray<[string, VcsFileStatusEntry["status"]]>) {
  return entries.map(([path, status]) => ({ path, status, staged: false }));
}

function statusOf(
  decorations: ReturnType<typeof buildFileTreeGitDecorations>,
  path: string,
): string | undefined {
  return decorations.entries.find((entry) => entry.path === path)?.status;
}

describe("buildFileTreeGitDecorations", () => {
  it("returns nothing for a clean tree", () => {
    const decorations = buildFileTreeGitDecorations([], ["src/", "src/index.ts"]);
    expect(decorations.entries).toEqual([]);
    expect(decorations.conflictedPaths.size).toBe(0);
  });

  it("decorates a changed file and every ancestor directory", () => {
    const decorations = buildFileTreeGitDecorations(statuses(["src/app/index.ts", "modified"]), [
      "src/",
      "src/app/",
      "src/app/index.ts",
    ]);

    expect(statusOf(decorations, "src/app/index.ts")).toBe("modified");
    expect(statusOf(decorations, "src/app/")).toBe("modified");
    expect(statusOf(decorations, "src/")).toBe("modified");
  });

  it("gives a folder the most attention-worthy descendant status", () => {
    const decorations = buildFileTreeGitDecorations(
      statuses(["src/new.ts", "untracked"], ["src/edited.ts", "modified"]),
      ["src/", "src/new.ts", "src/edited.ts"],
    );

    expect(statusOf(decorations, "src/")).toBe("modified");
    expect(statusOf(decorations, "src/new.ts")).toBe("untracked");
    expect(statusOf(decorations, "src/edited.ts")).toBe("modified");
  });

  it("expands an untracked directory record over the rows inside it", () => {
    const decorations = buildFileTreeGitDecorations(statuses(["fresh/", "untracked"]), [
      "fresh/",
      "fresh/a.ts",
      "fresh/nested/",
      "fresh/nested/b.ts",
      "tracked.ts",
    ]);

    expect(statusOf(decorations, "fresh/")).toBe("untracked");
    expect(statusOf(decorations, "fresh/a.ts")).toBe("untracked");
    expect(statusOf(decorations, "fresh/nested/")).toBe("untracked");
    expect(statusOf(decorations, "fresh/nested/b.ts")).toBe("untracked");
    expect(statusOf(decorations, "tracked.ts")).toBeUndefined();
  });

  it("does not let an untracked directory overwrite a reported status", () => {
    // A directory can be untracked while git still reports a conflict inside a
    // sibling path that shares its prefix.
    const decorations = buildFileTreeGitDecorations(
      statuses(["fresh/", "untracked"], ["fresh/kept.ts", "modified"]),
      ["fresh/", "fresh/kept.ts", "fresh/other.ts"],
    );

    expect(statusOf(decorations, "fresh/kept.ts")).toBe("modified");
    expect(statusOf(decorations, "fresh/other.ts")).toBe("untracked");
  });

  it("reports conflicts as modified rows plus a conflicted path", () => {
    const decorations = buildFileTreeGitDecorations(statuses(["merge.txt", "conflicted"]), [
      "merge.txt",
    ]);

    expect(statusOf(decorations, "merge.txt")).toBe("modified");
    expect([...decorations.conflictedPaths]).toEqual(["merge.txt"]);
  });

  it("decorates ancestors of deleted files that no longer have a row", () => {
    const decorations = buildFileTreeGitDecorations(statuses(["src/gone.ts", "deleted"]), ["src/"]);

    expect(statusOf(decorations, "src/")).toBe("deleted");
    expect(statusOf(decorations, "src/gone.ts")).toBe("deleted");
  });

  it("maps renamed and added files to their own statuses", () => {
    const decorations = buildFileTreeGitDecorations(
      statuses(["moved.ts", "renamed"], ["created.ts", "added"]),
      ["moved.ts", "created.ts"],
    );

    expect(statusOf(decorations, "moved.ts")).toBe("renamed");
    expect(statusOf(decorations, "created.ts")).toBe("added");
  });
});
