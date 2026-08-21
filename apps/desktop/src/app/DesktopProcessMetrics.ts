/**
 * Dev-only per-process memory sampler.
 *
 * Electron's per-process memory (GPU process, preview guest renderers) is
 * invisible to the server's ProcessResourceMonitor, which only samples the
 * server's own descendants. This logs an app.getAppMetrics() summary on an
 * interval so preview-lifecycle memory changes (open/hide/close tabs) are
 * observable in dev without attaching an external profiler. Never runs in
 * packaged builds.
 */
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Schedule from "effect/Schedule";

import * as DesktopEnvironment from "./DesktopEnvironment.ts";

const SAMPLE_INTERVAL = Duration.seconds(30);

export interface DesktopProcessMetric {
  readonly type: string;
  readonly memory: {
    readonly workingSetSize: number;
  };
}

export interface DesktopProcessTypeSummary {
  readonly count: number;
  readonly workingSetMB: number;
}

export function summarizeProcessMetrics(
  metrics: ReadonlyArray<DesktopProcessMetric>,
): Record<string, DesktopProcessTypeSummary> {
  const summary: Record<string, { count: number; workingSetMB: number }> = {};
  for (const metric of metrics) {
    const entry = (summary[metric.type] ??= { count: 0, workingSetMB: 0 });
    entry.count += 1;
    entry.workingSetMB += metric.memory.workingSetSize / 1024;
  }
  for (const entry of Object.values(summary)) {
    entry.workingSetMB = Math.round(entry.workingSetMB);
  }
  return summary;
}

export const layer = (
  readMetrics: () => ReadonlyArray<DesktopProcessMetric>,
): Layer.Layer<never, never, DesktopEnvironment.DesktopEnvironment> =>
  Layer.effectDiscard(
    Effect.gen(function* () {
      const environment = yield* DesktopEnvironment.DesktopEnvironment;
      if (!environment.isDevelopment) {
        return;
      }
      yield* Effect.suspend(() =>
        Effect.log("[desktop-process-metrics]", summarizeProcessMetrics(readMetrics())),
      ).pipe(Effect.repeat(Schedule.spaced(SAMPLE_INTERVAL)), Effect.forkScoped);
    }),
  );
