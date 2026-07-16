import {
  ORCHESTRATION_WS_METHODS,
  type EnvironmentId as EnvironmentIdType,
  type OrchestrationThread,
  type OrchestrationThreadDetailSnapshot,
  type OrchestrationThreadStreamItem,
  type ThreadId as ThreadIdType,
} from "@t3tools/contracts";
import * as Cause from "effect/Cause";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import * as Queue from "effect/Queue";
import * as Stream from "effect/Stream";
import * as SubscriptionRef from "effect/SubscriptionRef";
import { Atom } from "effect/unstable/reactivity";

import { EnvironmentRegistry } from "../connection/registry.ts";
import { connectionProjectionPhase } from "../connection/model.ts";
import { EnvironmentSupervisor } from "../connection/supervisor.ts";
import { EnvironmentCacheStore } from "../platform/persistence.ts";
import { subscribe } from "../rpc/client.ts";
import { ThreadSnapshotLoader } from "./threadSnapshotHttp.ts";
import { parseThreadKey, threadKey } from "./entities.ts";
import { applyThreadDetailEvent } from "./threadReducer.ts";
import { THREAD_STATE_IDLE_TTL_MS } from "./threadRetention.ts";
import { followStreamInEnvironment } from "./runtime.ts";
import {
  EMPTY_ENVIRONMENT_THREAD_STATE,
  type EnvironmentThreadState,
  type EnvironmentThreadStatus,
} from "./threadState.ts";

function statusWithoutLiveData(data: Option.Option<OrchestrationThread>): EnvironmentThreadStatus {
  return Option.isSome(data) ? "cached" : "empty";
}

function formatThreadError(cause: Cause.Cause<unknown>): string {
  const error = Cause.squash(cause);
  return error instanceof Error && error.message.trim().length > 0
    ? error.message
    : "Could not synchronize the thread.";
}

