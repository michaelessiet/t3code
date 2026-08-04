import { scopeThreadRef } from "@t3tools/client-runtime/environment";
import { EnvironmentId, ThreadId } from "@t3tools/contracts";
import { beforeEach, describe, expect, it, vi } from "vite-plus/test";

import { openWorkspaceFilePrimaryAction } from "./openFileActions";
import { selectThreadRightPanelState, useRightPanelStore } from "./rightPanelStore";

const THREAD_REF = scopeThreadRef(
  EnvironmentId.make("environment-local"),
  ThreadId.make("thread-1"),
);

describe("openWorkspaceFilePrimaryAction", () => {
  beforeEach(() => {
    useRightPanelStore.setState({ byThreadKey: {} });
  });

  it("opens workspace files in the built-in editor by default", () => {
    const openInEditor = vi.fn();

    openWorkspaceFilePrimaryAction({
      threadRef: THREAD_REF,
      filePath: "src/main.ts",
      workspaceRoot: "/repo/project",
      openFilesInExternalEditor: false,
      openInEditor,
    });

    expect(
      selectThreadRightPanelState(useRightPanelStore.getState().byThreadKey, THREAD_REF),
    ).toMatchObject({
      isOpen: true,
      activeSurfaceId: "file:src/main.ts",
    });
    expect(openInEditor).not.toHaveBeenCalled();
  });

  it("carries the line suffix into the built-in editor reveal", () => {
    openWorkspaceFilePrimaryAction({
      threadRef: THREAD_REF,
      filePath: "src/main.ts:42:7",
      workspaceRoot: "/repo/project",
      openFilesInExternalEditor: false,
      openInEditor: vi.fn(),
    });

    const panelState = selectThreadRightPanelState(
      useRightPanelStore.getState().byThreadKey,
      THREAD_REF,
    );
    expect(panelState.activeSurfaceId).toBe("file:src/main.ts");
    expect(panelState.surfaces[0]).toMatchObject({ revealLine: 42 });
  });

  it("resolves absolute paths inside the workspace to a relative surface", () => {
    openWorkspaceFilePrimaryAction({
      threadRef: THREAD_REF,
      filePath: "/repo/project/src/main.ts",
      workspaceRoot: "/repo/project",
      openFilesInExternalEditor: false,
      openInEditor: vi.fn(),
    });

    expect(
      selectThreadRightPanelState(useRightPanelStore.getState().byThreadKey, THREAD_REF)
        .activeSurfaceId,
    ).toBe("file:src/main.ts");
  });

  it("opens externally when the user opted into an external editor", () => {
    const openInEditor = vi.fn();

    openWorkspaceFilePrimaryAction({
      threadRef: THREAD_REF,
      filePath: "src/main.ts",
      workspaceRoot: "/repo/project",
      openFilesInExternalEditor: true,
      openInEditor,
    });

    expect(
      selectThreadRightPanelState(useRightPanelStore.getState().byThreadKey, THREAD_REF),
    ).toMatchObject({ isOpen: false });
    expect(openInEditor).toHaveBeenCalledWith("/repo/project/src/main.ts");
  });

  it("falls back to the external editor for files outside the workspace", () => {
    const openInEditor = vi.fn();

    openWorkspaceFilePrimaryAction({
      threadRef: THREAD_REF,
      filePath: "/etc/hosts",
      workspaceRoot: "/repo/project",
      openFilesInExternalEditor: false,
      openInEditor,
    });

    expect(openInEditor).toHaveBeenCalledWith("/etc/hosts");
  });

  it("falls back to the external editor without thread context", () => {
    const openInEditor = vi.fn();

    openWorkspaceFilePrimaryAction({
      threadRef: null,
      filePath: "src/main.ts",
      workspaceRoot: "/repo/project",
      openFilesInExternalEditor: false,
      openInEditor,
    });

    expect(openInEditor).toHaveBeenCalledWith("/repo/project/src/main.ts");
  });
});
