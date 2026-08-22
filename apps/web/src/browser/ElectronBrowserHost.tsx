"use client";

import { parseScopedThreadKey } from "@t3tools/client-runtime/environment";
import { FILL_PREVIEW_VIEWPORT } from "@t3tools/contracts";
import { useEffect, useMemo } from "react";
import { useShallow } from "zustand/react/shallow";

import { isElectron } from "~/env";
import { useTheme } from "~/hooks/useTheme";
import { useActivePreviewSessions } from "~/previewStateStore";

import { readPreviewAnnotationTheme } from "./annotationTheme";
import { useBrowserPointerStore } from "./browserPointerStore";
import { useActiveBrowserRecordingTabIds } from "./browserRecording";
import { useBrowserSurfaceStore } from "./browserSurfaceStore";
import { HostedBrowserWebview } from "./HostedBrowserWebview";
import { selectLivePreviewGuests } from "./previewGuestBudget";

export function ElectronBrowserHost() {
  const { resolvedTheme } = useTheme();
  const previewByThreadKey = useActivePreviewSessions();
  const sessions = useMemo(
    () =>
      Object.entries(previewByThreadKey).flatMap(([threadKey, previewState]) => {
        const threadRef = parseScopedThreadKey(threadKey);
        return threadRef
          ? Object.values(previewState.sessions).map((snapshot) => {
              const overlay = previewState.desktopByTabId[snapshot.tabId];
              return {
                threadRef,
                snapshot,
                zoomFactor: overlay?.zoomFactor ?? 1,
                controller: overlay?.controller ?? ("none" as const),
                pictureInPicture: overlay?.pictureInPicture ?? false,
              };
            })
          : [];
      }),
    [previewByThreadKey],
  );
  // Narrow subscription: presentContent updates byTabId every frame while
  // scrolling, so subscribe to the visible flags only.
  const visibleTabIds = useBrowserSurfaceStore(
    useShallow((state) => {
      const visible: Record<string, true> = {};
      for (const [tabId, surface] of Object.entries(state.byTabId)) {
        if (surface.visible) visible[tabId] = true;
      }
      return visible;
    }),
  );
  const recordingTabIds = useActiveBrowserRecordingTabIds();
  const liveTabIds = useMemo(
    () =>
      selectLivePreviewGuests(
        sessions.map(({ snapshot, controller, pictureInPicture }) => ({
          tabId: snapshot.tabId,
          updatedAt: snapshot.updatedAt,
          visible: visibleTabIds[snapshot.tabId] === true,
          controller,
          pictureInPicture,
          recording: recordingTabIds.has(snapshot.tabId),
        })),
      ),
    [recordingTabIds, sessions, visibleTabIds],
  );

  useEffect(() => {
    const preview = window.desktopBridge?.preview;
    if (!preview) return;

    let lastSerializedTheme = "";
    const syncTheme = () => {
      const theme = readPreviewAnnotationTheme();
      const serializedTheme = JSON.stringify(theme);
      if (serializedTheme === lastSerializedTheme) return;
      lastSerializedTheme = serializedTheme;
      void preview.setAnnotationTheme(theme).catch(() => {
        lastSerializedTheme = "";
      });
    };
    const frameId = window.requestAnimationFrame(syncTheme);
    const observer = new MutationObserver(syncTheme);
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class", "style"],
    });
    const headObserver = new MutationObserver(syncTheme);
    headObserver.observe(document.head, {
      childList: true,
      subtree: true,
      characterData: true,
    });
    return () => {
      window.cancelAnimationFrame(frameId);
      observer.disconnect();
      headObserver.disconnect();
    };
  }, [resolvedTheme]);

  useEffect(() => {
    const preview = window.desktopBridge?.preview;
    if (!preview) return;
    return preview.onPointerEvent((event) => {
      useBrowserPointerStore.getState().apply(event);
    });
  }, []);

  if (!isElectron) return null;
  return (
    <div className="contents" data-electron-browser-host>
      {sessions
        .filter(({ snapshot }) => liveTabIds.has(snapshot.tabId))
        .map(({ threadRef, snapshot, zoomFactor }) => {
          const url = snapshot.navStatus._tag === "Idle" ? null : snapshot.navStatus.url;
          return (
            <HostedBrowserWebview
              key={snapshot.tabId}
              threadRef={threadRef}
              tabId={snapshot.tabId}
              initialUrl={url}
              viewport={snapshot.viewport ?? FILL_PREVIEW_VIEWPORT}
              zoomFactor={zoomFactor}
            />
          );
        })}
    </div>
  );
}
