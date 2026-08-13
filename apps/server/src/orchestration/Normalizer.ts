import * as DateTime from "effect/DateTime";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Path from "effect/Path";
import {
  type ClientOrchestrationCommand,
  type IsoDateTime,
  type OrchestrationCommand,
  OrchestrationDispatchCommandError,
  PROVIDER_SEND_TURN_MAX_FILE_BYTES,
  PROVIDER_SEND_TURN_MAX_IMAGE_BYTES,
  type WorkspaceRootRef,
} from "@t3tools/contracts";

import { createAttachmentId, resolveAttachmentPath } from "../attachmentStore.ts";
import { ServerConfig } from "../config.ts";
import { parseBase64DataUrl } from "../imageMime.ts";
import * as WorkspacePaths from "../workspace/WorkspacePaths.ts";

export const canonicalizeClientCommandTimestamps = (
  command: ClientOrchestrationCommand,
  receivedAt: IsoDateTime,
): ClientOrchestrationCommand => {
  const canonicalCommand =
    "createdAt" in command
      ? {
          ...command,
          createdAt: receivedAt,
        }
      : command;

  if (canonicalCommand.type !== "thread.turn.start" || !canonicalCommand.bootstrap?.createThread) {
    return canonicalCommand;
  }

  return {
    ...canonicalCommand,
    bootstrap: {
      ...canonicalCommand.bootstrap,
      createThread: {
        ...canonicalCommand.bootstrap.createThread,
        createdAt: receivedAt,
      },
    },
  };
};

