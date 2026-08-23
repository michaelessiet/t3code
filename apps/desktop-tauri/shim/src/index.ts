/**
 * `window.desktopBridge` shim for the Tauri shell.
 *
 * The Electron desktop app exposes this bridge from a preload script backed
 * by ipcRenderer; here it is an initialization script backed by Tauri
 * commands and events. Three bridge methods are synchronous in the contract
 * (`getAppBranding`, `getLocalEnvironmentBootstraps`,
 * `getWindowFullscreenState`), so the Rust shell prepends a
 * `__VITRE_SEED__` JSON blob to this script (generated after the
 * backend is ready, before the window exists) and keeps the snapshot fresh
 * through Tauri events.
 *
 * M1 scope: SSH, WSL, server exposure, and updates are inert stubs shaped to
 * keep the web state stores happy.
 *
 * M2 adds `preview`, backed by src-tauri/src/preview.rs: shell-owned child
 * webviews (Window::add_child) instead of renderer `<webview>` tags. The
 * presence of the optional `setTabBounds` method is what switches the web
 * app's HostedBrowserWebview into bounds-sync mode.
 */
import type {
  ClientSettings,
  ContextMenuItem,
  DesktopAppBranding,
  DesktopBridge,
  DesktopEnvironmentBootstrap,
  DesktopPreviewBridge,
  DesktopPreviewColorScheme,
  DesktopPreviewPointerEvent,
  DesktopPreviewRecordingArtifact,
  DesktopPreviewRecordingFrame,
  DesktopPreviewScreenshotArtifact,
  DesktopPreviewTabBounds,
  DesktopPreviewTabState,
  DesktopPreviewWebviewConfig,
  DesktopRuntimeArch,
  DesktopServerExposureState,
  DesktopTheme,
  DesktopUpdateState,
  DesktopWslState,
  EnvironmentId,
  PickFolderOptions,
  PreviewAnnotationPayload,
  PreviewAutomationSnapshot,
  PreviewAutomationStatus,
} from "@t3tools/contracts";

interface TauriSeed {
  readonly branding: DesktopAppBranding;
  readonly bootstraps: readonly DesktopEnvironmentBootstrap[];
  readonly appVersion: string;
  readonly arch: DesktopRuntimeArch;
}

interface TauriEventApi {
  listen: (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>;
}

interface TauriGlobal {
  core: { invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown> };
  event: TauriEventApi;
}

declare global {
  interface Window {
    desktopBridge?: DesktopBridge;
    __VITRE_SEED__?: TauriSeed;
    __TAURI__?: TauriGlobal;
  }
}

const seed: TauriSeed = (() => {
  const injected = window.__VITRE_SEED__;
  if (!injected) {
    throw new Error("vitre shim loaded without a __VITRE_SEED__ blob");
  }
  return injected;
})();

const snapshot = {
  bootstraps: seed.bootstraps,
  fullscreen: false,
};

// Tauri injects its API bundle as an initialization script too; guard against
// script ordering by polling for the global instead of assuming it exists.
let tauriPromise: Promise<TauriGlobal> | null = null;
function tauriReady(): Promise<TauriGlobal> {
  tauriPromise ??= new Promise((resolve, reject) => {
    const startedAt = Date.now();
    const poll = () => {
      const tauri = window.__TAURI__;
      if (tauri) {
        resolve(tauri);
        return;
      }
      if (Date.now() - startedAt > 10_000) {
        reject(new Error("Tauri API global never appeared (withGlobalTauri disabled?)"));
        return;
      }
      setTimeout(poll, 10);
    };
    poll();
  });
  return tauriPromise;
}

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const tauri = await tauriReady();
  return (await tauri.core.invoke(command, args)) as T;
}

type Listener<T> = (value: T) => void;

function makeEventFanout<T>(eventName: string, onPayload?: (payload: T) => void) {
  const listeners = new Set<Listener<T>>();
  void tauriReady().then((tauri) =>
    tauri.event.listen(eventName, (event) => {
      const payload = event.payload as T;
      onPayload?.(payload);
      for (const listener of listeners) {
        listener(payload);
      }
    }),
  );
  return (listener: Listener<T>): (() => void) => {
    listeners.add(listener);
    return () => listeners.delete(listener);
  };
}