export const makeEnvironmentThreadState = Effect.fn("EnvironmentThreadState.make")(function* (
  threadId: ThreadIdType,
) {
  const supervisor = yield* EnvironmentSupervisor;
  const cache = yield* EnvironmentCacheStore;
  const snapshotLoader = yield* ThreadSnapshotLoader;
  const environmentId = supervisor.target.environmentId;
  const cached = yield* cache.loadThread(environmentId, threadId).pipe(
    Effect.catch((error) =>
      Effect.logWarning("Could not load cached thread.").pipe(
        Effect.annotateLogs({
          environmentId,
          threadId,
          error: error.message,
        }),
        Effect.as(Option.none<OrchestrationThreadDetailSnapshot>()),
      ),
    ),
  );
  const cachedThread = Option.map(cached, (snapshot) => snapshot.thread);
  const state = yield* SubscriptionRef.make<EnvironmentThreadState>({
    data: cachedThread,
    status: statusWithoutLiveData(cachedThread),
    error: Option.none(),
  });

  // The reducer's source of truth. `state.data` renders this thread with the
  // ephemeral overlays applied; only this un-overlaid thread may be persisted
  // to the cache, otherwise overlay text would be replayed twice on resume.
  let persistedThread = cachedThread;
  type ThreadMessage = OrchestrationThread["messages"][number];
  const overlays = new Map<
    ThreadMessage["id"],
    { turnId: ThreadMessage["turnId"]; createdAt: string; text: string }
  >();

  const overlaidThread = (thread: OrchestrationThread): OrchestrationThread => {
    if (overlays.size === 0) {
      return thread;
    }
    let messages = thread.messages;
    let changed = false;
    for (const [messageId, overlay] of overlays) {
      if (overlay.text.length === 0) {
        continue;
      }
      const existing = messages.find((entry) => entry.id === messageId);
      if (existing) {
        if (!existing.streaming) {
          continue;
        }
        messages = messages.map((entry) =>
          entry.id === messageId ? { ...entry, text: `${entry.text}${overlay.text}` } : entry,
        );
      } else {
        messages = [
          ...messages,
          {
            id: messageId,
            role: "assistant" as const,
            text: overlay.text,
            turnId: overlay.turnId,
            streaming: true,
            createdAt: overlay.createdAt,
            updatedAt: overlay.createdAt,
          },
        ];
      }
      changed = true;
    }
    return changed ? { ...thread, messages } : thread;
  };
  // Seed the resume cursor from the cached snapshot so a warm cache can catch up
  // via `afterSequence` instead of re-downloading the full thread body.
  const lastSequence = yield* SubscriptionRef.make(
    Option.match(cached, { onNone: () => 0, onSome: (snapshot) => snapshot.snapshotSequence }),
  );
  const persistence = yield* Queue.sliding<OrchestrationThreadDetailSnapshot>(1);

  const persist = Effect.fn("EnvironmentThreadState.persist")(function* (
    snapshot: OrchestrationThreadDetailSnapshot,
  ) {
    yield* cache.saveThread(environmentId, snapshot).pipe(
      Effect.catch((error) =>
        Effect.logWarning("Could not persist the thread cache.").pipe(
          Effect.annotateLogs({
            environmentId,
            threadId,
            error: error.message,
          }),
        ),
      ),
    );
  });

  yield* Stream.fromQueue(persistence).pipe(
    Stream.debounce("500 millis"),
    Stream.runForEach(persist),
    Effect.forkScoped,
  );

  const setSynchronizing = SubscriptionRef.update(state, (current) => ({
    ...current,
    status: "synchronizing" as const,
    error: Option.none(),
  }));
  const setReady = SubscriptionRef.update(state, (current) =>
    current.status === "live" || current.status === "deleted"
      ? current
      : {
          ...current,
          status: "synchronizing" as const,
          error: Option.none(),
        },
  );
  const setDisconnected = SubscriptionRef.update(state, (current) => ({
    ...current,
    status: current.status === "deleted" ? current.status : statusWithoutLiveData(current.data),
  }));
  const setStreamError = (cause: Cause.Cause<unknown>) =>
    SubscriptionRef.update(state, (current) => ({
      ...current,
      status: current.status === "deleted" ? current.status : statusWithoutLiveData(current.data),
      error: Option.some(formatThreadError(cause)),
    }));

  const setThread = Effect.fn("EnvironmentThreadState.setThread")(function* (
    thread: OrchestrationThread,
  ) {
    persistedThread = Option.some(thread);
    yield* SubscriptionRef.set(state, {
      data: Option.some(overlaidThread(thread)),
      status: "live",
      error: Option.none(),
    });
    // Persist the thread together with the sequence it reflects so the next warm
    // cache can resume from exactly here. Always the un-overlaid thread: the
    // overlay's characters arrive again via the persisted events after the
    // cached sequence.
    const snapshotSequence = yield* SubscriptionRef.get(lastSequence);
    yield* Queue.offer(persistence, { snapshotSequence, thread });
  });

  // Re-render the current persisted thread with overlays, without touching the
  // resume cursor, the persistence queue, or the connection status.
  const refreshOverlaidView = SubscriptionRef.update(state, (current) =>
    current.status === "deleted" || Option.isNone(persistedThread)
      ? current
      : { ...current, data: Option.some(overlaidThread(persistedThread.value)) },
  );

  const setDeleted = Effect.fn("EnvironmentThreadState.setDeleted")(function* () {
    persistedThread = Option.none();
    overlays.clear();
    yield* SubscriptionRef.set(state, {
      data: Option.none(),
      status: "deleted",
      error: Option.none(),
    });
    yield* cache.removeThread(environmentId, threadId).pipe(
      Effect.catch((error) =>
        Effect.logWarning("Could not remove the cached thread.").pipe(
          Effect.annotateLogs({
            environmentId,
            threadId,
            error: error.message,
          }),
        ),
      ),
    );
  });

  const applyItem = Effect.fn("EnvironmentThreadState.applyItem")(function* (
    item: OrchestrationThreadStreamItem,
  ) {
    if (item.kind === "snapshot") {
      // The snapshot may already contain any overlaid text; drop overlays
      // rather than risk rendering them twice. The next flush self-heals.
      overlays.clear();
      yield* SubscriptionRef.set(lastSequence, item.snapshot.snapshotSequence);
      yield* setThread(item.snapshot.thread);
      return;
    }

    if (item.kind === "ephemeral-delta") {
      // Best-effort live text. Never advances lastSequence and is only
      // applied when contiguous with what we already render; anything
      // dropped here arrives again in the coalesced persisted delta.
      if (Option.isNone(persistedThread)) {
        return;
      }
      const persistedMessage = persistedThread.value.messages.find(
        (entry) => entry.id === item.messageId,
      );
      if (persistedMessage && !persistedMessage.streaming) {
        return;
      }
      const persistedLength = persistedMessage?.text.length ?? 0;
      const overlay = overlays.get(item.messageId);
      const overlayLength = overlay?.text.length ?? 0;
      if (item.offset !== persistedLength + overlayLength) {
        return;
      }
      overlays.set(item.messageId, {
        turnId: item.turnId,
        createdAt: overlay?.createdAt ?? item.createdAt,
        text: `${overlay?.text ?? ""}${item.delta}`,
      });
      yield* refreshOverlaidView;
      return;
    }

    const sequence = yield* SubscriptionRef.get(lastSequence);
    if (item.event.sequence <= sequence) {
      return;
    }
    yield* SubscriptionRef.set(lastSequence, item.event.sequence);

    // Reconcile overlays against the persisted delta that carries the same
    // characters: trim the flushed prefix from the overlay (streaming) or
    // drop it entirely (message finalized).
    if (item.event.type === "thread.message-sent") {
      const overlay = overlays.get(item.event.payload.messageId);
      if (overlay !== undefined) {
        if (item.event.payload.streaming) {
          const remaining = overlay.text.slice(item.event.payload.text.length);
          if (remaining.length === 0) {
            overlays.delete(item.event.payload.messageId);
          } else {
            overlays.set(item.event.payload.messageId, { ...overlay, text: remaining });
          }
        } else {
          overlays.delete(item.event.payload.messageId);
        }
      }
    }

    if (Option.isNone(persistedThread)) {
      if (item.event.type === "thread.deleted") {
        yield* setDeleted();
      }
      return;
    }
    const result = applyThreadDetailEvent(persistedThread.value, item.event);
    if (result.kind === "updated") {
      yield* setThread(result.thread);
    } else if (result.kind === "deleted") {
      yield* setDeleted();
    }
  });

  yield* SubscriptionRef.changes(supervisor.state).pipe(
    Stream.runForEach((connectionState) => {
      switch (connectionProjectionPhase(connectionState)) {
        case "synchronizing":
          return setSynchronizing;
        case "disconnected":
          return setDisconnected;
        case "ready":
          return setReady;
      }
    }),
    Effect.forkScoped,
  );

  yield* setSynchronizing;
  yield* Effect.forkScoped(
    Effect.gen(function* () {
      // Establish the base snapshot to resume from, minimizing bytes over the
      // wire:
      // - Warm cache: reuse the cached snapshot (zero network) and resume via
      //   `afterSequence` so we only receive events since the cached sequence.
      // - Cold cache: load the full snapshot over HTTP (gzip-compressible, and
      //   off the socket), then resume via `afterSequence`.
      // If no base can be established we fall back to the socket-embedded
      // snapshot so the thread still synchronizes. Overlapping/replayed events
      // are deduped by sequence in applyItem.
      const base = Option.isSome(cached)
        ? cached
        : yield* Effect.gen(function* () {
            // Cold cache only: wait for a prepared connection so we can
            // authenticate the HTTP request; this mirrors the socket path, which
            // likewise waits for a live session.
            const prepared = yield* SubscriptionRef.changes(supervisor.prepared).pipe(
              Stream.filter(Option.isSome),
              Stream.map((current) => current.value),
              Stream.runHead,
            );
            return Option.isSome(prepared)
              ? yield* snapshotLoader.load(prepared.value, threadId)
              : Option.none<OrchestrationThreadDetailSnapshot>();
          });

      if (Option.isSome(base)) {
        yield* applyItem({ kind: "snapshot", snapshot: base.value });
      }

      const subscribeInput = Option.match(base, {
        onNone: () => ({ threadId }),
        onSome: (snapshot) => ({ threadId, afterSequence: snapshot.snapshotSequence }),
      });

      yield* subscribe(ORCHESTRATION_WS_METHODS.subscribeThread, subscribeInput, {
        onExpectedFailure: setStreamError,
        retryExpectedFailureAfter: "250 millis",
      }).pipe(Stream.runForEach(applyItem));
    }),
  );

  yield* Effect.addFinalizer(() =>
    Effect.all([SubscriptionRef.get(state), SubscriptionRef.get(lastSequence)]).pipe(
      Effect.flatMap(([current, snapshotSequence]) =>
        Option.match(current.data, {
          onNone: () => Effect.void,
          onSome: (thread) => persist({ snapshotSequence, thread }),
        }),
      ),
    ),
  );

  return state;
});

