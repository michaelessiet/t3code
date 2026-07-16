import type { OrchestrationAssistantEphemeralDelta, ThreadId } from "@t3tools/contracts";
import * as Context from "effect/Context";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as PubSub from "effect/PubSub";
import * as Stream from "effect/Stream";

/**
 * In-process fan-out for live assistant text deltas, published by ingestion
 * before any persistence work. Websocket thread subscriptions merge this into
 * their live stream so streamed text reaches clients without waiting on the
 * command/SQLite pipeline. Delivery is best-effort: the coalesced persisted
 * delta that follows carries the same characters, so dropped or unobserved
 * ephemeral deltas self-heal.
 */
export class AssistantStreamBus extends Context.Service<
  AssistantStreamBus,
  {
    readonly publish: (delta: OrchestrationAssistantEphemeralDelta) => Effect.Effect<void>;
    readonly subscribe: (threadId: ThreadId) => Stream.Stream<OrchestrationAssistantEphemeralDelta>;
  }
>()("t3/orchestration/AssistantStreamBus") {}

export const layer = Layer.effect(
  AssistantStreamBus,
  Effect.gen(function* () {
    const pubsub = yield* PubSub.unbounded<OrchestrationAssistantEphemeralDelta>();

    return AssistantStreamBus.of({
      publish: (delta) => PubSub.publish(pubsub, delta).pipe(Effect.asVoid),
      subscribe: (threadId) =>
        Stream.fromPubSub(pubsub).pipe(Stream.filter((delta) => delta.threadId === threadId)),
    });
  }),
);
