import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as Queue from "effect/Queue";
import * as Stream from "effect/Stream";

/**
 * Document visibility as a stream: the current state, then every change.
 * Outside a DOM (tests, node tooling) the document counts as always visible.
 */
export function documentVisibilityChanges(): Stream.Stream<boolean> {
  if (typeof document === "undefined") {
    return Stream.concat(Stream.make(true), Stream.never);
  }
  return Stream.concat(
    Stream.sync(() => document.visibilityState === "visible"),
    Stream.callback<boolean>((queue) =>
      Effect.acquireRelease(
        Effect.sync(() => {
          const listener = () => {
            Queue.offerUnsafe(queue, document.visibilityState === "visible");
          };
          document.addEventListener("visibilitychange", listener);
          return listener;
        }),
        (listener) => Effect.sync(() => document.removeEventListener("visibilitychange", listener)),
      ).pipe(Effect.asVoid),
    ),
  );
}

/**
 * Interrupts `stream` after the visibility source has reported hidden for
 * `hiddenGrace`, and re-runs it from scratch when visibility returns. The
 * grace absorbs quick app switches, so the stream is only torn down when the
 * window stays hidden — use this for subscriptions whose server-side work
 * (polling, fetching) should stop while nobody can see the result, and whose
 * resubscription starts from a fresh snapshot. Exported separately from
 * {@link pauseWhileDocumentHidden} so tests can inject a visibility stream.
 */
export function pauseStreamWhileHidden<A, E, R>(
  visibility: Stream.Stream<boolean>,
  stream: Stream.Stream<A, E, R>,
  hiddenGrace: Duration.Input,
): Stream.Stream<A, E, R> {
  return visibility.pipe(
    Stream.switchMap((visible) =>
      visible
        ? Stream.make(true)
        : Stream.fromEffect(Effect.sleep(hiddenGrace).pipe(Effect.as(false))),
    ),
    // Dedupe so hidden->visible flips inside the grace window never reach the
    // outer switchMap (which would needlessly interrupt and resubscribe).
    Stream.changes,
    Stream.switchMap((active) => (active ? stream : Stream.empty)),
  );
}

export function pauseWhileDocumentHidden<A, E, R>(
  stream: Stream.Stream<A, E, R>,
  options: { readonly hiddenGrace: Duration.Input },
): Stream.Stream<A, E, R> {
  return pauseStreamWhileHidden(documentVisibilityChanges(), stream, options.hiddenGrace);
}
