import { assert, describe, it } from "@effect/vitest";
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Option from "effect/Option";
import * as Ref from "effect/Ref";
import { HttpClient } from "effect/unstable/http";
import { ChildProcessSpawner } from "effect/unstable/process";

import * as DesktopObservability from "../app/DesktopObservability.ts";
import * as DesktopAppSettings from "../settings/DesktopAppSettings.ts";
import * as ElectronDialog from "../electron/ElectronDialog.ts";
import * as ElectronWindow from "../electron/ElectronWindow.ts";
import * as IpcChannels from "../ipc/channels.ts";
import * as DesktopWindow from "../window/DesktopWindow.ts";
import * as DesktopBackendConfiguration from "./DesktopBackendConfiguration.ts";
import * as DesktopBackendPool from "./DesktopBackendPool.ts";
import type {
  BackendInstanceSpec,
  DesktopBackendSnapshot,
  DesktopBackendStartConfig,
} from "./DesktopBackendManager.ts";

function makeStubInstance(
  id: DesktopBackendPool.BackendInstanceId,
  label: string,
): DesktopBackendPool.DesktopBackendInstance {
  const snapshot: DesktopBackendSnapshot = {
    desiredRunning: false,
    ready: false,
    activePid: Option.none(),
    restartAttempt: 0,
    restartScheduled: false,
  };
  return {
    id,
    label: Effect.succeed(label),
    start: Effect.void,
    stop: () => Effect.void,
    currentConfig: Effect.succeed(Option.none<DesktopBackendStartConfig>()),
    snapshot: Effect.succeed(snapshot),
    waitForReady: (_timeout: Duration.Duration) => Effect.succeed(false),
  };
}

function makePoolLayer(
  labelRef: Ref.Ref<string>,
  options?: { readonly onSendAll?: (channel: string) => void },
): Layer.Layer<DesktopBackendPool.DesktopBackendPool> {
  return DesktopBackendPool.layer.pipe(
    Layer.provideMerge(
      Layer.mergeAll(
        Layer.mock(ElectronWindow.ElectronWindow)({
          sendAll: (channel) =>
            Effect.sync(() => {
              options?.onSendAll?.(channel);
            }),
        }),
        FileSystem.layerNoop({}),
        Layer.succeed(
          ChildProcessSpawner.ChildProcessSpawner,
          ChildProcessSpawner.make(() => Effect.die("unexpected child process spawn")),
        ),
        Layer.succeed(
          HttpClient.HttpClient,
          HttpClient.make(() => Effect.die("unexpected HTTP request")),
        ),
        Layer.succeed(DesktopObservability.DesktopBackendOutputLogFactory, {
          forInstance: () =>
            Effect.succeed({
              writeSessionBoundary: () => Effect.void,
              writeOutputChunk: () => Effect.void,
            } satisfies DesktopObservability.DesktopBackendOutputLogShape),
        } satisfies DesktopObservability.DesktopBackendOutputLogFactory["Service"]),
        Layer.succeed(DesktopBackendConfiguration.DesktopBackendConfiguration, {
          resolvePrimary: Effect.die("unexpected primary config resolve"),
          resolvePrimaryLabel: Ref.get(labelRef),
          resolveWsl: () => Effect.die("unexpected WSL config resolve"),
        } satisfies DesktopBackendConfiguration.DesktopBackendConfiguration["Service"]),
        DesktopAppSettings.layerTest(),
        ElectronDialog.layer,
        Layer.succeed(DesktopWindow.DesktopWindow, {
          createMain: Effect.die("unexpected window create"),
          ensureMain: Effect.die("unexpected window ensure"),
          revealOrCreateMain: Effect.die("unexpected window reveal"),
          activate: Effect.die("unexpected window activate"),
          createMainIfBackendReady: Effect.die("unexpected window create"),
          showConnectingSplash: Effect.void,
          handleBackendReady: () => Effect.void,
          handleBackendNotReady: Effect.void,
          flushMainWindowBounds: Effect.void,
          dispatchMenuAction: () => Effect.die("unexpected menu action"),
          syncAppearance: Effect.void,
        } satisfies DesktopWindow.DesktopWindow["Service"]),
      ),
    ),
  );
}

