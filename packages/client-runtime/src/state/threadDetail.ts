import type {
  MessageId,
  OrchestrationCheckpointSummary,
  OrchestrationLatestTurn,
  OrchestrationMessage,
  OrchestrationProposedPlan,
  OrchestrationSession,
  OrchestrationThread,
  OrchestrationThreadActivity,
  ScopedThreadRef,
} from "@t3tools/contracts";
import * as Option from "effect/Option";
import { AsyncResult, Atom } from "effect/unstable/reactivity";

import type { EnvironmentThread, EnvironmentThreadShell } from "./models.ts";
import { scopeThread } from "./models.ts";
import { EMPTY_ENVIRONMENT_THREAD_STATE, type EnvironmentThreadState } from "./threadState.ts";
import { parseThreadKey, threadKey } from "./entities.ts";
import { THREAD_STATE_IDLE_TTL_MS } from "./threadRetention.ts";

const EMPTY_MESSAGES: ReadonlyArray<OrchestrationMessage> = Object.freeze([]);
const EMPTY_ACTIVITIES: ReadonlyArray<OrchestrationThreadActivity> = Object.freeze([]);
const EMPTY_PROPOSED_PLANS: ReadonlyArray<OrchestrationProposedPlan> = Object.freeze([]);
const EMPTY_CHECKPOINTS: ReadonlyArray<OrchestrationCheckpointSummary> = Object.freeze([]);

function sourceProposedPlansEqual(
  a: OrchestrationLatestTurn["sourceProposedPlan"],
  b: OrchestrationLatestTurn["sourceProposedPlan"],
): boolean {
  if (a === b) {
    return true;
  }
  if (a === undefined || b === undefined) {
    return false;
  }
  return a.threadId === b.threadId && a.planId === b.planId;
}

/**
 * Streaming text deltas rebuild `latestTurn` with identical contents on every
 * commit (see threadReducer's `thread.message-sent` case), so slice consumers
 * compare by value to keep a referentially stable turn while it streams.
 */
function latestTurnsEqual(
  a: OrchestrationLatestTurn | null,
  b: OrchestrationLatestTurn | null,
): boolean {
  if (a === b) {
    return true;
  }
  if (a === null || b === null) {
    return false;
  }
  return (
    a.turnId === b.turnId &&
    a.state === b.state &&
    a.requestedAt === b.requestedAt &&
    a.startedAt === b.startedAt &&
    a.completedAt === b.completedAt &&
    a.assistantMessageId === b.assistantMessageId &&
    sourceProposedPlansEqual(a.sourceProposedPlan, b.sourceProposedPlan)
  );
}

function checkpointSummariesEqual(
  a: OrchestrationCheckpointSummary,
  b: OrchestrationCheckpointSummary,
): boolean {
  if (a === b) {
    return true;
  }
  return (
    a.turnId === b.turnId &&
    a.checkpointTurnCount === b.checkpointTurnCount &&
    a.checkpointRef === b.checkpointRef &&
    a.status === b.status &&
    a.files === b.files &&
    a.assistantMessageId === b.assistantMessageId &&
    a.completedAt === b.completedAt
  );
}

/**
 * Streaming assistant deltas re-map the checkpoint list on every commit
 * (assistant-message rebinding), minting a new array whose elements are
 * value-equal. Compare element-wise so the checkpoints slice stays
 * referentially stable during a pure text delta.
 */
function checkpointListsEqual(
  a: ReadonlyArray<OrchestrationCheckpointSummary>,
  b: ReadonlyArray<OrchestrationCheckpointSummary>,
): boolean {
  if (a === b) {
    return true;
  }
  if (a.length !== b.length) {
    return false;
  }
  for (let index = 0; index < a.length; index += 1) {
    const left = a[index];
    const right = b[index];
    if (left === undefined || right === undefined || !checkpointSummariesEqual(left, right)) {
      return false;
    }
  }
  return true;
}

