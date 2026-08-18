import { Text } from "@codemirror/state";
import { describe, expect, it } from "vite-plus/test";

import {
  type GitDiffMarkerSpec,
  buildGitDiffOverviewRuns,
  computeGitDiffMarkerSpecs,
  gitDiffBaselineText,
} from "./gitDiffGutter";

function doc(contents: string): Text {
  return Text.of(contents.split("\n"));
}

function specs(head: string, buffer: string, index: string | null = null): GitDiffMarkerSpec[] {
  return computeGitDiffMarkerSpecs(
    gitDiffBaselineText(head),
    doc(buffer),
    index === null ? null : gitDiffBaselineText(index),
  );
}

describe("computeGitDiffMarkerSpecs", () => {
  it("returns no specs for identical documents", () => {
    expect(specs("a\nb\nc\n", "a\nb\nc\n")).toEqual([]);
    expect(specs("", "")).toEqual([]);
  });

  it("marks a changed line as modified", () => {
    expect(specs("a\nb\nc\n", "a\nB\nc\n")).toEqual([
      { line: 2, kind: "modified", staged: false, wedge: null },
    ]);
  });

  it("marks inserted lines as added", () => {
    expect(specs("a\nb\n", "a\nx\ny\nb\n")).toEqual([
      { line: 2, kind: "added", staged: false, wedge: null },
      { line: 3, kind: "added", staged: false, wedge: null },
    ]);
  });

  it("marks appended lines at the end of the file as added", () => {
    expect(specs("a\n", "a\nb\nc\n")).toEqual([
      { line: 2, kind: "added", staged: false, wedge: null },
      { line: 3, kind: "added", staged: false, wedge: null },
    ]);
  });

  it("splits a growing replacement into modified then added lines", () => {
    expect(specs("a\nb\nc\nd\n", "a\nX\nY\nZ\nd\n")).toEqual([
      { line: 2, kind: "modified", staged: false, wedge: null },
      { line: 3, kind: "modified", staged: false, wedge: null },
      { line: 4, kind: "added", staged: false, wedge: null },
    ]);
  });

  it("hangs a deletion wedge below a shrinking replacement", () => {
    expect(specs("a\nb\nc\nd\ne\n", "a\nX\nY\ne\n")).toEqual([
      { line: 2, kind: "modified", staged: false, wedge: null },
      { line: 3, kind: "modified", staged: false, wedge: "below" },
    ]);
  });

  it("marks a pure deletion with a wedge above the boundary line", () => {
    expect(specs("a\nb\nc\n", "a\nc\n")).toEqual([
      { line: 2, kind: null, staged: false, wedge: "above" },
    ]);
  });

  it("marks a deletion of leading lines above the first line", () => {
    expect(specs("a\nb\nc\n", "c\n")).toEqual([
      { line: 1, kind: null, staged: false, wedge: "above" },
    ]);
  });

  it("hangs a trailing deletion below the last line", () => {
    // A deletion at EOF owns the previous line's newline, so the chunk
    // includes the surviving line: it reads as modified with a wedge below.
    const result = specs("a\nb\nc", "a");
    expect(result).toEqual([{ line: 1, kind: "modified", staged: false, wedge: "below" }]);
  });

  it("treats a CRLF baseline against an LF buffer as unchanged", () => {
    expect(specs("a\r\nb\r\nc\r\n", "a\nb\nc\n")).toEqual([]);
  });

  it("marks new content against an empty baseline", () => {
    // An empty-but-committed file is one empty line, so the first content
    // line reads as modified and the rest as added.
    expect(specs("", "a\nb")).toEqual([
      { line: 1, kind: "modified", staged: false, wedge: null },
      { line: 2, kind: "added", staged: false, wedge: null },
    ]);
  });

  describe("staged classification", () => {
    it("marks a hunk staged when the buffer matches the index", () => {
      // Change is committed to the index; buffer has no further edits.
      expect(specs("a\nb\nc\n", "a\nB\nc\n", "a\nB\nc\n")).toEqual([
        { line: 2, kind: "modified", staged: true, wedge: null },
      ]);
    });

    it("marks a hunk unstaged when the index still matches HEAD", () => {
      expect(specs("a\nb\nc\n", "a\nB\nc\n", "a\nb\nc\n")).toEqual([
        { line: 2, kind: "modified", staged: false, wedge: null },
      ]);
    });

    it("classifies hunks independently", () => {
      // Line 2's edit is staged; line 4's edit is not.
      const result = specs("a\nb\nc\nd\ne\n", "a\nB\nc\nD\ne\n", "a\nB\nc\nd\ne\n");
      expect(result).toEqual([
        { line: 2, kind: "modified", staged: true, wedge: null },
        { line: 4, kind: "modified", staged: false, wedge: null },
      ]);
    });

    it("treats a missing index baseline as unstaged", () => {
      expect(specs("a\n", "A\n", null)).toEqual([
        { line: 1, kind: "modified", staged: false, wedge: null },
      ]);
    });
  });

  describe("overview ruler runs", () => {
    it("collapses contiguous same-kind lines into one run", () => {
      expect(buildGitDiffOverviewRuns(specs("a\nb\nc\n", "a\nX\nY\nZ\nc\n"))).toEqual([
        { kind: "modified", staged: false, firstLine: 2, lastLine: 2 },
        { kind: "added", staged: false, firstLine: 3, lastLine: 4 },
      ]);
    });

    it("keeps separate hunks as separate runs", () => {
      expect(buildGitDiffOverviewRuns(specs("a\nb\nc\nd\ne\n", "a\nB\nc\nD\ne\n"))).toEqual([
        { kind: "modified", staged: false, firstLine: 2, lastLine: 2 },
        { kind: "modified", staged: false, firstLine: 4, lastLine: 4 },
      ]);
    });

    it("does not merge runs that differ in staged state", () => {
      expect(
        buildGitDiffOverviewRuns([
          { line: 1, kind: "modified", staged: true, wedge: null },
          { line: 2, kind: "modified", staged: false, wedge: null },
        ]),
      ).toEqual([
        { kind: "modified", staged: true, firstLine: 1, lastLine: 1 },
        { kind: "modified", staged: false, firstLine: 2, lastLine: 2 },
      ]);
    });

    it("emits a deleted run for a pure-deletion boundary spec", () => {
      expect(buildGitDiffOverviewRuns(specs("a\nb\nc\n", "a\nc\n"))).toEqual([
        { kind: "deleted", staged: false, firstLine: 2, lastLine: 2 },
      ]);
    });

    it("keeps a bar hunk as one run even when it also carries a deletion wedge", () => {
      // Shrinking replacement: one modified line plus a wedge below it.
      expect(buildGitDiffOverviewRuns(specs("a\nb\nc\nd\n", "a\nX\nd\n"))).toEqual([
        { kind: "modified", staged: false, firstLine: 2, lastLine: 2 },
      ]);
    });

    it("produces no runs for a clean document", () => {
      expect(buildGitDiffOverviewRuns(specs("a\nb\n", "a\nb\n"))).toEqual([]);
    });
  });
});