describe("DesktopBackendPool", () => {
  it.effect("layerTest exposes registered instances by id", () =>
    Effect.gen(function* () {
      const pool = yield* DesktopBackendPool.DesktopBackendPool;
      const fetchedPrimary = yield* pool.get(DesktopBackendPool.PRIMARY_INSTANCE_ID);
      const fetchedWsl = yield* pool.get(DesktopBackendPool.BackendInstanceId("wsl:ubuntu"));
      const fetchedMissing = yield* pool.get(DesktopBackendPool.BackendInstanceId("missing"));
      const all = yield* pool.list;
      const resolvedPrimary = yield* pool.primary;

      assert.equal(yield* Option.getOrThrow(fetchedPrimary).label, "Windows");
      assert.equal(yield* Option.getOrThrow(fetchedWsl).label, "WSL (Ubuntu)");
      assert.isTrue(Option.isNone(fetchedMissing));
      assert.lengthOf(all, 2);
      // First instance becomes primary in layerTest so single-instance
      // stubs don't have to wire an explicit primary.
      assert.equal(resolvedPrimary.id, DesktopBackendPool.PRIMARY_INSTANCE_ID);
    }).pipe(
      Effect.provide(
        DesktopBackendPool.layerTest([
          makeStubInstance(DesktopBackendPool.PRIMARY_INSTANCE_ID, "Windows"),
          makeStubInstance(DesktopBackendPool.BackendInstanceId("wsl:ubuntu"), "WSL (Ubuntu)"),
        ]),
      ),
    ),
  );

  it.effect("layerTest dies when no instances are supplied", () =>
    Effect.exit(
      Effect.gen(function* () {
        yield* DesktopBackendPool.DesktopBackendPool;
      }).pipe(Effect.provide(DesktopBackendPool.layerTest([]))),
    ).pipe(Effect.map((exit) => assert.equal(exit._tag, "Failure"))),
  );

  it.effect("resolves the primary label lazily after pool layer construction", () =>
    Effect.scoped(
      Effect.gen(function* () {
        const labelRef = yield* Ref.make("Windows");
        const pool = yield* DesktopBackendPool.DesktopBackendPool.pipe(
          Effect.provide(makePoolLayer(labelRef)),
        );
        const primary = yield* pool.primary;

        yield* Ref.set(labelRef, "WSL (Ubuntu)");

        assert.equal(yield* primary.label, "WSL (Ubuntu)");
      }),
    ),
  );

  it.effect("pings renderer windows when the pool registry changes", () => {
    const sends: string[] = [];
    // Provide the layer around the whole test body (rather than extracting
    // the service through Effect.provide inside it) so the pool's layer scope
    // stays open across register/unregister — closing it early would run the
    // instances' stop finalizers and ping ahead of the assertions.
    return Effect.gen(function* () {
      const pool = yield* DesktopBackendPool.DesktopBackendPool;

      const id = DesktopBackendPool.BackendInstanceId("wsl:test");
      yield* pool.register({
        id,
        label: Effect.succeed("WSL (test)"),
        configResolve: Effect.die("unexpected WSL config resolve"),
      });
      // A freshly registered instance surfaces as a pending bootstrap, so
      // registration alone must ping.
      assert.deepEqual(sends, [IpcChannels.LOCAL_ENVIRONMENT_BOOTSTRAPS_CHANGED_CHANNEL]);

      yield* pool.unregister(id);
      // Unregister pings twice: closing the instance scope runs its stop
      // finalizer (whose wrapped onShutdown pings), then the pool pings for
      // the registry removal itself. Duplicate pings are harmless — the
      // renderer just re-reads the sync getter.
      assert.deepEqual(sends, [
        IpcChannels.LOCAL_ENVIRONMENT_BOOTSTRAPS_CHANGED_CHANNEL,
        IpcChannels.LOCAL_ENVIRONMENT_BOOTSTRAPS_CHANGED_CHANNEL,
        IpcChannels.LOCAL_ENVIRONMENT_BOOTSTRAPS_CHANGED_CHANNEL,
      ]);
    }).pipe(
      Effect.provide(
        makePoolLayer(Ref.makeUnsafe("Windows"), {
          onSendAll: (channel) => sends.push(channel),
        }),
      ),
    );
  });
});

describe("withLocalBootstrapChangeNotifications", () => {
  const baseSpec: BackendInstanceSpec = {
    id: DesktopBackendPool.BackendInstanceId("wsl:test"),
    label: Effect.succeed("WSL (test)"),
    configResolve: Effect.die("unexpected config resolve"),
  };

  it.effect("notifies on ready, shutdown, and preflight-failure transitions", () =>
    Effect.gen(function* () {
      const events: string[] = [];
      let notifications = 0;
      const spec = DesktopBackendPool.withLocalBootstrapChangeNotifications(
        {
          ...baseSpec,
          onReady: () =>
            Effect.sync(() => {
              events.push("ready");
            }),
          onShutdown: () =>
            Effect.sync(() => {
              events.push("shutdown");
            }),
          onPreflightFailed: () =>
            Effect.sync(() => {
              events.push("preflight");
              return true;
            }),
        },
        Effect.sync(() => {
          notifications += 1;
        }),
      );

      yield* spec.onReady!(new URL("http://127.0.0.1:3774"));
      yield* spec.onShutdown!();
      const shouldRestart = yield* spec.onPreflightFailed!({
        reason: "no node",
        fatal: true,
      });

      // The wrapped callbacks still run the original side effects and
      // preserve onPreflightFailed's restart decision.
      assert.deepEqual(events, ["ready", "shutdown", "preflight"]);
      assert.isTrue(shouldRestart);
      assert.equal(notifications, 3);
    }),
  );

  it.effect("notifies even when the spec omits the lifecycle callbacks", () =>
    Effect.gen(function* () {
      let notifications = 0;
      const spec = DesktopBackendPool.withLocalBootstrapChangeNotifications(
        baseSpec,
        Effect.sync(() => {
          notifications += 1;
        }),
      );

      yield* spec.onReady!(new URL("http://127.0.0.1:3774"));
      yield* spec.onShutdown!();
      assert.isFalse(yield* spec.onPreflightFailed!({ reason: "no node", fatal: true }));
      assert.equal(notifications, 3);
    }),
  );
});
