"use client";

import type { KeybindingCommand } from "@t3tools/contracts";

/**
 * Typed window-event bus for running keybinding commands from UI entry
 * points other than the keyboard (command palette). Handlers that own a
 * command's behavior (ChatView, QuickSearch) subscribe and run the same
 * logic their keydown path uses.
 */
const EVENT_NAME = "t3code:app-command";

export function dispatchAppCommand(command: KeybindingCommand): void {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent<KeybindingCommand>(EVENT_NAME, { detail: command }));
}

export function subscribeAppCommand(listener: (command: KeybindingCommand) => void): () => void {
  if (typeof window === "undefined") return () => {};
  const handler = (event: Event) => {
    const detail = (event as CustomEvent<KeybindingCommand>).detail;
    if (typeof detail === "string") listener(detail);
  };
  window.addEventListener(EVENT_NAME, handler);
  return () => window.removeEventListener(EVENT_NAME, handler);
}