const subscribeBootstrapsChanged = makeEventFanout<readonly DesktopEnvironmentBootstrap[]>(
  "t3code://bootstraps-changed",
  (bootstraps) => {
    snapshot.bootstraps = bootstraps;
  },
);
const subscribeFullscreenChanged = makeEventFanout<boolean>(
  "t3code://fullscreen-changed",
  (fullscreen) => {
    snapshot.fullscreen = fullscreen;
  },
);
const subscribeMenuAction = makeEventFanout<string>("t3code://menu-action");

// Same exchange the Electron main process performs in
// DesktopLocalEnvironmentAuth (bootstrapRemoteBearerSession → POST
// /oauth/token), executed from the renderer. The dev CORS allowlist admits
// this shell's origin via T3CODE_DEV_ALLOWED_ORIGINS (set in backend.rs);
// packaged mode falls back to the wildcard origin.
let bearerTokenPromise: Promise<string> | null = null;
function getLocalEnvironmentBearerToken(): Promise<string> {
  bearerTokenPromise ??= (async () => {
    const primary = snapshot.bootstraps.find((bootstrap) => bootstrap.id === "primary");
    if (!primary?.httpBaseUrl || !primary.bootstrapToken) {
      throw new Error("Local backend is not configured.");
    }
    // AuthTokenExchangeRequest is asFormUrlEncoded() (contracts/src/auth.ts).
    const response = await fetch(new URL("/oauth/token", primary.httpBaseUrl), {
      method: "POST",
      body: new URLSearchParams({
        grant_type: "urn:ietf:params:oauth:grant-type:token-exchange",
        subject_token: primary.bootstrapToken,
        subject_token_type: "urn:t3:params:oauth:token-type:environment-bootstrap",
        requested_token_type: "urn:ietf:params:oauth:token-type:access_token",
        client_label: "Vitre Desktop",
        client_device_type: "desktop",
      }),
    });
    if (!response.ok) {
      throw new Error(`Local bearer session bootstrap failed (${response.status})`);
    }
    const result = (await response.json()) as { access_token: string };
    return result.access_token;
  })().catch((error: unknown) => {
    bearerTokenPromise = null;
    throw error;
  });
  return bearerTokenPromise;
}

function makeUpdateState(): DesktopUpdateState {
  return {
    enabled: false,
    status: "disabled",
    channel: "latest",
    currentVersion: seed.appVersion,
    hostArch: seed.arch,
    appArch: seed.arch,
    runningUnderArm64Translation: false,
    availableVersion: null,
    downloadedVersion: null,
    releaseNotes: [],
    downloadPercent: null,
    checkedAt: null,
    message: null,
    errorContext: null,
    canRetry: false,
  };
}

function makeWslState(): DesktopWslState {
  return {
    enabled: false,
    distro: null,
    available: false,
    wslOnly: false,
    distros: [],
    preflightError: null,
  };
}

function makeServerExposureState(): DesktopServerExposureState {
  return {
    mode: "local-only",
    endpointUrl: null,
    advertisedHost: null,
    tailscaleServeEnabled: false,
    tailscaleServePort: 443,
  };
}

function sshUnavailable(): Promise<never> {
  return Promise.reject(new Error("SSH environments are not available in the Tauri shell yet."));
}

// ---------------------------------------------------------------------------
// Preview bridge (M2): shell-owned child webviews driven by preview.rs.
// Presence of `setTabBounds` switches HostedBrowserWebview into bounds-sync
// mode (no <webview> tag; a placeholder div reports its on-screen rect).

const subscribePreviewState = makeEventFanout<{
  tabId: string;
  state: DesktopPreviewTabState;
}>("t3code://preview-state");
const subscribePreviewPointer = makeEventFanout<DesktopPreviewPointerEvent>(
  "t3code://preview-pointer",
);
const subscribePreviewFrame =
  makeEventFanout<DesktopPreviewRecordingFrame>("t3code://preview-frame");

