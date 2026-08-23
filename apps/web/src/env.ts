/**
 * True when running inside the Electron preload bridge, false in a regular browser.
 * The preload script sets window.nativeApi via contextBridge before any web-app
 * code executes, so this is reliable at module load time.
 */
export const isElectron =
  typeof window !== "undefined" &&
  (window.desktopBridge !== undefined || window.nativeApi !== undefined);

/**
 * True when running inside the Tauri desktop shell (apps/desktop-tauri). The
 * shell exposes the same window.desktopBridge contract as Electron, so
 * isElectron is also true there; use this to branch where the runtimes differ
 * (e.g. Clerk needs Electron's preload bridge, which Tauri cannot provide).
 * Tauri injects the __TAURI__ global from an initialization script
 * (withGlobalTauri), which runs before any web-app code.
 */
export const isTauri = typeof window !== "undefined" && "__TAURI__" in window;
