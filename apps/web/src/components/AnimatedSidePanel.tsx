import {
  type ReactNode,
  type TransitionEvent,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";

import { cn } from "~/lib/utils";

/** Matches the left sidebar's width animation (200ms linear). */
const PANEL_TRANSITION_FALLBACK_MS = 300;

function prefersReducedMotion(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

/**
 * Horizontal reveal/collapse for side panels, matching the left sidebar's
 * 200ms linear width animation.
 *
 * Plays one-shot width transitions: opening mounts the children collapsed and
 * expands to their natural width; closing collapses to zero and only then
 * unmounts. Between animations no inline width is applied, so user-driven
 * panel resizing stays live and is never animated. Inner content keeps its
 * own width during the sweep, so the panel clips at the moving edge instead
 * of reflowing.
 */
export function AnimatedSidePanel({
  open,
  immediate = false,
  className,
  children,
}: {
  open: boolean;
  /** Mount/unmount without animating (e.g. a maximized panel). */
  immediate?: boolean;
  className?: string;
  children: ReactNode;
}) {
  const [mounted, setMounted] = useState(open);
  // null = settled (no inline width; content defines it), number = animating.
  const [targetWidth, setTargetWidth] = useState<number | null>(open ? null : 0);
  const contentRef = useRef<HTMLDivElement | null>(null);
  const skipAnimation = immediate || prefersReducedMotion();

  useLayoutEffect(() => {
    if (skipAnimation) {
      setMounted(open);
      setTargetWidth(open ? null : 0);
      return;
    }
    if (open) {
      if (!mounted) {
        // Mount collapsed; the expansion effect below measures and animates.
        setMounted(true);
        setTargetWidth(0);
      } else if (targetWidth !== null) {
        // Reopened mid-close: head back to the content's width.
        setTargetWidth(contentRef.current?.scrollWidth ?? 0);
      }
      return;
    }
    if (!mounted) {
      return;
    }
    // Freeze at the current width this commit; collapse on the next frame so
    // the style change transitions instead of snapping.
    const width = contentRef.current?.offsetWidth ?? 0;
    setTargetWidth(width);
    const frame = requestAnimationFrame(() => setTargetWidth(0));
    return () => cancelAnimationFrame(frame);
    // Reacts to open/skip changes only: `mounted`/`targetWidth` are read to
    // decide the animation leg, not to re-trigger it.
  }, [open, skipAnimation]);

  // Expansion leg of the open animation: once mounted at width 0, measure the
  // content's natural width and transition to it.
  useLayoutEffect(() => {
    if (!open || skipAnimation || !mounted || targetWidth !== 0) {
      return;
    }
    const width = contentRef.current?.scrollWidth ?? 0;
    const frame = requestAnimationFrame(() => setTargetWidth(width));
    return () => cancelAnimationFrame(frame);
  }, [open, skipAnimation, mounted, targetWidth]);

  const settle = useCallback(() => {
    if (open) {
      setTargetWidth(null);
    } else {
      setMounted(false);
      setTargetWidth(0);
    }
  }, [open]);

  // Fallback: transitionend can be swallowed (tab hidden, transition removed
  // mid-flight). Whenever an inline width lingers, settle after a beat.
  useEffect(() => {
    if (targetWidth === null || !mounted) {
      return;
    }
    const timeout = window.setTimeout(settle, PANEL_TRANSITION_FALLBACK_MS);
    return () => window.clearTimeout(timeout);
  }, [targetWidth, mounted, settle]);

  const handleTransitionEnd = useCallback(
    (event: TransitionEvent<HTMLDivElement>) => {
      if (event.target !== event.currentTarget || event.propertyName !== "width") {
        return;
      }
      settle();
    },
    [settle],
  );

  if (!mounted) {
    return null;
  }

  const animating = targetWidth !== null;

  return (
    <div
      className={cn(
        "flex min-h-0",
        animating && "overflow-hidden transition-[width] duration-200 ease-linear",
        className,
      )}
      style={animating ? { width: `${targetWidth}px` } : undefined}
      onTransitionEnd={handleTransitionEnd}
      data-side-panel-state={animating ? (open ? "opening" : "closing") : "open"}
    >
      <div ref={contentRef} className="flex h-full min-h-0 min-w-0 flex-1">
        {children}
      </div>
    </div>
  );
}