/**
 * Fields intentionally excluded from the sans-messages equality check:
 * - `messages`: this slice exists to hide them.
 * - `updatedAt`: bumped on every streaming delta; consumers that need a live
 *   `updatedAt` must read it from the thread shell (which the merge in
 *   {@link mergeEnvironmentThread} makes authoritative anyway).
 */
const DETAIL_SANS_MESSAGES_IGNORED_KEYS: ReadonlySet<string> = new Set(["messages", "updatedAt"]);

function detailsSansMessagesEqual(a: EnvironmentThread, b: EnvironmentThread): boolean {
  const aKeys = Object.keys(a) as ReadonlyArray<keyof EnvironmentThread>;
  const bKeys = Object.keys(b);
  if (aKeys.length !== bKeys.length) {
    return false;
  }
  for (const key of aKeys) {
    if (DETAIL_SANS_MESSAGES_IGNORED_KEYS.has(key)) {
      continue;
    }
    if (!Object.is(a[key], b[key])) {
      return false;
    }
  }
  return true;
}

/**
 * Combine detail-only collections with the shell's authoritative thread metadata.
 *
 * Shell and detail subscriptions are intentionally independent. A cached detail can
 * therefore briefly outlive a newer shell snapshot after reconnecting. Workspace
 * consumers must use the shell branch/worktree/project fields so they do not target
 * a stale checkout while retaining messages, activities, plans, and checkpoints
 * from the detail subscription.
 */
export function mergeEnvironmentThread(
  detail: EnvironmentThread | null,
  shell: EnvironmentThreadShell | null,
): EnvironmentThread | null {
  if (detail === null || shell === null) {
    return detail;
  }
  if (detail.environmentId !== shell.environmentId || detail.id !== shell.id) {
    return detail;
  }

  return {
    ...detail,
    environmentId: shell.environmentId,
    id: shell.id,
    projectId: shell.projectId,
    title: shell.title,
    modelSelection: shell.modelSelection,
    runtimeMode: shell.runtimeMode,
    interactionMode: shell.interactionMode,
    branch: shell.branch,
    worktreePath: shell.worktreePath,
    additionalRoots: shell.additionalRoots,
    ...(shell.resolvedAdditionalRoots !== undefined
      ? { resolvedAdditionalRoots: shell.resolvedAdditionalRoots }
      : {}),
    latestTurn: shell.latestTurn,
    createdAt: shell.createdAt,
    updatedAt: shell.updatedAt,
    archivedAt: shell.archivedAt,
    settledOverride: shell.settledOverride,
    settledAt: shell.settledAt,
    snoozedUntil: shell.snoozedUntil,
    snoozedAt: shell.snoozedAt,
    session: shell.session,
  };
}

