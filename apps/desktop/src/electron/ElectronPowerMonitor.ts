import * as Context from "effect/Context";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Scope from "effect/Scope";

import * as Electron from "electron";

export type ElectronPowerMonitorEvent = "suspend" | "resume";

export class ElectronPowerMonitor extends Context.Service<
  ElectronPowerMonitor,
  {
    readonly on: (
      eventName: ElectronPowerMonitorEvent,
      listener: () => void,
    ) => Effect.Effect<void, never, Scope.Scope>;
  }
>()("@t3tools/desktop/electron/ElectronPowerMonitor") {}

export const make = ElectronPowerMonitor.of({
  on: (eventName, listener) => {
    const eventTarget = Electron.powerMonitor as unknown as {
      on: (eventName: string, listener: () => void) => void;
      removeListener: (eventName: string, listener: () => void) => void;
    };
    return Effect.acquireRelease(
      Effect.suspend(() => {
        eventTarget.on(eventName, listener);
        return Effect.void;
      }),
      () =>
        Effect.suspend(() => {
          eventTarget.removeListener(eventName, listener);
          return Effect.void;
        }),
    );
  },
});

export const layer = Layer.succeed(ElectronPowerMonitor, make);