export function threadStateChanges(environmentId: EnvironmentIdType, threadId: ThreadIdType) {
  return followStreamInEnvironment(
    environmentId,
    Stream.unwrap(makeEnvironmentThreadState(threadId).pipe(Effect.map(SubscriptionRef.changes))),
  );
}

export function createEnvironmentThreadStateAtoms<R, E>(
  runtime: Atom.AtomRuntime<
    EnvironmentRegistry | EnvironmentCacheStore | ThreadSnapshotLoader | R,
    E
  >,
) {
  const family = Atom.family((key: string) => {
    const { environmentId, threadId } = parseThreadKey(key);
    return runtime
      .atom(threadStateChanges(environmentId, threadId), {
        initialValue: EMPTY_ENVIRONMENT_THREAD_STATE,
      })
      .pipe(
        Atom.setIdleTTL(THREAD_STATE_IDLE_TTL_MS),
        Atom.withLabel(`environment-thread-state:${key}`),
      );
  });

  return {
    stateAtom: (environmentId: EnvironmentIdType, threadId: ThreadIdType) =>
      family(threadKey({ environmentId, threadId })),
  };
}

export * from "./archivedThreads.ts";
export * from "./checkpointDiff.ts";
export * from "./threadSnapshotHttp.ts";
export * from "./composerPathSearch.ts";
export * from "./threadCommands.ts";
export * from "./threadDetail.ts";
export * from "./threadReducer.ts";
export * from "./threadShell.ts";
export * from "./threadState.ts";
