/**
 * In-memory clipboard for copying workspace entries between thread file
 * explorers.
 *
 * Only one file explorer (the active thread's) is mounted at a time, so a
 * cross-thread copy can't be a live drag between two panels. Instead, "Copy"
 * stashes the source entry here; switching to another thread and choosing
 * "Paste" reads it back and performs the copy against that thread's workspace.
 *
 * Deliberately not persisted: a clipboard entry references a live workspace
 * path that may not survive a reload, and file copies shouldn't silently span
 * app sessions.
 */
import type { EnvironmentId } from "@t3tools/contracts";
import { create } from "zustand";

export interface FileClipboardEntry {
  /** Environment hosting the source workspace. */
  readonly environmentId: EnvironmentId;
  /** Source workspace root (thread worktree path or project root). */
  readonly cwd: string;
  /** Source path relative to `cwd`. */
  readonly relativePath: string;
  readonly kind: "file" | "directory";
  /** Basename of the source, used as the default destination name. */
  readonly name: string;
  /** Source thread title, for display in the destination's paste affordance. */
  readonly threadTitle: string;
  /** Source project name, for display. */
  readonly projectName: string;
}

interface FileClipboardState {
  entry: FileClipboardEntry | null;
  copy: (entry: FileClipboardEntry) => void;
  clear: () => void;
}

export const useFileClipboardStore = create<FileClipboardState>()((set) => ({
  entry: null,
  copy: (entry) => set({ entry }),
  clear: () => set({ entry: null }),
}));
