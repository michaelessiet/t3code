import * as Schema from "effect/Schema";

/**
 * Status of the Claude Code CLI executable the server would spawn.
 *
 * `version` is the user-facing Claude Code CLI version pinned by the installed
 * @anthropic-ai/claude-agent-sdk (its manifest.json); the managed installer
 * downloads exactly that build. `binarySizeBytes` is the installed size of the
 * binary from the same manifest (the compressed download is smaller).
 */
export const ClaudeBinaryStatusSchema = Schema.Union([
  Schema.Struct({
    status: Schema.Literal("available"),
    executablePath: Schema.String,
    source: Schema.Literals(["explicit", "override", "managed", "path"]),
    version: Schema.String,
  }),
  Schema.Struct({
    status: Schema.Literal("missing"),
    version: Schema.String,
    binarySizeBytes: Schema.Number,
  }),
  Schema.Struct({
    status: Schema.Literal("unsupported"),
    platform: Schema.String,
    arch: Schema.String,
    version: Schema.String,
  }),
]);
export type ClaudeBinaryStatus = typeof ClaudeBinaryStatusSchema.Type;

export const ClaudeBinaryInstallProgressStageSchema = Schema.Literals([
  "checking",
  "waiting_for_lock",
  "downloading",
  "verifying",
  "installing",
  "validating",
  "activating",
]);
export type ClaudeBinaryInstallProgressStage = typeof ClaudeBinaryInstallProgressStageSchema.Type;

export const ClaudeBinaryInstallProgressEventSchema = Schema.Union([
  Schema.Struct({
    type: Schema.Literal("progress"),
    stage: ClaudeBinaryInstallProgressStageSchema,
    bytesDownloaded: Schema.optional(Schema.Number),
    totalBytes: Schema.optional(Schema.Number),
  }),
  Schema.Struct({
    type: Schema.Literal("complete"),
    status: ClaudeBinaryStatusSchema,
  }),
]);
export type ClaudeBinaryInstallProgressEvent = typeof ClaudeBinaryInstallProgressEventSchema.Type;

export const ClaudeBinaryInstallFailureReasonSchema = Schema.Literals([
  "download_failed",
  "invalid_checksum",
  "install_locked",
  "override_missing",
  "unsupported_platform",
  "validation_failed",
  "write_failed",
]);
export type ClaudeBinaryInstallFailureReason = typeof ClaudeBinaryInstallFailureReasonSchema.Type;

export class ClaudeBinaryInstallFailedError extends Schema.TaggedErrorClass<ClaudeBinaryInstallFailedError>()(
  "ClaudeBinaryInstallFailedError",
  {
    reason: ClaudeBinaryInstallFailureReasonSchema,
    message: Schema.String,
  },
) {}