function bytesToBase64(data: Uint8Array): string {
  let binary = "";
  const chunkSize = 0x8000;
  for (let index = 0; index < data.length; index += chunkSize) {
    binary += String.fromCharCode(...data.subarray(index, index + chunkSize));
  }
  return btoa(binary);
}

function previewUnsupported(feature: string): Promise<never> {
  return Promise.reject(new Error(`${feature} is not available in the Tauri shell yet.`));
}

const preview: DesktopPreviewBridge = {
  createTab: (tabId) => invoke<void>("preview_create_tab", { tabId }),
  closeTab: (tabId) => invoke<void>("preview_close_tab", { tabId }),
  registerWebview: (tabId, webContentsId) =>
    invoke<void>("preview_register_webview", { tabId, webContentsId }),
  setTabBounds: (tabId, bounds: DesktopPreviewTabBounds | null) =>
    invoke<void>("preview_set_tab_bounds", { tabId, bounds }),
  navigate: (tabId, url) => invoke<void>("preview_navigate", { tabId, url }),
  goBack: (tabId) => invoke<void>("preview_go_back", { tabId }),
  goForward: (tabId) => invoke<void>("preview_go_forward", { tabId }),
  refresh: (tabId) => invoke<void>("preview_refresh", { tabId }),
  zoomIn: (tabId) => invoke<void>("preview_zoom_in", { tabId }),
  zoomOut: (tabId) => invoke<void>("preview_zoom_out", { tabId }),
  resetZoom: (tabId) => invoke<void>("preview_reset_zoom", { tabId }),
  hardReload: (tabId) => invoke<void>("preview_hard_reload", { tabId }),
  setColorScheme: (tabId, colorScheme: DesktopPreviewColorScheme) =>
    invoke<void>("preview_set_color_scheme", { tabId, colorScheme }),
  openDevTools: (tabId) => invoke<void>("preview_open_devtools", { tabId }),
  clearCookies: () => invoke<void>("preview_clear_cookies"),
  clearCache: () => invoke<void>("preview_clear_cache"),
  getPreviewConfig: (environmentId: EnvironmentId) =>
    invoke<DesktopPreviewWebviewConfig>("preview_get_config", { environmentId }),
  setAnnotationTheme: (theme) => invoke<void>("preview_set_annotation_theme", { theme }),
  // M3: the annotation studio (apps/desktop PickPreload.ts, bundled unchanged
  // into the injected preview runtime) is armed by the shell over the picker
  // dispatch channel; the command resolves when the guest submits or cancels.
  pickElement: (tabId) =>
    invoke<PreviewAnnotationPayload | null>("preview_pick_element", { tabId }),
  cancelPickElement: (tabId) => invoke<void>("preview_cancel_pick_element", { tabId }),
  captureScreenshot: (tabId) =>
    invoke<DesktopPreviewScreenshotArtifact>("preview_capture_screenshot", { tabId }),
  revealArtifact: (path) => invoke<void>("preview_reveal_artifact", { path }),
  copyArtifactToClipboard: () => previewUnsupported("Copying artifacts to the clipboard"),
  // M3: per-tab always-on-top window fed by the shell's shared frame loop.
  pictureInPicture: {
    open: (tabId) => invoke<void>("preview_pip_open", { tabId }),
    close: (tabId) => invoke<void>("preview_pip_close", { tabId }),
  },
  recording: {
    startScreencast: (tabId) => invoke<void>("preview_recording_start", { tabId }),
    stopScreencast: (tabId) => invoke<void>("preview_recording_stop", { tabId }),
    save: (tabId, mimeType, data) =>
      invoke<DesktopPreviewRecordingArtifact>("preview_recording_save", {
        tabId,
        mimeType,
        dataBase64: bytesToBase64(data),
      }),
    onFrame: (listener) => subscribePreviewFrame(listener),
  },
  automation: {
    status: (tabId) => invoke<PreviewAutomationStatus>("preview_automation_status", { tabId }),
    snapshot: (tabId) =>
      invoke<PreviewAutomationSnapshot>("preview_automation_snapshot", { tabId }),
    click: (tabId, input) => invoke<void>("preview_automation_click", { tabId, input }),
    type: (tabId, input) => invoke<void>("preview_automation_type", { tabId, input }),
    press: (tabId, input) => invoke<void>("preview_automation_press", { tabId, input }),
    scroll: (tabId, input) => invoke<void>("preview_automation_scroll", { tabId, input }),
    evaluate: (tabId, input) => invoke<unknown>("preview_automation_evaluate", { tabId, input }),
    waitFor: (tabId, input) => invoke<void>("preview_automation_wait_for", { tabId, input }),
  },
  onStateChange: (listener) => subscribePreviewState(({ tabId, state }) => listener(tabId, state)),
  onPointerEvent: (listener) => subscribePreviewPointer(listener),
};

