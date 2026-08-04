import type { AtomCommandResult } from "@t3tools/client-runtime/state/runtime";

/**
 * How pending edits reach disk. "debounce" persists automatically after the
 * given quiet period (autosave after delay). "manual" only marks edits
 * pending and waits for an explicit flush() — used both when autosave is off
 * (⌘S / vim `:w`) and for focus-triggered autosave, where the focus loss is
 * what calls flush().
 */
export type SaveScheduling =
  | { readonly kind: "debounce"; readonly delayMs: number }
  | { readonly kind: "manual" };

export interface FileSaveCoordinatorOptions<A, E> {
  /**
   * Read live on every scheduling decision so a settings change mid-edit
   * takes effect without recreating the coordinator (recreation would
   * dispose, and disposing can flush or drop pending state).
   */
  readonly getScheduling: () => SaveScheduling;
  /**
   * Whether dispose() persists pending edits. Autosave modes flush on
   * unmount; with autosave off the buffer intentionally stays unsaved (it
   * survives in the optimistic file atom and feeds the unsaved-changes
   * prompt).
   */
  readonly getFlushOnDispose: () => boolean;
  readonly persist: (contents: string) => Promise<AtomCommandResult<A, E>>;
  readonly onPendingChange: (pending: boolean) => void;
  readonly onConfirmed: (contents: string) => void;
}

export class FileSaveCoordinator<A = unknown, E = unknown> {
  private timer: ReturnType<typeof setTimeout> | null = null;
  private latestContents = "";
  private latestRevision = 0;
  private lastChangeAt = 0;
  private saving = false;
  private disposed = false;
  private flushRequested = false;
  private inflight: Promise<void> | null = null;

  constructor(private readonly options: FileSaveCoordinatorOptions<A, E>) {}

  change(contents: string): void {
    this.latestContents = contents;
    this.latestRevision += 1;
    this.lastChangeAt = Date.now();
    this.options.onPendingChange(true);
    const scheduling = this.options.getScheduling();
    if (scheduling.kind === "debounce") {
      this.schedule(scheduling.delayMs);
    } else {
      this.clearTimer();
    }
  }

  dispose(): void {
    this.disposed = true;
    this.clearTimer();
    if (this.latestRevision > 0 && this.options.getFlushOnDispose()) void this.persistLatest();
  }

  /**
   * Persist pending edits now instead of waiting out the debounce (manual
   * save, vim `:w`, focus-lost autosave). Resolves once the buffer has been
   * persisted; a flush while a save is already in flight persists any
   * trailing edits as soon as that save settles rather than dropping them.
   */
  flush(): Promise<void> {
    this.clearTimer();
    if (this.saving) {
      this.flushRequested = true;
      return this.inflight ?? Promise.resolve();
    }
    return this.persistLatest();
  }

  /**
   * Discard unsaved local changes (e.g. when the user chooses to reload the
   * buffer from disk after a concurrent-edit conflict). Pending debounced
   * saves are cancelled; an in-flight save settles without rescheduling.
   */
  reset(): void {
    this.clearTimer();
    this.latestContents = "";
    this.latestRevision = 0;
    this.flushRequested = false;
    if (!this.saving) this.options.onPendingChange(false);
  }

  private schedule(delay: number): void {
    this.clearTimer();
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.persistLatest();
    }, delay);
  }

  private clearTimer(): void {
    if (this.timer === null) return;
    clearTimeout(this.timer);
    this.timer = null;
  }

  private persistLatest(): Promise<void> {
    if (this.saving) return this.inflight ?? Promise.resolve();
    if (this.latestRevision === 0) return Promise.resolve();
    this.inflight = this.run().finally(() => {
      this.inflight = null;
    });
    return this.inflight;
  }

  private async run(): Promise<void> {
    for (;;) {
      this.saving = true;
      const contents = this.latestContents;
      const revision = this.latestRevision;
      const result = await this.options.persist(contents);
      const succeeded = result._tag === "Success";
      if (succeeded) {
        this.options.onConfirmed(contents);
      }

      this.saving = false;
      if (this.latestRevision === 0) {
        // A reset() landed while this save was in flight; nothing left to persist.
        this.flushRequested = false;
        this.options.onPendingChange(false);
        return;
      }
      if (revision === this.latestRevision) {
        this.flushRequested = false;
        if (succeeded) this.options.onPendingChange(false);
        return;
      }

      // Trailing edits landed while the save was in flight.
      if (this.flushRequested) {
        this.flushRequested = false;
        continue;
      }
      if (this.disposed) {
        if (this.options.getFlushOnDispose()) continue;
        return;
      }
      const scheduling = this.options.getScheduling();
      if (scheduling.kind === "manual") {
        // Stay pending; the next explicit flush picks the edits up.
        return;
      }
      this.schedule(Math.max(0, scheduling.delayMs - (Date.now() - this.lastChangeAt)));
      return;
    }
  }
}
