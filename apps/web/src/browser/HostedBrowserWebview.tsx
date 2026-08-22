"use client";

import type { PreviewViewportSetting, ScopedThreadRef } from "@t3tools/contracts";
import { useShallow } from "zustand/react/shallow";
import { useCallback, useEffect, useRef, useState } from "react";

import { previewBridge } from "~/components/preview/previewBridge";
import { usePreviewBridge } from "~/components/preview/usePreviewBridge";
import { cn } from "~/lib/utils";

import { resolveBrowserSurfacePanelRect, useBrowserSurfaceStore } from "./browserSurfaceStore";
import { browserViewportSettingKey, resolveBrowserViewportLayout } from "./browserViewportLayout";
import { BrowserDeviceToolbar } from "./BrowserDeviceToolbar";
import { BrowserViewportResizeHandles } from "./BrowserViewportResizeHandles";
import { acquireDesktopTab, type AcquiredDesktopTab } from "./desktopTabLifetime";
import { resolveHostedBrowserWebviewWrapperStyle } from "./hostedBrowserWebviewStyle";
import { usePreviewWebviewConfig } from "./previewWebviewConfigState";
import { useBrowserViewportResize } from "./useBrowserViewportResize";
import {
  INITIAL_WEBVIEW_CRASH_RECOVERY_STATE,
  planWebviewCrashRecovery,
  type WebviewCrashRecoveryState,
} from "./webviewCrashRecovery";

interface ElectronWebview extends HTMLElement {
  src: string;
  partition: string;
  preload?: string;
  webpreferences?: string;
  getWebContentsId: () => number;
  executeJavaScript: (code: string, userGesture?: boolean) => Promise<unknown>;
}

declare global {
  interface HTMLElementTagNameMap {
    webview: ElectronWebview;
  }
}

/**
 * Bounds-sync mode: when the desktop shell owns the preview webviews natively
 * (the Tauri shell — signalled by `previewBridge.setTabBounds` being present)
 * there is no `<webview>` tag to mount. A placeholder div takes its place and
 * this component reports the div's on-screen rect so the shell can position
 * its child webview over it. Constant for an app session.
 */
const boundsSyncMode = Boolean(previewBridge?.setTabBounds);