const bridge: DesktopBridge = {
  getAppBranding: () => seed.branding,
  getLocalEnvironmentBootstraps: () => snapshot.bootstraps,
  onLocalEnvironmentBootstrapsChanged: (listener) => subscribeBootstrapsChanged(() => listener()),
  getLocalEnvironmentBearerToken,

  getClientSettings: () => invoke<ClientSettings | null>("get_client_settings"),
  setClientSettings: (settings) => invoke<void>("set_client_settings", { settings }),
  getConnectionCatalog: () => invoke<string | null>("get_connection_catalog"),
  setConnectionCatalog: (catalog) => invoke<boolean>("set_connection_catalog", { catalog }),
  clearConnectionCatalog: () => invoke<void>("clear_connection_catalog"),

  discoverSshHosts: () => Promise.resolve([]),
  ensureSshEnvironment: () => sshUnavailable(),
  disconnectSshEnvironment: () => sshUnavailable(),
  fetchSshEnvironmentDescriptor: () => sshUnavailable(),
  bootstrapSshBearerSession: () => sshUnavailable(),
  fetchSshSessionState: () => sshUnavailable(),
  issueSshWebSocketTicket: () => sshUnavailable(),
  onSshPasswordPrompt: () => () => {},
  resolveSshPasswordPrompt: () => Promise.resolve(),

  getServerExposureState: () => Promise.resolve(makeServerExposureState()),
  setServerExposureMode: () => Promise.resolve(makeServerExposureState()),
  setTailscaleServeEnabled: () => Promise.resolve(makeServerExposureState()),
  getAdvertisedEndpoints: () => Promise.resolve([]),

  getWslState: () => Promise.resolve(makeWslState()),
  setWslBackendEnabled: () => Promise.resolve(makeWslState()),
  setWslDistro: () => Promise.resolve(makeWslState()),
  setWslOnly: () => Promise.resolve(makeWslState()),

  pickFolder: (options?: PickFolderOptions) =>
    invoke<string | null>("pick_folder", { options: options ?? null }),
  confirm: (message) => invoke<boolean>("confirm_dialog", { message }),
  setTheme: (theme: DesktopTheme) => invoke<void>("set_theme", { theme }),
  showContextMenu: async <T extends string>(
    items: readonly ContextMenuItem<T>[],
    position?: { x: number; y: number },
  ) => invoke<T | null>("show_context_menu", { items, position: position ?? null }),
  openExternal: async (url) => {
    try {
      return await invoke<boolean>("open_external", { url });
    } catch {
      return false;
    }
  },
  onMenuAction: (listener) => subscribeMenuAction(listener),
  getWindowFullscreenState: () => snapshot.fullscreen,
  onWindowFullscreenStateChange: (listener) => subscribeFullscreenChanged(listener),

  preview,

  getUpdateState: () => Promise.resolve(makeUpdateState()),
  setUpdateChannel: () => Promise.resolve(makeUpdateState()),
  checkForUpdate: () => Promise.resolve({ checked: false, state: makeUpdateState() }),
  downloadUpdate: () =>
    Promise.resolve({ accepted: false, completed: false, state: makeUpdateState() }),
  installUpdate: () =>
    Promise.resolve({ accepted: false, completed: false, state: makeUpdateState() }),
  onUpdateState: () => () => {},
};

window.desktopBridge = bridge;
