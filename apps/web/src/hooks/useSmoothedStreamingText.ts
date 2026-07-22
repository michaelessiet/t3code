import { useEffect, useRef, useState } from "react";

/** Aim to catch up to the freshest streamed text within this window. */
const CATCH_UP_WINDOW_MS = 220;
/**
 * If the reveal falls further behind than this (a code-block spill, a huge
 * coalesced flush), jump close to the edge instead of crawling through it.
 */
const MAX_SMOOTH_LAG_CHARS = 640;
/**
 * Very long messages re-parse markdown on every reveal commit; past this
 * size the smoothing is not worth the CPU, so render chunks as they arrive.
 */
const MAX_SMOOTHED_TEXT_LENGTH = 20_000;
/** Floor speed so short tails still finish promptly. */
const MIN_CHARS_PER_MS = 0.045;
/** Commit reveal updates at ~30fps; per-frame markdown re-parses buy nothing. */
const COMMIT_INTERVAL_MS = 28;

function prefersReducedMotion(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

/**
 * Smooths a streaming message's text reveal: instead of jumping in
 * network-chunk-sized bursts, the visible text glides toward the freshest
 * text at a rate that always catches up within ~{@link CATCH_UP_WINDOW_MS}.
 *
 * - Returns `text` unchanged (and stops all work) when not streaming, under
 *   prefers-reduced-motion, or for very long messages.
 * - Mounting mid-stream reveals the existing text instantly; only text that
 *   arrives after mount glides. Virtualized rows scrolling back into view
 *   therefore never replay.
 * - If `text` shrinks (message rewritten/reverted), the reveal snaps.
 */
export function useSmoothedStreamingText(text: string, streaming: boolean): string {
  const [revealedLength, setRevealedLength] = useState(text.length);
  // Fractional reveal position; state commits are throttled snapshots of it.
  const revealedRef = useRef<number>(text.length);
  const textRef = useRef(text);
  const frameRef = useRef(0);
  const lastTickRef = useRef(0);
  const lastCommitRef = useRef(0);

  textRef.current = text;

  useEffect(() => {
    const snap = () => {
      if (frameRef.current !== 0) {
        cancelAnimationFrame(frameRef.current);
        frameRef.current = 0;
      }
      revealedRef.current = text.length;
      setRevealedLength(text.length);
    };

    if (!streaming || text.length > MAX_SMOOTHED_TEXT_LENGTH || prefersReducedMotion()) {
      snap();
      return;
    }
    if (text.length < revealedRef.current) {
      snap();
      return;
    }
    if (text.length - revealedRef.current > MAX_SMOOTH_LAG_CHARS) {
      revealedRef.current = text.length - MAX_SMOOTH_LAG_CHARS;
    }
    if (frameRef.current !== 0) {
      // A glide is already running; it reads the freshest text via textRef.
      return;
    }

    lastTickRef.current = 0;
    const step = (now: number) => {
      frameRef.current = 0;
      const target = textRef.current.length;
      const remaining = target - revealedRef.current;
      if (remaining <= 0) {
        revealedRef.current = target;
        setRevealedLength(target);
        return;
      }
      const dt = lastTickRef.current === 0 ? 16 : Math.min(64, now - lastTickRef.current);
      lastTickRef.current = now;
      const speed = Math.max(MIN_CHARS_PER_MS, remaining / CATCH_UP_WINDOW_MS);
      revealedRef.current = Math.min(target, revealedRef.current + speed * dt);
      if (now - lastCommitRef.current >= COMMIT_INTERVAL_MS || revealedRef.current >= target) {
        lastCommitRef.current = now;
        const next = Math.floor(revealedRef.current);
        setRevealedLength((current) => (next > current ? next : current));
      }
      frameRef.current = requestAnimationFrame(step);
    };
    frameRef.current = requestAnimationFrame(step);
  }, [text, streaming]);

  useEffect(
    () => () => {
      if (frameRef.current !== 0) {
        cancelAnimationFrame(frameRef.current);
        frameRef.current = 0;
      }
    },
    [],
  );

  if (!streaming || revealedLength >= text.length) {
    return text;
  }
  return text.slice(0, revealedLength);
}
