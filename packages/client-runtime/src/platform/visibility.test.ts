import { describe, expect, it } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as Fiber from "effect/Fiber";
import * as Ref from "effect/Ref";
import * as Stream from "effect/Stream";
import * as SubscriptionRef from "effect/SubscriptionRef";
import * as TestClock from "effect/testing/TestClock";

import { pauseStreamWhileHidden } from "./visibility.ts";

const HIDDEN_GRACE = "2 minutes";

const makeHarness = Effect.gen(function* () {
  const visibility = yield* SubscriptionRef.make(true);
  const subscribeCount = yield* Ref.make(0);
  const teardownCount = yield* Ref.make(0);
  const emitted: Array<number> = [];
  // Emits its subscription ordinal, then stays open until interrupted.
  const inner = Stream.concat(
    Stream.fromEffect(Ref.updateAndGet(subscribeCount, (count) => count + 1)),
    Stream.never,
  ).pipe(Stream.ensuring(Ref.update(teardownCount, (count) => count + 1)));
  const fiber = yield* pauseStreamWhileHidden(
    SubscriptionRef.changes(visibility),
    inner,
    HIDDEN_GRACE,
  ).pipe(
    Stream.runForEach((ordinal) =>
      Effect.sync(() => {
        emitted.push(ordinal);
      }),
    ),
    Effect.forkChild,
  );
  const waitFor = (predicate: Effect.Effect<boolean>) =>
    Effect.gen(function* () {
      for (let attempt = 0; attempt < 1_000; attempt += 1) {
        if (yield* predicate) return;
        yield* Effect.yieldNow;
      }
    });
  // Wait on the delivered elements (not the subscription-side Refs): the
  // ordinal takes a few extra scheduler hops to travel from the inner stream
  // to runForEach, and asserting on emitted skips that race.
  const waitForEmitted = (length: number) => waitFor(Effect.sync(() => emitted.length >= length));
  return { visibility, subscribeCount, teardownCount, emitted, fiber, waitFor, waitForEmitted };
});

describe("pauseStreamWhileHidden", () => {
  it.effect("keeps the stream running across a hidden flip shorter than the grace", () =>
    Effect.gen(function* () {
      const harness = yield* makeHarness;
      yield* harness.waitForEmitted(1);
      expect(harness.emitted).toEqual([1]);

      yield* SubscriptionRef.set(harness.visibility, false);
      yield* TestClock.adjust("1 minute");
      yield* SubscriptionRef.set(harness.visibility, true);
      yield* TestClock.adjust("5 minutes");
      yield* Fiber.interrupt(harness.fiber);

      expect(yield* Ref.get(harness.subscribeCount)).toBe(1);
      expect(harness.emitted).toEqual([1]);
    }),
  );

  it.effect("tears down after the grace elapses hidden and resubscribes on visible", () =>
    Effect.gen(function* () {
      const harness = yield* makeHarness;
      yield* harness.waitForEmitted(1);

      yield* SubscriptionRef.set(harness.visibility, false);
      yield* TestClock.adjust("2 minutes");
      yield* harness.waitFor(Ref.get(harness.teardownCount).pipe(Effect.map((n) => n >= 1)));
      expect(yield* Ref.get(harness.teardownCount)).toBe(1);
      expect(harness.emitted).toEqual([1]);

      yield* SubscriptionRef.set(harness.visibility, true);
      yield* harness.waitForEmitted(2);
      yield* Fiber.interrupt(harness.fiber);

      expect(yield* Ref.get(harness.subscribeCount)).toBe(2);
      expect(harness.emitted).toEqual([1, 2]);
    }),
  );
});