export function HostedBrowserWebview(props: {
  readonly threadRef: ScopedThreadRef;
  readonly tabId: string;
  readonly initialUrl: string | null;
  readonly viewport: PreviewViewportSetting;
  readonly zoomFactor: number;
}) {
  const { threadRef, tabId, initialUrl, viewport, zoomFactor } = props;
  const config = usePreviewWebviewConfig(threadRef.environmentId);
  const [initialSrc] = useState(() => initialUrl ?? "about:blank");
  const tabLeaseRef = useRef<AcquiredDesktopTab | null>(null);
  const wrapperRef = useRef<HTMLDivElement | null>(null);
  const webviewRef = useRef<ElectronWebview | null>(null);
  const crashRecoveryRef = useRef<WebviewCrashRecoveryState>(INITIAL_WEBVIEW_CRASH_RECOVERY_STATE);
  const [aspectRatioLocked, setAspectRatioLocked] = useState(false);
  const presentation = useBrowserSurfaceStore(
    useShallow((state) => {
      const current = state.byTabId[tabId];
      return {
        cornerRadius: current?.cornerRadius ?? 0,
        fitSourceContent: current?.fitSourceContent ?? false,
        fittedSourceContent: current?.fittedSourceContent ?? null,
        rect: resolveBrowserSurfacePanelRect(state.byTabId, tabId),
        visible: current?.visible ?? false,
      };
    }),
  );
  usePreviewBridge({ threadRef, tabId });

  useEffect(() => {
    crashRecoveryRef.current = INITIAL_WEBVIEW_CRASH_RECOVERY_STATE;
    const lease = acquireDesktopTab(tabId);
    tabLeaseRef.current = lease;
    return () => {
      if (tabLeaseRef.current === lease) tabLeaseRef.current = null;
      lease.release();
    };
  }, [tabId]);

  const [webviewGeneration, setWebviewGeneration] = useState(0);
  const [recoverySrc, setRecoverySrc] = useState(initialSrc);
  const latestUrlRef = useRef(initialUrl);

  useEffect(() => {
    latestUrlRef.current = initialUrl;
  }, [initialUrl]);

  const setWebviewRef = useCallback((node: HTMLElement | null) => {
    webviewRef.current = node as ElectronWebview | null;
    if (node && !node.hasAttribute("allowpopups")) node.setAttribute("allowpopups", "true");
  }, []);

  useEffect(() => {
    const webview = webviewRef.current;
    const bridge = previewBridge;
    // Bounds-sync shells own their webviews natively; there is no guest
    // WebContents to register.
    if (boundsSyncMode) return;
    if (!webview || !config || !bridge) return;
    let disposed = false;
    let recoveryTimeout: ReturnType<typeof setTimeout> | null = null;
    const register = () => {
      const lease = tabLeaseRef.current;
      if (!lease) return;
      void (async () => {
        try {
          // The main-process tab and the DOM webview are created by separate
          // effects. Wait for the former so registration cannot race and fail
          // with PreviewTabNotFoundError on a fast about:blank attachment.
          await lease.ready;
          if (disposed || webviewRef.current !== webview) return;
          const webContentsId = webview.getWebContentsId();
          if (Number.isInteger(webContentsId) && webContentsId > 0) {
            await bridge.registerWebview(tabId, webContentsId);
          }
        } catch {
          // did-attach/dom-ready will retry if the guest was not ready yet.
        }
      })();
    };
    const recoverGuest = () => {
      if (disposed || recoveryTimeout !== null) return;
      const recovery = planWebviewCrashRecovery(crashRecoveryRef.current, Date.now());
      if (!recovery) return;
      crashRecoveryRef.current = recovery.state;
      recoveryTimeout = setTimeout(() => {
        recoveryTimeout = null;
        if (!disposed) {
          setRecoverySrc(latestUrlRef.current ?? initialSrc);
          setWebviewGeneration((generation) => generation + 1);
        }
      }, recovery.delayMs);
    };
    webview.addEventListener("did-attach", register);
    webview.addEventListener("dom-ready", register);
    webview.addEventListener("render-process-gone", recoverGuest);
    register();
    return () => {
      disposed = true;
      if (recoveryTimeout !== null) clearTimeout(recoveryTimeout);
      webview.removeEventListener("did-attach", register);
      webview.removeEventListener("dom-ready", register);
      webview.removeEventListener("render-process-gone", recoverGuest);
    };
  }, [config, initialSrc, tabId, webviewGeneration]);

  const active = presentation.visible && presentation.rect !== null;
  const lastRect = presentation.rect;
  const normalizedZoomFactor = Number.isFinite(zoomFactor) && zoomFactor > 0 ? zoomFactor : 1;
  const viewportWidth = viewport._tag === "fill" ? null : viewport.width;
  const viewportHeight = viewport._tag === "fill" ? null : viewport.height;
  const viewportAspectRatio =
    viewportWidth === null || viewportHeight === null ? null : viewportWidth / viewportHeight;
  const lockedAspectRatio =
    aspectRatioLocked && viewportAspectRatio !== null ? viewportAspectRatio : null;
  const handleAspectRatioChange = useCallback((aspectRatio: number | null) => {
    setAspectRatioLocked(aspectRatio !== null);
  }, []);
  const hiddenSize =
    viewport._tag !== "fill"
      ? {
          width: viewport.width * normalizedZoomFactor,
          height: viewport.height * normalizedZoomFactor,
        }
      : { width: lastRect?.width ?? 1280, height: lastRect?.height ?? 800 };
  const containerSize = active && lastRect ? lastRect : hiddenSize;
  const deviceToolbarVisible = active && viewport._tag !== "fill" && !presentation.fitSourceContent;
  const {
    activeDrag,
    commitViewportChange,
    effectiveViewport,
    handleResizeKeyDown,
    handleResizePointerDown,
    layout: viewportLayout,
  } = useBrowserViewportResize({
    tabId,
    viewport,
    zoomFactor,
    containerSize,
    deviceToolbarVisible,
    aspectRatio: lockedAspectRatio,
  });
  const fittedSourceViewport =
    presentation.fitSourceContent && lastRect
      ? presentation.fittedSourceContent
        ? {
            _tag: "freeform" as const,
            width: Math.max(
              1,
              Math.round(
                presentation.fittedSourceContent.width /
                  presentation.fittedSourceContent.scale /
                  normalizedZoomFactor,
              ),
            ),
            height: Math.max(
              1,
              Math.round(
                presentation.fittedSourceContent.height /
                  presentation.fittedSourceContent.scale /
                  normalizedZoomFactor,
              ),
            ),
          }
        : {
            _tag: "freeform" as const,
            width: viewport._tag === "fill" ? 1280 : viewport.width,
            height: viewport._tag === "fill" ? 800 : viewport.height,
          }
      : null;
  const layout =
    fittedSourceViewport && lastRect
      ? resolveBrowserViewportLayout(lastRect, fittedSourceViewport, normalizedZoomFactor)
      : viewportLayout;

  const syncContentPresentation = useCallback(() => {
    const wrapper = wrapperRef.current;
    if (!wrapper) return;
    useBrowserSurfaceStore.getState().presentContent(tabId, {
      x: layout.viewportX,
      y: layout.viewportY,
      width: layout.viewportWidth,
      height: layout.viewportHeight,
      scale: layout.viewportScale,
      scrollLeft: wrapper.scrollLeft,
      scrollTop: wrapper.scrollTop,
    });
  }, [layout, tabId]);

  // Bounds-sync mode: report the placeholder's on-screen rect (clipped to the
  // wrapper, which the native child webview cannot be) to the shell.
  const lastSentBoundsRef = useRef("");
  const reportBounds = useCallback(() => {
    const bridge = previewBridge;
    if (!boundsSyncMode || !bridge?.setTabBounds) return;
    const element = webviewRef.current;
    const wrapper = wrapperRef.current;
    if (!element || !wrapper) return;
    const rect = element.getBoundingClientRect();
    const clip = wrapper.getBoundingClientRect();
    const left = Math.max(rect.left, clip.left);
    const top = Math.max(rect.top, clip.top);
    const width = Math.max(0, Math.min(rect.right, clip.right) - left);
    const height = Math.max(0, Math.min(rect.bottom, clip.bottom) - top);
    const bounds = {
      x: left,
      y: top,
      width,
      height,
      scale: layout.viewportScale,
      visible: active && width > 0 && height > 0,
    };
    const serialized = JSON.stringify(bounds);
    if (serialized === lastSentBoundsRef.current) return;
    lastSentBoundsRef.current = serialized;
    void bridge.setTabBounds(tabId, bounds).catch(() => {
      lastSentBoundsRef.current = "";
    });
  }, [active, layout.viewportScale, tabId]);

  useEffect(() => {
    const frameId = window.requestAnimationFrame(() => {
      syncContentPresentation();
      reportBounds();
    });
    return () => window.cancelAnimationFrame(frameId);
  }, [reportBounds, syncContentPresentation]);

  // Bounds-sync mode: drive the initial navigation (the Electron path gets it
  // from the `<webview src>` attribute) and clear the placement on unmount.
  useEffect(() => {
    const bridge = previewBridge;
    if (!boundsSyncMode || !bridge?.setTabBounds) return;
    const setTabBounds = bridge.setTabBounds.bind(bridge);
    const lease = tabLeaseRef.current;
    void (async () => {
      try {
        await lease?.ready;
        // Only navigate tabs the shell has never loaded; on remounts (layout
        // changes, tab switches) the shell already has the page.
        const status = await bridge.automation.status(tabId);
        const url = latestUrlRef.current;
        if (!status.url && url) await bridge.navigate(tabId, url);
      } catch {
        // The tab may have been closed mid-flight; state events will recover.
      }
    })();
    return () => {
      lastSentBoundsRef.current = "";
      void setTabBounds(tabId, null).catch(() => undefined);
    };
  }, [tabId]);

  useEffect(() => {
    const wrapper = wrapperRef.current;
    if (!wrapper) return;
    wrapper.scrollTo({ left: 0, top: 0 });
  }, [tabId, viewport._tag, viewportHeight, viewportWidth]);

  if (!config) return null;

  const wrapperStyle = resolveHostedBrowserWebviewWrapperStyle({
    active,
    cornerRadius: presentation.cornerRadius,
    rect: lastRect,
    hiddenSize,
  });

  return (
    <div
      ref={wrapperRef}
      className="fixed overflow-hidden bg-muted/35"
      style={{ ...wrapperStyle, overscrollBehavior: "contain" }}
      onScroll={() => {
        syncContentPresentation();
        reportBounds();
      }}
      data-preview-viewport={tabId}
    >
      <div className="relative" style={{ width: layout.canvasWidth, height: layout.canvasHeight }}>
        {deviceToolbarVisible && effectiveViewport._tag !== "fill" ? (
          <BrowserDeviceToolbar
            setting={effectiveViewport}
            width={Math.max(1, Math.round(containerSize.width))}
            aspectRatio={lockedAspectRatio}
            onAspectRatioChange={handleAspectRatioChange}
            onChange={commitViewportChange}
          />
        ) : null}
        {(() => {
          const sharedProps = {
            "data-preview-tab": tabId,
            "data-preview-viewport-mode": effectiveViewport._tag,
            "data-preview-viewport-key": browserViewportSettingKey(effectiveViewport),
            "data-preview-css-width": fittedSourceViewport
              ? fittedSourceViewport.width
              : effectiveViewport._tag === "fill"
                ? Math.max(1, Math.round(layout.viewportWidth / normalizedZoomFactor))
                : effectiveViewport.width,
            "data-preview-css-height": fittedSourceViewport
              ? fittedSourceViewport.height
              : effectiveViewport._tag === "fill"
                ? Math.max(1, Math.round(layout.viewportHeight / normalizedZoomFactor))
                : effectiveViewport.height,
            "aria-hidden": active ? undefined : true,
            className: cn(
              "absolute flex overflow-hidden bg-background",
              active && !layout.fillsPanel && "ring-1 ring-border/70 shadow-sm",
            ),
            style: {
              left: layout.viewportX,
              top: layout.viewportY,
              width: layout.viewportWidth / layout.viewportScale,
              height: layout.viewportHeight / layout.viewportScale,
              transform: layout.viewportScale < 1 ? `scale(${layout.viewportScale})` : undefined,
              transformOrigin: "top left" as const,
            },
          };
          // Bounds-sync shells render the guest natively above this window;
          // the div only stakes out the on-screen rect (and paints the
          // background while the native view attaches).
          return boundsSyncMode ? (
            <div ref={setWebviewRef} {...sharedProps} />
          ) : (
            <webview
              key={webviewGeneration}
              ref={setWebviewRef}
              src={webviewGeneration === 0 ? initialSrc : recoverySrc}
              partition={config.partition}
              webpreferences={config.webPreferences}
              {...(config.preloadUrl ? { preload: config.preloadUrl } : {})}
              {...sharedProps}
            />
          );
        })()}
        {active && effectiveViewport._tag !== "fill" && !fittedSourceViewport ? (
          <>
            <BrowserViewportResizeHandles
              layout={layout}
              activeDirection={activeDrag?.direction ?? null}
              onPointerDown={handleResizePointerDown}
              onKeyDown={handleResizeKeyDown}
            />
            {activeDrag ? (
              <div
                className="pointer-events-none absolute z-40 -translate-x-1/2 rounded-md border border-border/80 bg-background/95 px-2 py-1 text-[11px] font-medium tabular-nums text-foreground shadow-md backdrop-blur-sm"
                style={{
                  left: layout.viewportX + layout.viewportWidth / 2,
                  top: layout.viewportY + 10,
                }}
                aria-hidden="true"
              >
                {activeDrag.width} × {activeDrag.height}
              </div>
            ) : null}
          </>
        ) : null}
      </div>
    </div>
  );
}
