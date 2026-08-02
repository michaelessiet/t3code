/** Gap kept between a floating menu and the viewport edge, in pixels. */
export const MENU_VIEWPORT_MARGIN = 4;

export interface MenuSize {
  readonly width: number;
  readonly height: number;
}

export interface MenuPoint {
  readonly left: number;
  readonly top: number;
}

export interface MenuViewport {
  readonly width: number;
  readonly height: number;
}

/**
 * Keeps an anchored menu inside the viewport. Callers pass the *measured* size
 * of the rendered menu — guessing it (a hardcoded height, say) silently clips
 * the last items once the menu grows. A menu larger than the viewport is pinned
 * to the top-left margin so its first items stay reachable; capping its height
 * with a scroll container is the caller's job.
 */
export function clampMenuToViewport(
  size: MenuSize,
  preferred: MenuPoint,
  viewport: MenuViewport,
  margin: number = MENU_VIEWPORT_MARGIN,
): MenuPoint {
  return {
    left: Math.min(
      Math.max(margin, preferred.left),
      Math.max(margin, viewport.width - size.width - margin),
    ),
    top: Math.min(
      Math.max(margin, preferred.top),
      Math.max(margin, viewport.height - size.height - margin),
    ),
  };
}