export function createEnvironmentThreadDetailAtoms<E>(
  threadStateAtom: (
    environmentId: ScopedThreadRef["environmentId"],
    threadId: ScopedThreadRef["threadId"],
  ) => Atom.Atom<AsyncResult.AsyncResult<EnvironmentThreadState, E>>,
) {
  const threadStateValueAtomFamily = Atom.family((key: string) => {
    const ref = parseThreadKey(key);
    return Atom.make((get) =>
      Option.getOrElse(
        AsyncResult.value(get(threadStateAtom(ref.environmentId, ref.threadId))),
        () => EMPTY_ENVIRONMENT_THREAD_STATE,
      ),
    ).pipe(
      Atom.setIdleTTL(THREAD_STATE_IDLE_TTL_MS),
      Atom.withLabel(`environment-thread-state-value:${key}`),
    );
  });

  const threadDetailAtomFamily = Atom.family((key: string) => {
    const ref = parseThreadKey(key);
    let previousSource: OrchestrationThread | null = null;
    let previousValue: EnvironmentThread | null = null;
    return Atom.make((get) => {
      const source = Option.getOrNull(get(threadStateValueAtomFamily(key)).data);
      if (source === previousSource) {
        return previousValue;
      }
      previousSource = source;
      previousValue = source === null ? null : scopeThread(ref.environmentId, source);
      return previousValue;
    }).pipe(
      Atom.setIdleTTL(THREAD_STATE_IDLE_TTL_MS),
      Atom.withLabel(`environment-thread-detail:${key}`),
    );
  });

  const threadStatusAtomFamily = Atom.family((key: string) =>
    Atom.make((get) => get(threadStateValueAtomFamily(key)).status).pipe(
      Atom.setIdleTTL(THREAD_STATE_IDLE_TTL_MS),
      Atom.withLabel(`environment-thread-status:${key}`),
    ),
  );

  const threadErrorAtomFamily = Atom.family((key: string) =>
    Atom.make((get) => Option.getOrNull(get(threadStateValueAtomFamily(key)).error)).pipe(
      Atom.setIdleTTL(THREAD_STATE_IDLE_TTL_MS),
      Atom.withLabel(`environment-thread-error:${key}`),
    ),
  );

  const threadMessagesAtomFamily = Atom.family((key: string) =>
    Atom.make(
      (get): ReadonlyArray<OrchestrationMessage> =>
        get(threadDetailAtomFamily(key))?.messages ?? EMPTY_MESSAGES,
    ).pipe(
      Atom.setIdleTTL(THREAD_STATE_IDLE_TTL_MS),
      Atom.withLabel(`environment-thread-messages:${key}`),
    ),
  );

  const threadActivitiesAtomFamily = Atom.family((key: string) =>
    Atom.make(
      (get): ReadonlyArray<OrchestrationThreadActivity> =>
        get(threadDetailAtomFamily(key))?.activities ?? EMPTY_ACTIVITIES,
    ).pipe(
      Atom.setIdleTTL(THREAD_STATE_IDLE_TTL_MS),
      Atom.withLabel(`environment-thread-activities:${key}`),
    ),
  );

  const threadProposedPlansAtomFamily = Atom.family((key: string) =>
    Atom.make(
      (get): ReadonlyArray<OrchestrationProposedPlan> =>
        get(threadDetailAtomFamily(key))?.proposedPlans ?? EMPTY_PROPOSED_PLANS,
    ).pipe(
      Atom.setIdleTTL(THREAD_STATE_IDLE_TTL_MS),
      Atom.withLabel(`environment-thread-proposed-plans:${key}`),
    ),
  );

  const threadCheckpointsAtomFamily = Atom.family((key: string) => {
    let previous: ReadonlyArray<OrchestrationCheckpointSummary> = EMPTY_CHECKPOINTS;
    return Atom.make((get): ReadonlyArray<OrchestrationCheckpointSummary> => {
      const next = get(threadDetailAtomFamily(key))?.checkpoints ?? EMPTY_CHECKPOINTS;
      if (checkpointListsEqual(previous, next)) {
        return previous;
      }
      previous = next;
      return next;
    }).pipe(
      Atom.setIdleTTL(THREAD_STATE_IDLE_TTL_MS),
      Atom.withLabel(`environment-thread-checkpoints:${key}`),
    );
  });

  const threadSessionAtomFamily = Atom.family((key: string) =>
    Atom.make(
      (get): OrchestrationSession | null => get(threadDetailAtomFamily(key))?.session ?? null,
    ).pipe(
      Atom.setIdleTTL(THREAD_STATE_IDLE_TTL_MS),
      Atom.withLabel(`environment-thread-session:${key}`),
    ),
  );

  const threadLatestTurnAtomFamily = Atom.family((key: string) => {
    let previous: OrchestrationLatestTurn | null = null;
    return Atom.make((get): OrchestrationLatestTurn | null => {
      const next = get(threadDetailAtomFamily(key))?.latestTurn ?? null;
      if (latestTurnsEqual(previous, next)) {
        return previous;
      }
      previous = next;
      return next;
    }).pipe(
      Atom.setIdleTTL(THREAD_STATE_IDLE_TTL_MS),
      Atom.withLabel(`environment-thread-latest-turn:${key}`),
    );
  });

  const threadHasMessagesAtomFamily = Atom.family((key: string) =>
    Atom.make((get): boolean => (get(threadDetailAtomFamily(key))?.messages.length ?? 0) > 0).pipe(
      Atom.setIdleTTL(THREAD_STATE_IDLE_TTL_MS),
      Atom.withLabel(`environment-thread-has-messages:${key}`),
    ),
  );

  const threadLatestUserMessageIdAtomFamily = Atom.family((key: string) =>
    Atom.make((get): MessageId | null => {
      const messages = get(threadDetailAtomFamily(key))?.messages;
      if (messages === undefined) {
        return null;
      }
      for (let index = messages.length - 1; index >= 0; index -= 1) {
        const message = messages[index];
        if (message !== undefined && message.role === "user") {
          return message.id;
        }
      }
      return null;
    }).pipe(
      Atom.setIdleTTL(THREAD_STATE_IDLE_TTL_MS),
      Atom.withLabel(`environment-thread-latest-user-message-id:${key}`),
    ),
  );

  /**
   * The thread detail with `messages` hidden (always {@link EMPTY_MESSAGES})
   * and per-streaming-delta identity churn removed: the value only changes
   * when a non-message field materially changes, so subscribers skip the
   * ~28ms streaming commits entirely.
   *
   * Caveat: `updatedAt` is carried over from the last material change and can
   * lag during a streaming turn — read `updatedAt` from the thread shell
   * (authoritative after {@link mergeEnvironmentThread}) when freshness
   * matters.
   */
  const threadDetailSansMessagesAtomFamily = Atom.family((key: string) => {
    let previous: EnvironmentThread | null = null;
    return Atom.make((get): EnvironmentThread | null => {
      const detail = get(threadDetailAtomFamily(key));
      if (detail === null) {
        previous = null;
        return null;
      }
      const next: EnvironmentThread = {
        ...detail,
        messages: EMPTY_MESSAGES,
        latestTurn: get(threadLatestTurnAtomFamily(key)),
        checkpoints: get(threadCheckpointsAtomFamily(key)),
      };
      if (previous !== null && detailsSansMessagesEqual(previous, next)) {
        return previous;
      }
      previous = next;
      return next;
    }).pipe(
      Atom.setIdleTTL(THREAD_STATE_IDLE_TTL_MS),
      Atom.withLabel(`environment-thread-detail-sans-messages:${key}`),
    );
  });

  return {
    stateAtom: (ref: ScopedThreadRef) => threadStateValueAtomFamily(threadKey(ref)),
    detailAtom: (ref: ScopedThreadRef) => threadDetailAtomFamily(threadKey(ref)),
    statusAtom: (ref: ScopedThreadRef) => threadStatusAtomFamily(threadKey(ref)),
    errorAtom: (ref: ScopedThreadRef) => threadErrorAtomFamily(threadKey(ref)),
    messagesAtom: (ref: ScopedThreadRef) => threadMessagesAtomFamily(threadKey(ref)),
    activitiesAtom: (ref: ScopedThreadRef) => threadActivitiesAtomFamily(threadKey(ref)),
    proposedPlansAtom: (ref: ScopedThreadRef) => threadProposedPlansAtomFamily(threadKey(ref)),
    checkpointsAtom: (ref: ScopedThreadRef) => threadCheckpointsAtomFamily(threadKey(ref)),
    sessionAtom: (ref: ScopedThreadRef) => threadSessionAtomFamily(threadKey(ref)),
    latestTurnAtom: (ref: ScopedThreadRef) => threadLatestTurnAtomFamily(threadKey(ref)),
    hasMessagesAtom: (ref: ScopedThreadRef) => threadHasMessagesAtomFamily(threadKey(ref)),
    latestUserMessageIdAtom: (ref: ScopedThreadRef) =>
      threadLatestUserMessageIdAtomFamily(threadKey(ref)),
    detailSansMessagesAtom: (ref: ScopedThreadRef) =>
      threadDetailSansMessagesAtomFamily(threadKey(ref)),
  };
}