export const normalizeDispatchCommand = (command: ClientOrchestrationCommand) =>
  Effect.gen(function* () {
    const receivedAt = DateTime.formatIso(yield* DateTime.now);
    const canonicalCommand = canonicalizeClientCommandTimestamps(command, receivedAt);
    const fileSystem = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    const serverConfig = yield* ServerConfig;
    const workspacePaths = yield* WorkspacePaths.WorkspacePaths;

    const normalizeProjectWorkspaceRoot = (workspaceRoot: string) =>
      workspacePaths.normalizeWorkspaceRoot(workspaceRoot).pipe(
        Effect.mapError(
          (cause) =>
            new OrchestrationDispatchCommandError({
              message: cause.message,
            }),
        ),
      );

    const normalizeProjectWorkspaceRootForCreate = (
      workspaceRoot: string,
      createIfMissing: boolean | undefined,
    ) =>
      workspacePaths
        .normalizeWorkspaceRoot(workspaceRoot, {
          createIfMissing: createIfMissing === true,
        })
        .pipe(
          Effect.mapError(
            (cause) =>
              new OrchestrationDispatchCommandError({
                message: cause.message,
              }),
          ),
        );

    // Additional-root path refs are validated the same way as a project's
    // primary workspace root: the directory must exist and the stored value
    // is the canonical form. Project refs carry no path and pass through.
    const normalizeAdditionalRoots = (
      roots: ReadonlyArray<WorkspaceRootRef> | undefined,
    ): Effect.Effect<
      ReadonlyArray<WorkspaceRootRef> | undefined,
      OrchestrationDispatchCommandError
    > =>
      roots === undefined
        ? Effect.succeed(undefined)
        : Effect.forEach(
            roots,
            (root): Effect.Effect<WorkspaceRootRef, OrchestrationDispatchCommandError> =>
              root.kind === "path"
                ? normalizeProjectWorkspaceRoot(root.path).pipe(
                    Effect.map((normalized) => ({ kind: "path" as const, path: normalized })),
                  )
                : Effect.succeed(root),
            { concurrency: 1 },
          );

    const withNormalizedAdditionalRoots = <
      T extends { readonly additionalRoots?: ReadonlyArray<WorkspaceRootRef> | undefined },
    >(
      command: T,
    ): Effect.Effect<T, OrchestrationDispatchCommandError> =>
      Effect.gen(function* () {
        const additionalRoots = yield* normalizeAdditionalRoots(command.additionalRoots);
        // The spread only rewrites `additionalRoots` with its normalized
        // form, so the result is still a `T`; the assertion keeps the
        // command union intact for callers.
        return additionalRoots === undefined ? command : ({ ...command, additionalRoots } as T);
      });

    if (canonicalCommand.type === "project.create") {
      return (yield* withNormalizedAdditionalRoots({
        ...canonicalCommand,
        workspaceRoot: yield* normalizeProjectWorkspaceRootForCreate(
          canonicalCommand.workspaceRoot,
          canonicalCommand.createWorkspaceRootIfMissing,
        ),
        createWorkspaceRootIfMissing: canonicalCommand.createWorkspaceRootIfMissing === true,
      })) satisfies OrchestrationCommand;
    }

    if (canonicalCommand.type === "project.meta.update") {
      const workspaceRoot =
        canonicalCommand.workspaceRoot !== undefined
          ? yield* normalizeProjectWorkspaceRoot(canonicalCommand.workspaceRoot)
          : undefined;
      return (yield* withNormalizedAdditionalRoots({
        ...canonicalCommand,
        ...(workspaceRoot !== undefined ? { workspaceRoot } : {}),
      })) satisfies OrchestrationCommand;
    }

    if (
      canonicalCommand.type === "thread.create" ||
      canonicalCommand.type === "thread.meta.update"
    ) {
      return (yield* withNormalizedAdditionalRoots(
        canonicalCommand,
      )) satisfies OrchestrationCommand;
    }

    if (canonicalCommand.type !== "thread.turn.start") {
      return canonicalCommand as OrchestrationCommand;
    }

    const normalizedBootstrapCreateThread = canonicalCommand.bootstrap?.createThread
      ? yield* withNormalizedAdditionalRoots(canonicalCommand.bootstrap.createThread)
      : undefined;

    const normalizedAttachments = yield* Effect.forEach(
      canonicalCommand.message.attachments,
      (attachment) =>
        Effect.gen(function* () {
          const parsed = parseBase64DataUrl(attachment.dataUrl);
          if (!parsed || (attachment.type === "image" && !parsed.mimeType.startsWith("image/"))) {
            return yield* new OrchestrationDispatchCommandError({
              message: `Invalid attachment payload for '${attachment.name}'.`,
            });
          }

          const maxBytes =
            attachment.type === "image"
              ? PROVIDER_SEND_TURN_MAX_IMAGE_BYTES
              : PROVIDER_SEND_TURN_MAX_FILE_BYTES;
          const bytes = Buffer.from(parsed.base64, "base64");
          if (bytes.byteLength === 0 || bytes.byteLength > maxBytes) {
            return yield* new OrchestrationDispatchCommandError({
              message: `Attachment '${attachment.name}' is empty or too large.`,
            });
          }

          const attachmentId = createAttachmentId(canonicalCommand.threadId);
          if (!attachmentId) {
            return yield* new OrchestrationDispatchCommandError({
              message: "Failed to create a safe attachment id.",
            });
          }

          // The client's declared mimeType is more reliable than the data-URL
          // header for non-image files (browsers report source files as
          // `application/octet-stream` in FileReader output).
          const persistedAttachment = {
            type: attachment.type,
            id: attachmentId,
            name: attachment.name,
            mimeType:
              attachment.type === "image"
                ? parsed.mimeType.toLowerCase()
                : attachment.mimeType.toLowerCase(),
            sizeBytes: bytes.byteLength,
          };

          const attachmentPath = resolveAttachmentPath({
            attachmentsDir: serverConfig.attachmentsDir,
            attachment: persistedAttachment,
          });
          if (!attachmentPath) {
            return yield* new OrchestrationDispatchCommandError({
              message: `Failed to resolve persisted path for '${attachment.name}'.`,
            });
          }

          yield* fileSystem.makeDirectory(path.dirname(attachmentPath), { recursive: true }).pipe(
            Effect.mapError(
              () =>
                new OrchestrationDispatchCommandError({
                  message: `Failed to create attachment directory for '${attachment.name}'.`,
                }),
            ),
          );
          yield* fileSystem.writeFile(attachmentPath, bytes).pipe(
            Effect.mapError(
              () =>
                new OrchestrationDispatchCommandError({
                  message: `Failed to persist attachment '${attachment.name}'.`,
                }),
            ),
          );

          return persistedAttachment;
        }),
      { concurrency: 1 },
    );

    return {
      ...canonicalCommand,
      ...(normalizedBootstrapCreateThread && canonicalCommand.bootstrap
        ? {
            bootstrap: {
              ...canonicalCommand.bootstrap,
              createThread: normalizedBootstrapCreateThread,
            },
          }
        : {}),
      message: {
        ...canonicalCommand.message,
        attachments: normalizedAttachments,
      },
    } satisfies OrchestrationCommand;
  });
