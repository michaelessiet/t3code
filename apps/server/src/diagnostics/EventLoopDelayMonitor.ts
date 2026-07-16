import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Metric from "effect/Metric";
import { monitorEventLoopDelay } from "node:perf_hooks";

import { eventLoopDelay } from "../observability/Metrics.ts";

const SAMPLE_INTERVAL_MS = 5_000;

/**
 * Samples Node event-loop delay so synchronous work on the main thread
 * (e.g. SQLite statements) shows up as measurable stall time. Reports the
 * p99 of each sampling window in milliseconds.
 */
export const layer = Layer.effectDiscard(
  Effect.gen(function* () {
    const histogram = monitorEventLoopDelay({ resolution: 20 });
    histogram.enable();
    yield* Effect.addFinalizer(() => Effect.sync(() => histogram.disable()));

    const sampleOnce = Effect.gen(function* () {
      const p99Ms = histogram.percentile(99) / 1e6;
      histogram.reset();
      yield* Metric.update(eventLoopDelay, p99Ms);
    });

    yield* Effect.forever(sampleOnce.pipe(Effect.andThen(Effect.sleep(SAMPLE_INTERVAL_MS)))).pipe(
      Effect.forkScoped,
    );
  }),
);
