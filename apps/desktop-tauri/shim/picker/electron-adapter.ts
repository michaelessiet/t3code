/**
 * Bundle-time replacement for the `electron` module when building the preview
 * element picker for the Tauri shell (scripts/build-preview-runtime.mjs
 * aliases `electron` to this file). apps/desktop's PickPreload.ts is bundled
 * UNCHANGED — the whole annotation studio (closed-shadow-DOM overlay, tools,
 * react-grab element capture, style diffing) is shared source; only the
 * transport at the edges differs:
 *
 * - guest→shell: `ipcRenderer.send` posts to the `t3preview://` protocol via
 *   the `__t3pPost` helper installed by shim/preview-runtime.src.js, which is
 *   concatenated ahead of this bundle in the same initialization script.
 * - shell→guest: preview.rs evals `__t3pPickerDispatch(channel, ...args)`,
 *   which fans out to the `ipcRenderer.on` listeners registered here.
 */

type PickerListener = (event: unknown, ...args: unknown[]) => void;

declare global {
  interface Window {
    __t3pPost?: (payload: Record<string, unknown>) => void;
    __t3pPickerDispatch?: (channel: string, ...args: unknown[]) => void;
  }
}

const listeners = new Map<string, Set<PickerListener>>();

window.__t3pPickerDispatch = (channel, ...args) => {
  for (const listener of listeners.get(channel) ?? []) {
    listener(null, ...args);
  }
};

export const ipcRenderer = {
  on(channel: string, listener: PickerListener): void {
    let set = listeners.get(channel);
    if (!set) {
      set = new Set();
      listeners.set(channel, set);
    }
    set.add(listener);
  },
  off(channel: string, listener: PickerListener): void {
    listeners.get(channel)?.delete(listener);
  },
  send(channel: string, ...args: unknown[]): void {
    if (channel === "preview:element-picked") {
      // GuestProtocol's ELEMENT_PICKED_CHANNEL: (annotation | null, rect?).
      window.__t3pPost?.({ kind: "pick", annotation: args[0] ?? null, rect: args[1] ?? null });
    }
    // preview:human-input is intentionally dropped — the Tauri runtime's own
    // trusted-input detection (preview-runtime.src.js) already reports human
    // control to the shell.
  },
};
