import { EnvironmentId, ProjectWriteFileError } from "@t3tools/contracts";
import * as Cause from "effect/Cause";
import { AsyncResult } from "effect/unstable/reactivity";
import { afterEach, describe, expect, it, vi } from "vite-plus/test";

import { fileContentRevision } from "./fileContentRevision";
import {
  getFileBaseRevision,
  isSelfWrittenRevision,
  persistUnsavedBuffer,
  recordSelfWrittenRevision,
  setFileBaseRevision,
  type WriteProjectFile,
} from "./fileSaveState";
import {
  clearProjectFileQueryData,
  getUnsavedProjectFileBuffer,
  setProjectFileQueryData,
} from "./projectFilesQueryState";

const environmentId = EnvironmentId.make("environment-file-save-state-test");
const cwd = "/repo";
const relativePath = "src/index.ts";

describe("file save state", () => {
  afterEach(() => {
    setFileBaseRevision(environmentId, cwd, relativePath, null);
    clearProjectFileQueryData(environmentId, cwd, relativePath);
    vi.unstubAllGlobals();
  });

  it("stores base revisions per file", () => {
    expect(getFileBaseRevision(environmentId, cwd, relativePath)).toBeNull();
    setFileBaseRevision(environmentId, cwd, relativePath, "1:a");
    expect(getFileBaseRevision(environmentId, cwd, relativePath)).toBe("1:a");
    expect(getFileBaseRevision(environmentId, cwd, "src/other.ts")).toBeNull();
    setFileBaseRevision(environmentId, cwd, relativePath, null);
    expect(getFileBaseRevision(environmentId, cwd, relativePath)).toBeNull();
  });

  it("recognizes self-written revisions per file", () => {
    recordSelfWrittenRevision(environmentId, cwd, relativePath, "2:b");
    expect(isSelfWrittenRevision(environmentId, cwd, relativePath, "2:b")).toBe(true);
    expect(isSelfWrittenRevision(environmentId, cwd, relativePath, "3:c")).toBe(false);
    expect(isSelfWrittenRevision(environmentId, cwd, "src/other.ts", "2:b")).toBe(false);
  });

  it("persists an unsaved buffer with the base-revision guard", async () => {
    vi.stubGlobal("window", {});
    setProjectFileQueryData(environmentId, cwd, relativePath, "edited contents");
    setFileBaseRevision(environmentId, cwd, relativePath, "1:a");
    const writtenRevision = fileContentRevision("edited contents");
    const writeFile = vi
      .fn<WriteProjectFile>()
      .mockResolvedValue(AsyncResult.success({ relativePath, revision: writtenRevision }));

    const outcome = await persistUnsavedBuffer(environmentId, cwd, relativePath, writeFile);

    expect(outcome).toBe("saved");
    expect(writeFile).toHaveBeenCalledWith({
      environmentId,
      input: { cwd, relativePath, contents: "edited contents", baseRevision: "1:a" },
    });
    expect(getFileBaseRevision(environmentId, cwd, relativePath)).toBe(writtenRevision);
    expect(isSelfWrittenRevision(environmentId, cwd, relativePath, writtenRevision)).toBe(true);
    expect(getUnsavedProjectFileBuffer(environmentId, cwd, relativePath)).toBeNull();
  });

  it("reports a stale write without confirming the buffer", async () => {
    vi.stubGlobal("window", {});
    setProjectFileQueryData(environmentId, cwd, relativePath, "edited contents");
    setFileBaseRevision(environmentId, cwd, relativePath, "1:a");
    const writeFile = vi
      .fn<WriteProjectFile>()
      .mockResolvedValue(
        AsyncResult.failure(
          Cause.fail(new ProjectWriteFileError({ cwd, relativePath, failure: "stale_revision" })),
        ),
      );

    const outcome = await persistUnsavedBuffer(environmentId, cwd, relativePath, writeFile);

    expect(outcome).toBe("stale");
    expect(getUnsavedProjectFileBuffer(environmentId, cwd, relativePath)).toBe("edited contents");
  });

  it("treats a missing buffer as already clean", async () => {
    const writeFile = vi.fn<WriteProjectFile>();
    const outcome = await persistUnsavedBuffer(environmentId, cwd, relativePath, writeFile);
    expect(outcome).toBe("clean");
    expect(writeFile).not.toHaveBeenCalled();
  });
});
