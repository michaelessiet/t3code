import { describe, expect, it } from "vite-plus/test";
import { ProjectWriteFileError } from "@t3tools/contracts";
import * as Cause from "effect/Cause";
import { AsyncResult } from "effect/unstable/reactivity";

import { detectsExternalConflict, isStaleRevisionWriteFailure } from "./fileBufferConflict";

describe("detectsExternalConflict", () => {
  it("flags a dirty buffer whose disk revision moved", () => {
    expect(detectsExternalConflict({ dirty: true, baseRevision: "1:a", diskRevision: "2:b" })).toBe(
      true,
    );
  });

  it("ignores clean buffers", () => {
    expect(
      detectsExternalConflict({ dirty: false, baseRevision: "1:a", diskRevision: "2:b" }),
    ).toBe(false);
  });

  it("ignores matching revisions", () => {
    expect(detectsExternalConflict({ dirty: true, baseRevision: "1:a", diskRevision: "1:a" })).toBe(
      false,
    );
  });

  it("never conflicts when a revision is unknown", () => {
    expect(detectsExternalConflict({ dirty: true, baseRevision: null, diskRevision: "2:b" })).toBe(
      false,
    );
    expect(
      detectsExternalConflict({ dirty: true, baseRevision: "1:a", diskRevision: undefined }),
    ).toBe(false);
  });
});

describe("isStaleRevisionWriteFailure", () => {
  it("matches stale_revision write failures", () => {
    const failure = AsyncResult.failure(
      Cause.fail(
        new ProjectWriteFileError({
          cwd: "/repo",
          relativePath: "src/index.ts",
          failure: "stale_revision",
        }),
      ),
    );
    expect(isStaleRevisionWriteFailure(failure)).toBe(true);
  });

  it("ignores other write failures", () => {
    const failure = AsyncResult.failure(
      Cause.fail(
        new ProjectWriteFileError({
          cwd: "/repo",
          relativePath: "src/index.ts",
          failure: "operation_failed",
        }),
      ),
    );
    expect(isStaleRevisionWriteFailure(failure)).toBe(false);
    expect(isStaleRevisionWriteFailure(AsyncResult.success(undefined))).toBe(false);
  });
});
