/**
 * True when keyboard focus is anywhere inside the right panel (any surface:
 * file editor, diff, search, preview, panel terminal, or the tab bar).
 *
 * Reuses the `data-preview-panel-mode` marker PreviewPanelShell puts on the
 * panel shell; "inline" and "sheet" are the right-panel hosts.
 */
export function isRightPanelFocused(): boolean {
  const activeElement = document.activeElement;
  if (!(activeElement instanceof HTMLElement)) return false;
  if (!activeElement.isConnected) return false;
  if (activeElement.tagName.toLowerCase() === "webview") return true;
  const shell = activeElement.closest<HTMLElement>("[data-preview-panel-mode]");
  const mode = shell?.dataset.previewPanelMode;
  return mode === "inline" || mode === "sheet";
}

/**
 * Move keyboard focus into the right panel shell (it carries tabIndex=-1).
 * Skipped when focus is already inside the panel — e.g. an editor that just
 * took a focus handoff — so callers can invoke this unconditionally.
 */
export function focusRightPanel(): void {
  if (isRightPanelFocused()) return;
  document
    .querySelector<HTMLElement>(
      '[data-preview-panel-mode="inline"], [data-preview-panel-mode="sheet"]',
    )
    ?.focus();
}

/**
 * Focus the panel shell once it can actually take focus. Opening the panel
 * is asynchronous in both host modes — the inline panel mounts over several
 * commits while it animates in, and the sheet popup is keep-mounted but
 * hidden until its open transition starts — so a single focus() call can
 * land on a missing or still-hidden element and silently no-op. Retries
 * every frame until focus lands inside the panel (a deeper surface handoff
 * counts) or the budget runs out.
 */
export function focusRightPanelWhenReady(maxFrames = 40): void {
  let attempts = 0;
  const tick = () => {
    // A surface-level handoff (editor, terminal) may have already landed.
    if (isRightPanelFocused()) return;
    const shell = document.querySelector<HTMLElement>(
      '[data-preview-panel-mode="inline"], [data-preview-panel-mode="sheet"]',
    );
    shell?.focus();
    if (shell !== null && document.activeElement === shell) return;
    attempts += 1;
    if (attempts < maxFrames) window.requestAnimationFrame(tick);
  };
  window.requestAnimationFrame(tick);
}
