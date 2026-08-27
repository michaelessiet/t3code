// GENERATED FILE — DO NOT EDIT.
// Source: contracts.gen.json (exported from packages/contracts by
// scripts/vitre/export-contracts.ts). Regenerate with:
//   node scripts/vitre/export-contracts.ts
//   node scripts/vitre/export-fixtures.ts
//   cargo run -p vitre-contracts-gen && cargo fmt -p vitre-contracts
#![allow(clippy::large_enum_variant)]

use crate::support::{DurationMillis, EffectOption};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Version of the contracts.gen.json this module was generated from.
pub const CONTRACTS_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ApprovalRequestId(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AssetAccessError {
    AssetWorkspaceContextNotFoundError(AssetWorkspaceContextNotFoundError),
    AssetWorkspaceContextResolutionError(AssetWorkspaceContextResolutionError),
    AssetWorkspaceRootNormalizationError(AssetWorkspaceRootNormalizationError),
    AssetWorkspacePathValidationError(AssetWorkspacePathValidationError),
    AssetPreviewTypeValidationError(AssetPreviewTypeValidationError),
    AssetWorkspaceAssetInspectionError(AssetWorkspaceAssetInspectionError),
    AssetWorkspaceAssetNotFoundError(AssetWorkspaceAssetNotFoundError),
    AssetWorkspaceResolutionError(AssetWorkspaceResolutionError),
    AssetAttachmentNotFoundError(AssetAttachmentNotFoundError),
    AssetProjectFaviconResolutionError(AssetProjectFaviconResolutionError),
    AssetProjectFaviconInspectionError(AssetProjectFaviconInspectionError),
    AssetProjectFaviconNotFoundError(AssetProjectFaviconNotFoundError),
    AssetSigningKeyLoadError(AssetSigningKeyLoadError),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetAttachmentNotFoundError {
    #[serde(rename = "_tag")]
    pub tag: AssetAttachmentNotFoundErrorTag,
    pub resource: AssetResource,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AssetAttachmentNotFoundErrorTag {
    #[default]
    #[serde(rename = "AssetAttachmentNotFoundError")]
    AssetAttachmentNotFoundError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetCreateUrlInput {
    pub resource: AssetResource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetCreateUrlResult {
    #[serde(rename = "expiresAt")]
    pub expires_at: DurationMillis,
    #[serde(rename = "relativeUrl")]
    pub relative_url: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetPreviewTypeValidationError {
    #[serde(rename = "_tag")]
    pub tag: AssetPreviewTypeValidationErrorTag,
    pub resource: AssetResource,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AssetPreviewTypeValidationErrorTag {
    #[default]
    #[serde(rename = "AssetPreviewTypeValidationError")]
    AssetPreviewTypeValidationError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetProjectFaviconInspectionError {
    #[serde(rename = "_tag")]
    pub tag: AssetProjectFaviconInspectionErrorTag,
    pub cause: serde_json::Value,
    pub resource: AssetResource,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AssetProjectFaviconInspectionErrorTag {
    #[default]
    #[serde(rename = "AssetProjectFaviconInspectionError")]
    AssetProjectFaviconInspectionError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetProjectFaviconNotFoundError {
    #[serde(rename = "_tag")]
    pub tag: AssetProjectFaviconNotFoundErrorTag,
    pub resource: AssetResource,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AssetProjectFaviconNotFoundErrorTag {
    #[default]
    #[serde(rename = "AssetProjectFaviconNotFoundError")]
    AssetProjectFaviconNotFoundError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetProjectFaviconResolutionError {
    #[serde(rename = "_tag")]
    pub tag: AssetProjectFaviconResolutionErrorTag,
    pub cause: serde_json::Value,
    pub resource: AssetResource,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AssetProjectFaviconResolutionErrorTag {
    #[default]
    #[serde(rename = "AssetProjectFaviconResolutionError")]
    AssetProjectFaviconResolutionError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "_tag")]
pub enum AssetResource {
    #[serde(rename = "workspace-file")]
    WorkspaceFile {
        path: TrimmedNonEmptyString,
        #[serde(rename = "threadId")]
        thread_id: ThreadId,
    },
    #[serde(rename = "attachment")]
    Attachment {
        #[serde(rename = "attachmentId")]
        attachment_id: TrimmedNonEmptyString,
    },
    #[serde(rename = "project-favicon")]
    ProjectFavicon { cwd: TrimmedNonEmptyString },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetSigningKeyLoadError {
    #[serde(rename = "_tag")]
    pub tag: AssetSigningKeyLoadErrorTag,
    pub cause: serde_json::Value,
    pub resource: AssetResource,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AssetSigningKeyLoadErrorTag {
    #[default]
    #[serde(rename = "AssetSigningKeyLoadError")]
    AssetSigningKeyLoadError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetWorkspaceAssetInspectionError {
    #[serde(rename = "_tag")]
    pub tag: AssetWorkspaceAssetInspectionErrorTag,
    pub cause: serde_json::Value,
    pub resource: AssetResource,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AssetWorkspaceAssetInspectionErrorTag {
    #[default]
    #[serde(rename = "AssetWorkspaceAssetInspectionError")]
    AssetWorkspaceAssetInspectionError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetWorkspaceAssetNotFoundError {
    #[serde(rename = "_tag")]
    pub tag: AssetWorkspaceAssetNotFoundErrorTag,
    pub resource: AssetResource,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AssetWorkspaceAssetNotFoundErrorTag {
    #[default]
    #[serde(rename = "AssetWorkspaceAssetNotFoundError")]
    AssetWorkspaceAssetNotFoundError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetWorkspaceContextNotFoundError {
    #[serde(rename = "_tag")]
    pub tag: AssetWorkspaceContextNotFoundErrorTag,
    pub resource: AssetResource,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AssetWorkspaceContextNotFoundErrorTag {
    #[default]
    #[serde(rename = "AssetWorkspaceContextNotFoundError")]
    AssetWorkspaceContextNotFoundError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetWorkspaceContextResolutionError {
    #[serde(rename = "_tag")]
    pub tag: AssetWorkspaceContextResolutionErrorTag,
    pub cause: serde_json::Value,
    pub resource: AssetResource,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AssetWorkspaceContextResolutionErrorTag {
    #[default]
    #[serde(rename = "AssetWorkspaceContextResolutionError")]
    AssetWorkspaceContextResolutionError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetWorkspacePathValidationError {
    #[serde(rename = "_tag")]
    pub tag: AssetWorkspacePathValidationErrorTag,
    pub cause: serde_json::Value,
    pub resource: AssetResource,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AssetWorkspacePathValidationErrorTag {
    #[default]
    #[serde(rename = "AssetWorkspacePathValidationError")]
    AssetWorkspacePathValidationError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetWorkspaceResolutionError {
    #[serde(rename = "_tag")]
    pub tag: AssetWorkspaceResolutionErrorTag,
    pub cause: serde_json::Value,
    pub resource: AssetResource,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AssetWorkspaceResolutionErrorTag {
    #[default]
    #[serde(rename = "AssetWorkspaceResolutionError")]
    AssetWorkspaceResolutionError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetWorkspaceRootNormalizationError {
    #[serde(rename = "_tag")]
    pub tag: AssetWorkspaceRootNormalizationErrorTag,
    pub cause: serde_json::Value,
    pub resource: AssetResource,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AssetWorkspaceRootNormalizationErrorTag {
    #[default]
    #[serde(rename = "AssetWorkspaceRootNormalizationError")]
    AssetWorkspaceRootNormalizationError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthAccessSnapshot {
    #[serde(rename = "clientSessions")]
    pub client_sessions: Vec<AuthClientSession>,
    #[serde(rename = "pairingLinks")]
    pub pairing_links: Vec<AuthPairingLink>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthAccessStreamClientRemovedEvent {
    pub payload: AuthAccessStreamClientRemovedEventPayload,
    pub revision: DurationMillis,
    pub r#type: AuthAccessStreamClientRemovedEventType,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthAccessStreamClientRemovedEventPayload {
    #[serde(rename = "sessionId")]
    pub session_id: AuthSessionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AuthAccessStreamClientRemovedEventType {
    #[default]
    #[serde(rename = "clientRemoved")]
    ClientRemoved,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthAccessStreamClientUpsertedEvent {
    pub payload: AuthClientSession,
    pub revision: DurationMillis,
    pub r#type: AuthAccessStreamClientUpsertedEventType,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AuthAccessStreamClientUpsertedEventType {
    #[default]
    #[serde(rename = "clientUpserted")]
    ClientUpserted,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthAccessStreamError {
    #[serde(rename = "_tag")]
    pub tag: AuthAccessStreamErrorTag,
    pub message: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AuthAccessStreamErrorTag {
    #[default]
    #[serde(rename = "AuthAccessStreamError")]
    AuthAccessStreamError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AuthAccessStreamEvent {
    AuthAccessStreamSnapshotEvent(AuthAccessStreamSnapshotEvent),
    AuthAccessStreamPairingLinkUpsertedEvent(AuthAccessStreamPairingLinkUpsertedEvent),
    AuthAccessStreamPairingLinkRemovedEvent(AuthAccessStreamPairingLinkRemovedEvent),
    AuthAccessStreamClientUpsertedEvent(AuthAccessStreamClientUpsertedEvent),
    AuthAccessStreamClientRemovedEvent(AuthAccessStreamClientRemovedEvent),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthAccessStreamPairingLinkRemovedEvent {
    pub payload: AuthAccessStreamPairingLinkRemovedEventPayload,
    pub revision: DurationMillis,
    pub r#type: AuthAccessStreamPairingLinkRemovedEventType,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthAccessStreamPairingLinkRemovedEventPayload {
    pub id: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AuthAccessStreamPairingLinkRemovedEventType {
    #[default]
    #[serde(rename = "pairingLinkRemoved")]
    PairingLinkRemoved,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthAccessStreamPairingLinkUpsertedEvent {
    pub payload: AuthPairingLink,
    pub revision: DurationMillis,
    pub r#type: AuthAccessStreamPairingLinkUpsertedEventType,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AuthAccessStreamPairingLinkUpsertedEventType {
    #[default]
    #[serde(rename = "pairingLinkUpserted")]
    PairingLinkUpserted,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthAccessStreamSnapshotEvent {
    pub payload: AuthAccessSnapshot,
    pub revision: DurationMillis,
    pub r#type: AuthAccessStreamSnapshotEventType,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AuthAccessStreamSnapshotEventType {
    #[default]
    #[serde(rename = "snapshot")]
    Snapshot,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthClientMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser: Option<TrimmedNonEmptyString>,
    #[serde(rename = "deviceType")]
    pub device_type: AuthClientMetadataDeviceType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "ipAddress")]
    pub ip_address: Option<TrimmedNonEmptyString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<TrimmedNonEmptyString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<TrimmedNonEmptyString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "userAgent")]
    pub user_agent: Option<TrimmedNonEmptyString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuthClientMetadataDeviceType {
    #[serde(rename = "desktop")]
    Desktop,
    #[serde(rename = "mobile")]
    Mobile,
    #[serde(rename = "tablet")]
    Tablet,
    #[serde(rename = "bot")]
    Bot,
    #[serde(rename = "unknown")]
    UnknownX,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthClientSession {
    pub client: AuthClientMetadata,
    pub connected: bool,
    pub current: bool,
    #[serde(rename = "expiresAt")]
    pub expires_at: TrimmedNonEmptyString,
    #[serde(rename = "issuedAt")]
    pub issued_at: TrimmedNonEmptyString,
    #[serde(rename = "lastConnectedAt")]
    pub last_connected_at: Option<TrimmedNonEmptyString>,
    pub method: ServerAuthSessionMethod,
    pub scopes: AuthEnvironmentScopes,
    #[serde(rename = "sessionId")]
    pub session_id: AuthSessionId,
    pub subject: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuthEnvironmentScope {
    #[serde(rename = "orchestration:read")]
    OrchestrationRead,
    #[serde(rename = "orchestration:operate")]
    OrchestrationOperate,
    #[serde(rename = "terminal:operate")]
    TerminalOperate,
    #[serde(rename = "review:write")]
    ReviewWrite,
    #[serde(rename = "access:read")]
    AccessRead,
    #[serde(rename = "access:write")]
    AccessWrite,
    #[serde(rename = "relay:read")]
    RelayRead,
    #[serde(rename = "relay:write")]
    RelayWrite,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

pub type AuthEnvironmentScopes = Vec<AuthEnvironmentScope>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthPairingLink {
    #[serde(rename = "createdAt")]
    pub created_at: TrimmedNonEmptyString,
    pub credential: TrimmedNonEmptyString,
    #[serde(rename = "expiresAt")]
    pub expires_at: TrimmedNonEmptyString,
    pub id: TrimmedNonEmptyString,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<TrimmedNonEmptyString>,
    pub scopes: AuthEnvironmentScopes,
    pub subject: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuthSessionId(pub String);

pub type AutoCompactThresholdTokens = Option<AutoCompactThresholdTokens1>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Copy, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AutoCompactThresholdTokens1(pub i64);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Copy, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AutoCompactThresholdTokens2(pub i64);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BooleanProviderOptionDescriptor {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "currentValue")]
    pub current_value: Option<Option<bool>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub description: Option<Option<TrimmedNonEmptyString>>,
    pub id: TrimmedNonEmptyString,
    pub label: TrimmedNonEmptyString,
    pub r#type: BooleanProviderOptionDescriptorType,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum BooleanProviderOptionDescriptorType {
    #[default]
    #[serde(rename = "boolean")]
    Boolean,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatAttachment {
    ChatImageAttachment(ChatImageAttachment),
    ChatFileAttachment(ChatFileAttachment),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatFileAttachment {
    pub id: TrimmedNonEmptyString,
    #[serde(rename = "mimeType")]
    pub mime_type: TrimmedNonEmptyString,
    pub name: TrimmedNonEmptyString,
    #[serde(rename = "sizeBytes")]
    pub size_bytes: i64,
    pub r#type: ChatFileAttachmentType,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ChatFileAttachmentType {
    #[default]
    #[serde(rename = "file")]
    File,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatImageAttachment {
    pub id: TrimmedNonEmptyString,
    #[serde(rename = "mimeType")]
    pub mime_type: TrimmedNonEmptyString,
    pub name: TrimmedNonEmptyString,
    #[serde(rename = "sizeBytes")]
    pub size_bytes: i64,
    pub r#type: ChatImageAttachmentType,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ChatImageAttachmentType {
    #[default]
    #[serde(rename = "image")]
    Image,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CheckpointRef(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClaudeBinaryInstallFailedError {
    #[serde(rename = "_tag")]
    pub tag: ClaudeBinaryInstallFailedErrorTag,
    pub message: TrimmedNonEmptyString,
    pub reason: ClaudeBinaryInstallFailureReasonSchema,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClaudeBinaryInstallFailedErrorTag {
    #[default]
    #[serde(rename = "ClaudeBinaryInstallFailedError")]
    ClaudeBinaryInstallFailedError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ClaudeBinaryInstallFailureReasonSchema {
    #[serde(rename = "download_failed")]
    DownloadFailed,
    #[serde(rename = "invalid_checksum")]
    InvalidChecksum,
    #[serde(rename = "install_locked")]
    InstallLocked,
    #[serde(rename = "override_missing")]
    OverrideMissing,
    #[serde(rename = "unsupported_platform")]
    UnsupportedPlatform,
    #[serde(rename = "validation_failed")]
    ValidationFailed,
    #[serde(rename = "write_failed")]
    WriteFailed,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClaudeBinaryInstallProgressEventSchema {
    #[serde(rename = "progress")]
    Progress {
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        #[serde(rename = "bytesDownloaded")]
        bytes_downloaded: Option<Option<DurationMillis>>,
        stage: ClaudeBinaryInstallProgressStageSchema,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        #[serde(rename = "totalBytes")]
        total_bytes: Option<Option<DurationMillis>>,
    },
    #[serde(rename = "complete")]
    Complete { status: ClaudeBinaryStatusSchema },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ClaudeBinaryInstallProgressStageSchema {
    #[serde(rename = "checking")]
    Checking,
    #[serde(rename = "waiting_for_lock")]
    WaitingForLock,
    #[serde(rename = "downloading")]
    Downloading,
    #[serde(rename = "verifying")]
    Verifying,
    #[serde(rename = "installing")]
    Installing,
    #[serde(rename = "validating")]
    Validating,
    #[serde(rename = "activating")]
    Activating,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum ClaudeBinaryStatusSchema {
    #[serde(rename = "available")]
    Available {
        #[serde(rename = "executablePath")]
        executable_path: TrimmedNonEmptyString,
        source: ClaudeBinaryStatusSchemaAvailableSource,
        version: TrimmedNonEmptyString,
    },
    #[serde(rename = "missing")]
    Missing {
        #[serde(rename = "binarySizeBytes")]
        binary_size_bytes: DurationMillis,
        version: TrimmedNonEmptyString,
    },
    #[serde(rename = "unsupported")]
    Unsupported {
        arch: TrimmedNonEmptyString,
        platform: TrimmedNonEmptyString,
        version: TrimmedNonEmptyString,
    },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ClaudeBinaryStatusSchemaAvailableSource {
    #[serde(rename = "explicit")]
    Explicit,
    #[serde(rename = "override")]
    Override,
    #[serde(rename = "managed")]
    Managed,
    #[serde(rename = "path")]
    Path,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

/// Void payload — send `{}` on the wire.
pub type ClaudeGetBinaryStatusPayload = serde_json::Value;

/// Void payload — send `{}` on the wire.
pub type ClaudeInstallBinaryPayload = serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ClientOrchestrationCommand {
    ProjectCreateCommand(ProjectCreateCommand),
    ProjectMetaUpdate {
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        #[serde(rename = "additionalRoots")]
        additional_roots: Option<Option<Vec<WorkspaceRootRef>>>,
        #[serde(rename = "commandId")]
        command_id: CommandId,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        #[serde(rename = "defaultModelSelection")]
        default_model_selection: Option<Option<Option<ModelSelection>>>,
        #[serde(rename = "projectId")]
        project_id: ProjectId,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        scripts: Option<Option<Vec<ProjectScript>>>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        title: Option<Option<TrimmedNonEmptyString>>,
        r#type: ClientOrchestrationCommandProjectMetaUpdateType,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        #[serde(rename = "workspaceRoot")]
        workspace_root: Option<Option<TrimmedNonEmptyString>>,
    },
    ProjectDelete {
        #[serde(rename = "commandId")]
        command_id: CommandId,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        force: Option<Option<bool>>,
        #[serde(rename = "projectId")]
        project_id: ProjectId,
        r#type: ClientOrchestrationCommandProjectDeleteType,
    },
    ThreadCreate {
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        #[serde(rename = "additionalRoots")]
        additional_roots: Option<Option<Vec<WorkspaceRootRef>>>,
        branch: Option<TrimmedNonEmptyString>,
        #[serde(rename = "commandId")]
        command_id: CommandId,
        #[serde(rename = "createdAt")]
        created_at: TrimmedNonEmptyString,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        #[serde(rename = "interactionMode")]
        interaction_mode: Option<Option<ProviderInteractionMode>>,
        #[serde(rename = "modelSelection")]
        model_selection: ModelSelection,
        #[serde(rename = "projectId")]
        project_id: ProjectId,
        #[serde(rename = "runtimeMode")]
        runtime_mode: RuntimeMode,
        #[serde(rename = "threadId")]
        thread_id: ThreadId,
        title: TrimmedNonEmptyString,
        r#type: ClientOrchestrationCommandThreadCreateType,
        #[serde(rename = "worktreePath")]
        worktree_path: Option<TrimmedNonEmptyString>,
    },
    ThreadDelete {
        #[serde(rename = "commandId")]
        command_id: CommandId,
        #[serde(rename = "threadId")]
        thread_id: ThreadId,
        r#type: ClientOrchestrationCommandThreadDeleteType,
    },
    ThreadArchive {
        #[serde(rename = "commandId")]
        command_id: CommandId,
        #[serde(rename = "threadId")]
        thread_id: ThreadId,
        r#type: ClientOrchestrationCommandThreadArchiveType,
    },
    ThreadUnarchive {
        #[serde(rename = "commandId")]
        command_id: CommandId,
        #[serde(rename = "threadId")]
        thread_id: ThreadId,
        r#type: ClientOrchestrationCommandThreadUnarchiveType,
    },
    ThreadSettle {
        #[serde(rename = "commandId")]
        command_id: CommandId,
        #[serde(rename = "threadId")]
        thread_id: ThreadId,
        r#type: ClientOrchestrationCommandThreadSettleType,
    },
    ThreadUnsettle {
        #[serde(rename = "commandId")]
        command_id: CommandId,
        reason: ClientOrchestrationCommandThreadUnsettleReason,
        #[serde(rename = "threadId")]
        thread_id: ThreadId,
        r#type: ClientOrchestrationCommandThreadUnsettleType,
    },
    ThreadSnooze {
        #[serde(rename = "commandId")]
        command_id: CommandId,
        #[serde(rename = "snoozedUntil")]
        snoozed_until: TrimmedNonEmptyString,
        #[serde(rename = "threadId")]
        thread_id: ThreadId,
        r#type: ClientOrchestrationCommandThreadSnoozeType,
    },
    ThreadUnsnooze {
        #[serde(rename = "commandId")]
        command_id: CommandId,
        reason: ClientOrchestrationCommandThreadUnsnoozeReason,
        #[serde(rename = "threadId")]
        thread_id: ThreadId,
        r#type: ClientOrchestrationCommandThreadUnsnoozeType,
    },
    ThreadMetaUpdate {
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        #[serde(rename = "additionalRoots")]
        additional_roots: Option<Option<Vec<WorkspaceRootRef>>>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        branch: Option<Option<Option<TrimmedNonEmptyString>>>,
        #[serde(rename = "commandId")]
        command_id: CommandId,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        #[serde(rename = "expectedBranch")]
        expected_branch: Option<Option<Option<TrimmedNonEmptyString>>>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        #[serde(rename = "modelSelection")]
        model_selection: Option<Option<ModelSelection>>,
        #[serde(rename = "threadId")]
        thread_id: ThreadId,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        title: Option<Option<TrimmedNonEmptyString>>,
        r#type: ClientOrchestrationCommandThreadMetaUpdateType,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        #[serde(rename = "worktreePath")]
        worktree_path: Option<Option<Option<TrimmedNonEmptyString>>>,
    },
    ThreadRuntimeModeSet {
        #[serde(rename = "commandId")]
        command_id: CommandId,
        #[serde(rename = "createdAt")]
        created_at: TrimmedNonEmptyString,
        #[serde(rename = "runtimeMode")]
        runtime_mode: RuntimeMode,
        #[serde(rename = "threadId")]
        thread_id: ThreadId,
        r#type: ClientOrchestrationCommandThreadRuntimeModeSetType,
    },
    ThreadInteractionModeSet {
        #[serde(rename = "commandId")]
        command_id: CommandId,
        #[serde(rename = "createdAt")]
        created_at: TrimmedNonEmptyString,
        #[serde(rename = "interactionMode")]
        interaction_mode: ProviderInteractionMode,
        #[serde(rename = "threadId")]
        thread_id: ThreadId,
        r#type: ClientOrchestrationCommandThreadInteractionModeSetType,
    },
    ThreadTurnStart {
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        bootstrap: Option<Option<ClientOrchestrationCommandThreadTurnStartBootstrap>>,
        #[serde(rename = "commandId")]
        command_id: CommandId,
        #[serde(rename = "createdAt")]
        created_at: TrimmedNonEmptyString,
        #[serde(rename = "interactionMode")]
        interaction_mode: ProviderInteractionMode,
        message: ClientOrchestrationCommandThreadTurnStartMessage,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        #[serde(rename = "modelSelection")]
        model_selection: Option<Option<ModelSelection>>,
        #[serde(rename = "runtimeMode")]
        runtime_mode: RuntimeMode,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        #[serde(rename = "sourceProposedPlan")]
        source_proposed_plan:
            Option<Option<ClientOrchestrationCommandThreadTurnStartSourceProposedPlan>>,
        #[serde(rename = "threadId")]
        thread_id: ThreadId,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        #[serde(rename = "titleSeed")]
        title_seed: Option<Option<TrimmedNonEmptyString>>,
        r#type: ClientOrchestrationCommandThreadTurnStartType,
    },
    ThreadTurnInterrupt {
        #[serde(rename = "commandId")]
        command_id: CommandId,
        #[serde(rename = "createdAt")]
        created_at: TrimmedNonEmptyString,
        #[serde(rename = "threadId")]
        thread_id: ThreadId,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        #[serde(rename = "turnId")]
        turn_id: Option<Option<TurnId>>,
        r#type: ClientOrchestrationCommandThreadTurnInterruptType,
    },
    ThreadApprovalRespond {
        #[serde(rename = "commandId")]
        command_id: CommandId,
        #[serde(rename = "createdAt")]
        created_at: TrimmedNonEmptyString,
        decision: ProviderApprovalDecision,
        #[serde(rename = "requestId")]
        request_id: ApprovalRequestId,
        #[serde(rename = "threadId")]
        thread_id: ThreadId,
        r#type: ClientOrchestrationCommandThreadApprovalRespondType,
    },
    ThreadUserInputRespond {
        answers: ProviderUserInputAnswers,
        #[serde(rename = "commandId")]
        command_id: CommandId,
        #[serde(rename = "createdAt")]
        created_at: TrimmedNonEmptyString,
        #[serde(rename = "requestId")]
        request_id: ApprovalRequestId,
        #[serde(rename = "threadId")]
        thread_id: ThreadId,
        r#type: ClientOrchestrationCommandThreadUserInputRespondType,
    },
    ThreadCheckpointRevert {
        #[serde(rename = "commandId")]
        command_id: CommandId,
        #[serde(rename = "createdAt")]
        created_at: TrimmedNonEmptyString,
        #[serde(rename = "threadId")]
        thread_id: ThreadId,
        #[serde(rename = "turnCount")]
        turn_count: NonNegativeInt,
        r#type: ClientOrchestrationCommandThreadCheckpointRevertType,
    },
    ThreadSessionStop {
        #[serde(rename = "commandId")]
        command_id: CommandId,
        #[serde(rename = "createdAt")]
        created_at: TrimmedNonEmptyString,
        #[serde(rename = "threadId")]
        thread_id: ThreadId,
        r#type: ClientOrchestrationCommandThreadSessionStopType,
    },
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandProjectMetaUpdateType {
    #[default]
    #[serde(rename = "project.meta.update")]
    ProjectMetaUpdate,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandProjectDeleteType {
    #[default]
    #[serde(rename = "project.delete")]
    ProjectDelete,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandThreadCreateType {
    #[default]
    #[serde(rename = "thread.create")]
    ThreadCreate,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandThreadDeleteType {
    #[default]
    #[serde(rename = "thread.delete")]
    ThreadDelete,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandThreadArchiveType {
    #[default]
    #[serde(rename = "thread.archive")]
    ThreadArchive,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandThreadUnarchiveType {
    #[default]
    #[serde(rename = "thread.unarchive")]
    ThreadUnarchive,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandThreadSettleType {
    #[default]
    #[serde(rename = "thread.settle")]
    ThreadSettle,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandThreadUnsettleReason {
    #[default]
    #[serde(rename = "user")]
    User,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandThreadUnsettleType {
    #[default]
    #[serde(rename = "thread.unsettle")]
    ThreadUnsettle,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandThreadSnoozeType {
    #[default]
    #[serde(rename = "thread.snooze")]
    ThreadSnooze,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandThreadUnsnoozeReason {
    #[default]
    #[serde(rename = "user")]
    User,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandThreadUnsnoozeType {
    #[default]
    #[serde(rename = "thread.unsnooze")]
    ThreadUnsnooze,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandThreadMetaUpdateType {
    #[default]
    #[serde(rename = "thread.meta.update")]
    ThreadMetaUpdate,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandThreadRuntimeModeSetType {
    #[default]
    #[serde(rename = "thread.runtime-mode.set")]
    ThreadRuntimeModeSet,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandThreadInteractionModeSetType {
    #[default]
    #[serde(rename = "thread.interaction-mode.set")]
    ThreadInteractionModeSet,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientOrchestrationCommandThreadTurnStartBootstrapCreateThread {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "additionalRoots")]
    pub additional_roots: Option<Option<Vec<WorkspaceRootRef>>>,
    pub branch: Option<TrimmedNonEmptyString>,
    #[serde(rename = "createdAt")]
    pub created_at: TrimmedNonEmptyString,
    #[serde(rename = "interactionMode")]
    pub interaction_mode: ProviderInteractionMode,
    #[serde(rename = "modelSelection")]
    pub model_selection: ModelSelection,
    #[serde(rename = "projectId")]
    pub project_id: ProjectId,
    #[serde(rename = "runtimeMode")]
    pub runtime_mode: RuntimeMode,
    pub title: TrimmedNonEmptyString,
    #[serde(rename = "worktreePath")]
    pub worktree_path: Option<TrimmedNonEmptyString>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientOrchestrationCommandThreadTurnStartBootstrapPrepareWorktree {
    #[serde(rename = "baseBranch")]
    pub base_branch: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub branch: Option<Option<TrimmedNonEmptyString>>,
    #[serde(rename = "projectCwd")]
    pub project_cwd: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "startFromOrigin")]
    pub start_from_origin: Option<Option<bool>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientOrchestrationCommandThreadTurnStartBootstrap {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "createThread")]
    pub create_thread:
        Option<Option<ClientOrchestrationCommandThreadTurnStartBootstrapCreateThread>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "prepareWorktree")]
    pub prepare_worktree:
        Option<Option<ClientOrchestrationCommandThreadTurnStartBootstrapPrepareWorktree>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "runSetupScript")]
    pub run_setup_script: Option<Option<bool>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientOrchestrationCommandThreadTurnStartMessageAttachments {
    #[serde(rename = "image")]
    Image {
        #[serde(rename = "dataUrl")]
        data_url: TrimmedNonEmptyString,
        #[serde(rename = "mimeType")]
        mime_type: TrimmedNonEmptyString,
        name: TrimmedNonEmptyString,
        #[serde(rename = "sizeBytes")]
        size_bytes: i64,
    },
    #[serde(rename = "file")]
    File {
        #[serde(rename = "dataUrl")]
        data_url: TrimmedNonEmptyString,
        #[serde(rename = "mimeType")]
        mime_type: TrimmedNonEmptyString,
        name: TrimmedNonEmptyString,
        #[serde(rename = "sizeBytes")]
        size_bytes: i64,
    },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandThreadTurnStartMessageRole {
    #[default]
    #[serde(rename = "user")]
    User,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientOrchestrationCommandThreadTurnStartMessage {
    pub attachments: Vec<ClientOrchestrationCommandThreadTurnStartMessageAttachments>,
    #[serde(rename = "messageId")]
    pub message_id: MessageId,
    pub role: ClientOrchestrationCommandThreadTurnStartMessageRole,
    pub text: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientOrchestrationCommandThreadTurnStartSourceProposedPlan {
    #[serde(rename = "planId")]
    pub plan_id: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandThreadTurnStartType {
    #[default]
    #[serde(rename = "thread.turn.start")]
    ThreadTurnStart,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandThreadTurnInterruptType {
    #[default]
    #[serde(rename = "thread.turn.interrupt")]
    ThreadTurnInterrupt,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandThreadApprovalRespondType {
    #[default]
    #[serde(rename = "thread.approval.respond")]
    ThreadApprovalRespond,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandThreadUserInputRespondType {
    #[default]
    #[serde(rename = "thread.user-input.respond")]
    ThreadUserInputRespond,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandThreadCheckpointRevertType {
    #[default]
    #[serde(rename = "thread.checkpoint.revert")]
    ThreadCheckpointRevert,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ClientOrchestrationCommandThreadSessionStopType {
    #[default]
    #[serde(rename = "thread.session.stop")]
    ThreadSessionStop,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

/// Void payload — send `{}` on the wire.
pub type CloudGetRelayClientStatusPayload = serde_json::Value;

/// Void payload — send `{}` on the wire.
pub type CloudInstallRelayClientPayload = serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommandId(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomLanguageServer {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub args: Option<Option<Vec<String>>>,
    pub command: TrimmedNonEmptyString,
    #[serde(rename = "displayName")]
    pub display_name: TrimmedNonEmptyString,
    pub extensions: Vec<TrimmedNonEmptyString>,
    #[serde(rename = "languageId")]
    pub language_id: TrimmedNonEmptyString,
    #[serde(rename = "serverId")]
    pub server_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveredLocalServer {
    pub host: TrimmedNonEmptyString,
    pub pid: Option<i64>,
    pub port: i64,
    #[serde(rename = "processName")]
    pub process_name: Option<TrimmedNonEmptyString>,
    pub terminal: Option<DiscoveredLocalServerTerminal>,
    pub url: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveredLocalServerTerminal {
    #[serde(rename = "terminalId")]
    pub terminal_id: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveredLocalServerList {
    #[serde(rename = "scannedAt")]
    pub scanned_at: TrimmedNonEmptyString,
    pub servers: Vec<DiscoveredLocalServer>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DispatchResult {
    pub sequence: NonNegativeInt,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EditorId {
    #[serde(rename = "cursor")]
    Cursor,
    #[serde(rename = "trae")]
    Trae,
    #[serde(rename = "kiro")]
    Kiro,
    #[serde(rename = "vscode")]
    Vscode,
    #[serde(rename = "vscode-insiders")]
    VscodeInsiders,
    #[serde(rename = "vscodium")]
    Vscodium,
    #[serde(rename = "zed")]
    Zed,
    #[serde(rename = "antigravity")]
    Antigravity,
    #[serde(rename = "idea")]
    Idea,
    #[serde(rename = "aqua")]
    Aqua,
    #[serde(rename = "clion")]
    Clion,
    #[serde(rename = "datagrip")]
    Datagrip,
    #[serde(rename = "dataspell")]
    Dataspell,
    #[serde(rename = "goland")]
    Goland,
    #[serde(rename = "phpstorm")]
    Phpstorm,
    #[serde(rename = "pycharm")]
    Pycharm,
    #[serde(rename = "rider")]
    Rider,
    #[serde(rename = "rubymine")]
    Rubymine,
    #[serde(rename = "rustrover")]
    Rustrover,
    #[serde(rename = "webstorm")]
    Webstorm,
    #[serde(rename = "file-manager")]
    FileManager,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentAuthorizationError {
    #[serde(rename = "_tag")]
    pub tag: EnvironmentAuthorizationErrorTag,
    pub message: String,
    #[serde(rename = "requiredScope")]
    pub required_scope: AuthEnvironmentScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum EnvironmentAuthorizationErrorTag {
    #[default]
    #[serde(rename = "EnvironmentAuthorizationError")]
    EnvironmentAuthorizationError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EnvironmentId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventId(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionEnvironmentCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "connectionProbe")]
    pub connection_probe: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "repositoryIdentity")]
    pub repository_identity: Option<Option<bool>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "serverSelfUpdate")]
    pub server_self_update: Option<ExecutionEnvironmentCapabilitiesServerSelfUpdate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "threadSettlement")]
    pub thread_settlement: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "threadSnooze")]
    pub thread_snooze: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExecutionEnvironmentCapabilitiesServerSelfUpdate {
    #[serde(rename = "boot-service")]
    BootService,
    #[serde(rename = "respawn")]
    Respawn,
    #[serde(rename = "desktop-managed")]
    DesktopManaged,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionEnvironmentDescriptor {
    pub capabilities: ExecutionEnvironmentCapabilities,
    #[serde(rename = "environmentId")]
    pub environment_id: EnvironmentId,
    pub label: TrimmedNonEmptyString,
    pub platform: ExecutionEnvironmentPlatform,
    #[serde(rename = "serverVersion")]
    pub server_version: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionEnvironmentPlatform {
    pub arch: ExecutionEnvironmentPlatformArch,
    pub os: ExecutionEnvironmentPlatformOs,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExecutionEnvironmentPlatformArch {
    #[serde(rename = "arm64")]
    Arm64,
    #[serde(rename = "x64")]
    X64,
    #[serde(rename = "other")]
    Other,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExecutionEnvironmentPlatformOs {
    #[serde(rename = "darwin")]
    Darwin,
    #[serde(rename = "linux")]
    Linux,
    #[serde(rename = "windows")]
    Windows,
    #[serde(rename = "unknown")]
    UnknownX,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalLauncherBrowserSpawnError {
    #[serde(rename = "_tag")]
    pub tag: ExternalLauncherBrowserSpawnErrorTag,
    pub args: Vec<TrimmedNonEmptyString>,
    pub cause: serde_json::Value,
    pub command: TrimmedNonEmptyString,
    pub target: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ExternalLauncherBrowserSpawnErrorTag {
    #[default]
    #[serde(rename = "ExternalLauncherBrowserSpawnError")]
    ExternalLauncherBrowserSpawnError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalLauncherCommandNotFoundError {
    #[serde(rename = "_tag")]
    pub tag: ExternalLauncherCommandNotFoundErrorTag,
    pub command: TrimmedNonEmptyString,
    pub editor: EditorId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ExternalLauncherCommandNotFoundErrorTag {
    #[default]
    #[serde(rename = "ExternalLauncherCommandNotFoundError")]
    ExternalLauncherCommandNotFoundError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalLauncherEditorSpawnError {
    #[serde(rename = "_tag")]
    pub tag: ExternalLauncherEditorSpawnErrorTag,
    pub args: Vec<TrimmedNonEmptyString>,
    pub cause: serde_json::Value,
    pub command: TrimmedNonEmptyString,
    pub editor: EditorId,
    pub target: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ExternalLauncherEditorSpawnErrorTag {
    #[default]
    #[serde(rename = "ExternalLauncherEditorSpawnError")]
    ExternalLauncherEditorSpawnError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExternalLauncherError {
    ExternalLauncherUnknownEditorError(ExternalLauncherUnknownEditorError),
    ExternalLauncherUnsupportedEditorError(ExternalLauncherUnsupportedEditorError),
    ExternalLauncherCommandNotFoundError(ExternalLauncherCommandNotFoundError),
    ExternalLauncherBrowserSpawnError(ExternalLauncherBrowserSpawnError),
    ExternalLauncherEditorSpawnError(ExternalLauncherEditorSpawnError),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalLauncherUnknownEditorError {
    #[serde(rename = "_tag")]
    pub tag: ExternalLauncherUnknownEditorErrorTag,
    pub editor: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ExternalLauncherUnknownEditorErrorTag {
    #[default]
    #[serde(rename = "ExternalLauncherUnknownEditorError")]
    ExternalLauncherUnknownEditorError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalLauncherUnsupportedEditorError {
    #[serde(rename = "_tag")]
    pub tag: ExternalLauncherUnsupportedEditorErrorTag,
    pub editor: EditorId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ExternalLauncherUnsupportedEditorErrorTag {
    #[default]
    #[serde(rename = "ExternalLauncherUnsupportedEditorError")]
    ExternalLauncherUnsupportedEditorError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilesystemBrowseEntry {
    #[serde(rename = "fullPath")]
    pub full_path: TrimmedNonEmptyString,
    pub name: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilesystemBrowseError {
    #[serde(rename = "_tag")]
    pub tag: FilesystemBrowseErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cwd: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub failure: Option<Option<FilesystemBrowseFailure>>,
    pub message: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "parentPath")]
    pub parent_path: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "partialPath")]
    pub partial_path: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub platform: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum FilesystemBrowseErrorTag {
    #[default]
    #[serde(rename = "FilesystemBrowseError")]
    FilesystemBrowseError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FilesystemBrowseFailure {
    #[serde(rename = "windows_path_unsupported")]
    WindowsPathUnsupported,
    #[serde(rename = "current_project_required")]
    CurrentProjectRequired,
    #[serde(rename = "read_directory_failed")]
    ReadDirectoryFailed,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilesystemBrowseInput {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cwd: Option<Option<TrimmedNonEmptyString>>,
    #[serde(rename = "partialPath")]
    pub partial_path: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilesystemBrowseResult {
    pub entries: Vec<FilesystemBrowseEntry>,
    #[serde(rename = "parentPath")]
    pub parent_path: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum GitActionProgressEvent {
    #[serde(rename = "action_started")]
    ActionStarted {
        action: GitStackedAction,
        #[serde(rename = "actionId")]
        action_id: TrimmedNonEmptyString,
        cwd: TrimmedNonEmptyString,
        phases: Vec<GitActionProgressPhase>,
    },
    #[serde(rename = "phase_started")]
    PhaseStarted {
        action: GitStackedAction,
        #[serde(rename = "actionId")]
        action_id: TrimmedNonEmptyString,
        cwd: TrimmedNonEmptyString,
        label: TrimmedNonEmptyString,
        phase: GitActionProgressPhase,
    },
    #[serde(rename = "hook_started")]
    HookStarted {
        action: GitStackedAction,
        #[serde(rename = "actionId")]
        action_id: TrimmedNonEmptyString,
        cwd: TrimmedNonEmptyString,
        #[serde(rename = "hookName")]
        hook_name: TrimmedNonEmptyString,
    },
    #[serde(rename = "hook_output")]
    HookOutput {
        action: GitStackedAction,
        #[serde(rename = "actionId")]
        action_id: TrimmedNonEmptyString,
        cwd: TrimmedNonEmptyString,
        #[serde(rename = "hookName")]
        hook_name: Option<TrimmedNonEmptyString>,
        stream: GitActionProgressStream,
        text: TrimmedNonEmptyString,
    },
    #[serde(rename = "hook_finished")]
    HookFinished {
        action: GitStackedAction,
        #[serde(rename = "actionId")]
        action_id: TrimmedNonEmptyString,
        cwd: TrimmedNonEmptyString,
        #[serde(rename = "durationMs")]
        duration_ms: Option<NonNegativeInt>,
        #[serde(rename = "exitCode")]
        exit_code: Option<i64>,
        #[serde(rename = "hookName")]
        hook_name: TrimmedNonEmptyString,
    },
    #[serde(rename = "action_finished")]
    ActionFinished {
        action: GitStackedAction,
        #[serde(rename = "actionId")]
        action_id: TrimmedNonEmptyString,
        cwd: TrimmedNonEmptyString,
        result: GitRunStackedActionResult,
    },
    #[serde(rename = "action_failed")]
    ActionFailed {
        action: GitStackedAction,
        #[serde(rename = "actionId")]
        action_id: TrimmedNonEmptyString,
        cwd: TrimmedNonEmptyString,
        message: TrimmedNonEmptyString,
        phase: Option<GitActionProgressPhase>,
    },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GitActionProgressPhase {
    #[serde(rename = "branch")]
    Branch,
    #[serde(rename = "commit")]
    Commit,
    #[serde(rename = "push")]
    Push,
    #[serde(rename = "pr")]
    Pr,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GitActionProgressStream {
    #[serde(rename = "stdout")]
    Stdout,
    #[serde(rename = "stderr")]
    Stderr,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitCommandError {
    #[serde(rename = "_tag")]
    pub tag: GitCommandErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "argumentCount")]
    pub argument_count: Option<Option<DurationMillis>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    pub command: TrimmedNonEmptyString,
    pub cwd: TrimmedNonEmptyString,
    pub detail: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "exitCode")]
    pub exit_code: Option<Option<DurationMillis>>,
    pub operation: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "outputLength")]
    pub output_length: Option<Option<DurationMillis>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "stderrLength")]
    pub stderr_length: Option<Option<DurationMillis>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "stdoutLength")]
    pub stdout_length: Option<Option<DurationMillis>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum GitCommandErrorTag {
    #[default]
    #[serde(rename = "GitCommandError")]
    GitCommandError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitManagerError {
    #[serde(rename = "_tag")]
    pub tag: GitManagerErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    pub cwd: TrimmedNonEmptyString,
    pub detail: TrimmedNonEmptyString,
    pub operation: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum GitManagerErrorTag {
    #[default]
    #[serde(rename = "GitManagerError")]
    GitManagerError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GitManagerServiceError {
    GitManagerError(GitManagerError),
    GitPullRequestMaterializationError(GitPullRequestMaterializationError),
    GitCommandError(GitCommandError),
    SourceControlProviderError(SourceControlProviderError),
    TextGenerationError(TextGenerationError),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitPreparePullRequestThreadInput {
    pub cwd: TrimmedNonEmptyString,
    pub mode: GitPreparePullRequestThreadInputMode,
    pub reference: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "threadId")]
    pub thread_id: Option<Option<ThreadId>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GitPreparePullRequestThreadInputMode {
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "worktree")]
    Worktree,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitPreparePullRequestThreadResult {
    pub branch: TrimmedNonEmptyString,
    #[serde(rename = "pullRequest")]
    pub pull_request: GitPreparePullRequestThreadResultPullRequest,
    #[serde(rename = "worktreePath")]
    pub worktree_path: Option<TrimmedNonEmptyString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GitPreparePullRequestThreadResultPullRequestState {
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "closed")]
    Closed,
    #[serde(rename = "merged")]
    Merged,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitPreparePullRequestThreadResultPullRequest {
    #[serde(rename = "baseBranch")]
    pub base_branch: TrimmedNonEmptyString,
    #[serde(rename = "headBranch")]
    pub head_branch: TrimmedNonEmptyString,
    pub number: PositiveInt,
    pub state: GitPreparePullRequestThreadResultPullRequestState,
    pub title: TrimmedNonEmptyString,
    pub url: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitPullRequestMaterializationError {
    #[serde(rename = "_tag")]
    pub tag: GitPullRequestMaterializationErrorTag,
    pub cause: serde_json::Value,
    pub cwd: TrimmedNonEmptyString,
    #[serde(rename = "headBranch")]
    pub head_branch: TrimmedNonEmptyString,
    #[serde(rename = "headRepository")]
    pub head_repository: Option<TrimmedNonEmptyString>,
    #[serde(rename = "localBranch")]
    pub local_branch: TrimmedNonEmptyString,
    #[serde(rename = "pullRequestNumber")]
    pub pull_request_number: PositiveInt,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum GitPullRequestMaterializationErrorTag {
    #[default]
    #[serde(rename = "GitPullRequestMaterializationError")]
    GitPullRequestMaterializationError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitPullRequestRefInput {
    pub cwd: TrimmedNonEmptyString,
    pub reference: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitResolvePullRequestResult {
    #[serde(rename = "pullRequest")]
    pub pull_request: GitResolvePullRequestResultPullRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GitResolvePullRequestResultPullRequestState {
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "closed")]
    Closed,
    #[serde(rename = "merged")]
    Merged,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitResolvePullRequestResultPullRequest {
    #[serde(rename = "baseBranch")]
    pub base_branch: TrimmedNonEmptyString,
    #[serde(rename = "headBranch")]
    pub head_branch: TrimmedNonEmptyString,
    pub number: PositiveInt,
    pub state: GitResolvePullRequestResultPullRequestState,
    pub title: TrimmedNonEmptyString,
    pub url: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitRunStackedActionInput {
    pub action: GitStackedAction,
    #[serde(rename = "actionId")]
    pub action_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "commitMessage")]
    pub commit_message: Option<Option<TrimmedNonEmptyString>>,
    pub cwd: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "featureBranch")]
    pub feature_branch: Option<Option<bool>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "filePaths")]
    pub file_paths: Option<Option<Vec<TrimmedNonEmptyString>>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitRunStackedActionResult {
    pub action: GitStackedAction,
    pub branch: GitRunStackedActionResultBranch,
    pub commit: GitRunStackedActionResultCommit,
    pub pr: GitRunStackedActionResultPr,
    pub push: GitRunStackedActionResultPush,
    pub toast: GitRunStackedActionResultToast,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GitRunStackedActionResultBranchStatus {
    #[serde(rename = "created")]
    Created,
    #[serde(rename = "skipped_not_requested")]
    SkippedNotRequested,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitRunStackedActionResultBranch {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub name: Option<Option<TrimmedNonEmptyString>>,
    pub status: GitRunStackedActionResultBranchStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GitRunStackedActionResultCommitStatus {
    #[serde(rename = "created")]
    Created,
    #[serde(rename = "skipped_no_changes")]
    SkippedNoChanges,
    #[serde(rename = "skipped_not_requested")]
    SkippedNotRequested,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitRunStackedActionResultCommit {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "commitSha")]
    pub commit_sha: Option<Option<TrimmedNonEmptyString>>,
    pub status: GitRunStackedActionResultCommitStatus,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub subject: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GitRunStackedActionResultPrStatus {
    #[serde(rename = "created")]
    Created,
    #[serde(rename = "opened_existing")]
    OpenedExisting,
    #[serde(rename = "skipped_not_requested")]
    SkippedNotRequested,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitRunStackedActionResultPr {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "baseBranch")]
    pub base_branch: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "headBranch")]
    pub head_branch: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub number: Option<Option<PositiveInt>>,
    pub status: GitRunStackedActionResultPrStatus,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub title: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub url: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GitRunStackedActionResultPushStatus {
    #[serde(rename = "pushed")]
    Pushed,
    #[serde(rename = "skipped_not_requested")]
    SkippedNotRequested,
    #[serde(rename = "skipped_up_to_date")]
    SkippedUpToDate,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitRunStackedActionResultPush {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub branch: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "setUpstream")]
    pub set_upstream: Option<Option<bool>>,
    pub status: GitRunStackedActionResultPushStatus,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "upstreamBranch")]
    pub upstream_branch: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum GitRunStackedActionResultToastCta {
    #[serde(rename = "none")]
    None {},
    #[serde(rename = "open_pr")]
    OpenPr {
        label: TrimmedNonEmptyString,
        url: TrimmedNonEmptyString,
    },
    #[serde(rename = "run_action")]
    RunAction {
        action: GitRunStackedActionToastRunAction,
        label: TrimmedNonEmptyString,
    },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitRunStackedActionResultToast {
    pub cta: GitRunStackedActionResultToastCta,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub description: Option<Option<TrimmedNonEmptyString>>,
    pub title: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitRunStackedActionToastRunAction {
    pub kind: GitStackedAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GitStackedAction {
    #[serde(rename = "commit")]
    Commit,
    #[serde(rename = "push")]
    Push,
    #[serde(rename = "create_pr")]
    CreatePr,
    #[serde(rename = "commit_push")]
    CommitPush,
    #[serde(rename = "commit_push_pr")]
    CommitPushPr,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphBuildInput {
    pub cwd: TrimmedNonEmptyString,
    pub force: bool,
    pub mode: GraphBuildMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GraphBuildMode {
    #[serde(rename = "structural")]
    Structural,
    #[serde(rename = "semantic")]
    Semantic,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GraphBuildState {
    #[serde(rename = "idle")]
    Idle,
    #[serde(rename = "queued")]
    Queued,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "succeeded")]
    Succeeded,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "cancelled")]
    Cancelled,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphBuildStatus {
    pub detail: Option<TrimmedNonEmptyString>,
    #[serde(rename = "finishedAt")]
    pub finished_at: Option<DurationMillis>,
    pub message: Option<TrimmedNonEmptyString>,
    pub mode: Option<GraphBuildMode>,
    #[serde(rename = "startedAt")]
    pub started_at: Option<DurationMillis>,
    pub state: GraphBuildState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphCommandFailedError {
    #[serde(rename = "_tag")]
    pub tag: GraphCommandFailedErrorTag,
    pub detail: TrimmedNonEmptyString,
    #[serde(rename = "exitCode")]
    pub exit_code: Option<DurationMillis>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum GraphCommandFailedErrorTag {
    #[default]
    #[serde(rename = "GraphCommandFailedError")]
    GraphCommandFailedError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphCommunity {
    pub cohesion: DurationMillis,
    pub id: NonNegativeInt,
    pub label: TrimmedNonEmptyString,
    #[serde(rename = "nodeCount")]
    pub node_count: NonNegativeInt,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GraphConfidence {
    #[serde(rename = "EXTRACTED")]
    EXTRACTED,
    #[serde(rename = "INFERRED")]
    INFERRED,
    #[serde(rename = "AMBIGUOUS")]
    AMBIGUOUS,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphDisabledError {
    #[serde(rename = "_tag")]
    pub tag: GraphDisabledErrorTag,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum GraphDisabledErrorTag {
    #[default]
    #[serde(rename = "GraphDisabledError")]
    GraphDisabledError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub confidence: GraphConfidence,
    #[serde(rename = "confidenceScore")]
    pub confidence_score: DurationMillis,
    pub relation: TrimmedNonEmptyString,
    pub source: GraphNodeId,
    pub target: GraphNodeId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphExplanation {
    pub community: Option<GraphCommunity>,
    pub neighbors: Vec<GraphNeighborLink>,
    pub node: GraphNode,
    pub stale: bool,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GraphFileType {
    #[serde(rename = "code")]
    Code,
    #[serde(rename = "document")]
    Document,
    #[serde(rename = "paper")]
    Paper,
    #[serde(rename = "image")]
    Image,
    #[serde(rename = "rationale")]
    Rationale,
    #[serde(rename = "concept")]
    Concept,
    #[serde(rename = "doc_ref")]
    DocRef,
    #[serde(rename = "unknown")]
    UnknownX,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GraphInstallEvent {
    #[serde(rename = "progress")]
    Progress {
        detail: Option<TrimmedNonEmptyString>,
        stage: GraphInstallStage,
    },
    #[serde(rename = "complete")]
    Complete { runtime: GraphRuntimeStatus },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphInstallRuntimeInput {
    #[serde(rename = "interpreterPath")]
    pub interpreter_path: Option<TrimmedNonEmptyString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GraphInstallStage {
    #[serde(rename = "checking")]
    Checking,
    #[serde(rename = "waiting_for_lock")]
    WaitingForLock,
    #[serde(rename = "creating_venv")]
    CreatingVenv,
    #[serde(rename = "installing")]
    Installing,
    #[serde(rename = "validating")]
    Validating,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNeighborLink {
    pub confidence: GraphConfidence,
    #[serde(rename = "confidenceScore")]
    pub confidence_score: DurationMillis,
    pub node: GraphNode,
    pub relation: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    #[serde(rename = "communityId")]
    pub community_id: Option<NonNegativeInt>,
    pub degree: NonNegativeInt,
    #[serde(rename = "fileType")]
    pub file_type: GraphFileType,
    pub id: GraphNodeId,
    pub label: TrimmedNonEmptyString,
    #[serde(rename = "sourceFile")]
    pub source_file: Option<TrimmedNonEmptyString>,
    #[serde(rename = "sourceLine")]
    pub source_line: Option<PositiveInt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GraphNodeId(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNodeNotFoundError {
    #[serde(rename = "_tag")]
    pub tag: GraphNodeNotFoundErrorTag,
    pub reference: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum GraphNodeNotFoundErrorTag {
    #[default]
    #[serde(rename = "GraphNodeNotFoundError")]
    GraphNodeNotFoundError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNodeQueryInput {
    pub cwd: TrimmedNonEmptyString,
    pub node: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNotBuiltError {
    #[serde(rename = "_tag")]
    pub tag: GraphNotBuiltErrorTag,
    pub cwd: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum GraphNotBuiltErrorTag {
    #[default]
    #[serde(rename = "GraphNotBuiltError")]
    GraphNotBuiltError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphPathInput {
    pub cwd: TrimmedNonEmptyString,
    pub from: TrimmedNonEmptyString,
    pub to: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphPathResult {
    pub edges: Vec<GraphEdge>,
    pub nodes: Vec<GraphNode>,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphQueryInput {
    pub cwd: TrimmedNonEmptyString,
    pub limit: i64,
    pub question: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GraphRuntimeSource {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "managed")]
    Managed,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GraphRuntimeState {
    #[serde(rename = "disabled")]
    Disabled,
    #[serde(rename = "missing")]
    Missing,
    #[serde(rename = "installing")]
    Installing,
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "failed")]
    Failed,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphRuntimeStatus {
    pub detail: Option<TrimmedNonEmptyString>,
    #[serde(rename = "interpreterPath")]
    pub interpreter_path: Option<TrimmedNonEmptyString>,
    #[serde(rename = "pythonAvailable")]
    pub python_available: bool,
    pub source: Option<GraphRuntimeSource>,
    pub state: GraphRuntimeState,
    pub version: Option<TrimmedNonEmptyString>,
}

/// Void payload — send `{}` on the wire.
pub type GraphRuntimeStatusPayload = serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphRuntimeUnavailableError {
    #[serde(rename = "_tag")]
    pub tag: GraphRuntimeUnavailableErrorTag,
    pub detail: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum GraphRuntimeUnavailableErrorTag {
    #[default]
    #[serde(rename = "GraphRuntimeUnavailableError")]
    GraphRuntimeUnavailableError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphSearchResult {
    pub matches: Vec<GraphNode>,
    pub stale: bool,
    #[serde(rename = "totalMatches")]
    pub total_matches: NonNegativeInt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphSnapshot {
    #[serde(rename = "builtAt")]
    pub built_at: DurationMillis,
    pub communities: Vec<GraphCommunity>,
    #[serde(rename = "edgeCount")]
    pub edge_count: NonNegativeInt,
    #[serde(rename = "godNodes")]
    pub god_nodes: Vec<GraphNode>,
    #[serde(rename = "nodeCount")]
    pub node_count: NonNegativeInt,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphStatus {
    pub branch: Option<TrimmedNonEmptyString>,
    pub build: GraphBuildStatus,
    pub enabled: bool,
    pub runtime: GraphRuntimeStatus,
    pub snapshot: Option<GraphSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphStorePathError {
    #[serde(rename = "_tag")]
    pub tag: GraphStorePathErrorTag,
    pub path: TrimmedNonEmptyString,
    pub reason: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum GraphStorePathErrorTag {
    #[default]
    #[serde(rename = "GraphStorePathError")]
    GraphStorePathError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphSubgraph {
    pub edges: Vec<GraphEdge>,
    pub nodes: Vec<GraphNode>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphSubgraphInput {
    #[serde(rename = "communityId")]
    pub community_id: Option<NonNegativeInt>,
    pub cwd: TrimmedNonEmptyString,
    pub depth: i64,
    pub limit: i64,
    #[serde(rename = "nodeId")]
    pub node_id: Option<GraphNodeId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphWorkspaceInput {
    pub cwd: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphWorkspaceUnknownError {
    #[serde(rename = "_tag")]
    pub tag: GraphWorkspaceUnknownErrorTag,
    pub cwd: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum GraphWorkspaceUnknownErrorTag {
    #[default]
    #[serde(rename = "GraphWorkspaceUnknownError")]
    GraphWorkspaceUnknownError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeybindingCommand {
    #[serde(rename = "sidebar.toggle")]
    SidebarToggle,
    #[serde(rename = "terminal.toggle")]
    TerminalToggle,
    #[serde(rename = "terminal.split")]
    TerminalSplit,
    #[serde(rename = "terminal.splitVertical")]
    TerminalSplitVertical,
    #[serde(rename = "terminal.new")]
    TerminalNew,
    #[serde(rename = "terminal.close")]
    TerminalClose,
    #[serde(rename = "rightPanel.toggle")]
    RightPanelToggle,
    #[serde(rename = "rightPanel.closeSurface")]
    RightPanelCloseSurface,
    #[serde(rename = "rightPanel.nextSurface")]
    RightPanelNextSurface,
    #[serde(rename = "rightPanel.previousSurface")]
    RightPanelPreviousSurface,
    #[serde(rename = "diff.toggle")]
    DiffToggle,
    #[serde(rename = "search.toggle")]
    SearchToggle,
    #[serde(rename = "graph.toggle")]
    GraphToggle,
    #[serde(rename = "graph.build")]
    GraphBuild,
    #[serde(rename = "editor.showCompletions")]
    EditorShowCompletions,
    #[serde(rename = "file.save")]
    FileSave,
    #[serde(rename = "fileTree.toggleFocus")]
    FileTreeToggleFocus,
    #[serde(rename = "fileTree.newFile")]
    FileTreeNewFile,
    #[serde(rename = "fileTree.newDirectory")]
    FileTreeNewDirectory,
    #[serde(rename = "fileTree.rename")]
    FileTreeRename,
    #[serde(rename = "fileTree.search")]
    FileTreeSearch,
    #[serde(rename = "quickSearch.open")]
    QuickSearchOpen,
    #[serde(rename = "quickSearch.content")]
    QuickSearchContent,
    #[serde(rename = "preview.toggle")]
    PreviewToggle,
    #[serde(rename = "preview.refresh")]
    PreviewRefresh,
    #[serde(rename = "preview.focusUrl")]
    PreviewFocusUrl,
    #[serde(rename = "preview.zoomIn")]
    PreviewZoomIn,
    #[serde(rename = "preview.zoomOut")]
    PreviewZoomOut,
    #[serde(rename = "preview.resetZoom")]
    PreviewResetZoom,
    #[serde(rename = "commandPalette.toggle")]
    CommandPaletteToggle,
    #[serde(rename = "chat.new")]
    ChatNew,
    #[serde(rename = "chat.newLocal")]
    ChatNewLocal,
    #[serde(rename = "workspaceRoots.manage")]
    WorkspaceRootsManage,
    #[serde(rename = "editor.openFavorite")]
    EditorOpenFavorite,
    #[serde(rename = "modelPicker.toggle")]
    ModelPickerToggle,
    #[serde(rename = "modelPicker.jump.1")]
    ModelPickerJump1,
    #[serde(rename = "modelPicker.jump.2")]
    ModelPickerJump2,
    #[serde(rename = "modelPicker.jump.3")]
    ModelPickerJump3,
    #[serde(rename = "modelPicker.jump.4")]
    ModelPickerJump4,
    #[serde(rename = "modelPicker.jump.5")]
    ModelPickerJump5,
    #[serde(rename = "modelPicker.jump.6")]
    ModelPickerJump6,
    #[serde(rename = "modelPicker.jump.7")]
    ModelPickerJump7,
    #[serde(rename = "modelPicker.jump.8")]
    ModelPickerJump8,
    #[serde(rename = "modelPicker.jump.9")]
    ModelPickerJump9,
    #[serde(rename = "thread.previous")]
    ThreadPrevious,
    #[serde(rename = "thread.next")]
    ThreadNext,
    #[serde(rename = "thread.jump.1")]
    ThreadJump1,
    #[serde(rename = "thread.jump.2")]
    ThreadJump2,
    #[serde(rename = "thread.jump.3")]
    ThreadJump3,
    #[serde(rename = "thread.jump.4")]
    ThreadJump4,
    #[serde(rename = "thread.jump.5")]
    ThreadJump5,
    #[serde(rename = "thread.jump.6")]
    ThreadJump6,
    #[serde(rename = "thread.jump.7")]
    ThreadJump7,
    #[serde(rename = "thread.jump.8")]
    ThreadJump8,
    #[serde(rename = "thread.jump.9")]
    ThreadJump9,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeybindingShortcut {
    #[serde(rename = "altKey")]
    pub alt_key: bool,
    #[serde(rename = "ctrlKey")]
    pub ctrl_key: bool,
    pub key: KeybindingValue,
    #[serde(rename = "metaKey")]
    pub meta_key: bool,
    #[serde(rename = "modKey")]
    pub mod_key: bool,
    #[serde(rename = "shiftKey")]
    pub shift_key: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KeybindingValue(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KeybindingWhen(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum KeybindingWhenNode {
    #[serde(rename = "identifier")]
    Identifier { name: String },
    #[serde(rename = "not")]
    Not { node: Box<KeybindingWhenNode> },
    #[serde(rename = "and")]
    And {
        left: Box<KeybindingWhenNode>,
        right: Box<KeybindingWhenNode>,
    },
    #[serde(rename = "or")]
    Or {
        left: Box<KeybindingWhenNode>,
        right: Box<KeybindingWhenNode>,
    },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeybindingsConfigParseError {
    #[serde(rename = "_tag")]
    pub tag: KeybindingsConfigParseErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    #[serde(rename = "configPath")]
    pub config_path: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum KeybindingsConfigParseErrorTag {
    #[default]
    #[serde(rename = "KeybindingsConfigParseError")]
    KeybindingsConfigParseError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaunchEditorInput {
    pub cwd: TrimmedNonEmptyString,
    pub editor: EditorId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspCompletionItem {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "additionalTextEdits")]
    pub additional_text_edits: Option<Option<Vec<LspTextEdit>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub detail: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub documentation: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "filterText")]
    pub filter_text: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "insertText")]
    pub insert_text: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub kind: Option<Option<NonNegativeInt>>,
    pub label: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub range: Option<Option<LspRange>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "resolveData")]
    pub resolve_data: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "sortText")]
    pub sort_text: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspCompletionResult {
    #[serde(rename = "isIncomplete")]
    pub is_incomplete: bool,
    pub items: Vec<LspCompletionItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspDiagnostic {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub code: Option<Option<TrimmedNonEmptyString>>,
    pub message: TrimmedNonEmptyString,
    pub range: LspRange,
    pub severity: NonNegativeInt,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub source: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspDiagnosticsStreamEvent {
    pub diagnostics: Vec<LspDiagnostic>,
    #[serde(rename = "relativePath")]
    pub relative_path: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspDidChangeInput {
    pub contents: TrimmedNonEmptyString,
    pub cwd: TrimmedNonEmptyString,
    #[serde(rename = "relativePath")]
    pub relative_path: TrimmedNonEmptyString,
    pub version: NonNegativeInt,
}

pub type LspDidChangeSuccess = ();

pub type LspDidCloseSuccess = ();

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspDidOpenInput {
    pub contents: TrimmedNonEmptyString,
    pub cwd: TrimmedNonEmptyString,
    #[serde(rename = "relativePath")]
    pub relative_path: TrimmedNonEmptyString,
}

pub type LspDidOpenSuccess = ();

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspDocumentInput {
    pub cwd: TrimmedNonEmptyString,
    #[serde(rename = "relativePath")]
    pub relative_path: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspError {
    #[serde(rename = "_tag")]
    pub tag: LspErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cwd: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub detail: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub failure: Option<Option<LspFailure>>,
    pub message: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "relativePath")]
    pub relative_path: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "serverId")]
    pub server_id: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum LspErrorTag {
    #[default]
    #[serde(rename = "LspError")]
    LspError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LspFailure {
    #[serde(rename = "workspace_root_not_found")]
    WorkspaceRootNotFound,
    #[serde(rename = "unsupported_language")]
    UnsupportedLanguage,
    #[serde(rename = "server_not_installed")]
    ServerNotInstalled,
    #[serde(rename = "server_start_failed")]
    ServerStartFailed,
    #[serde(rename = "server_crashed")]
    ServerCrashed,
    #[serde(rename = "request_failed")]
    RequestFailed,
    #[serde(rename = "request_timed_out")]
    RequestTimedOut,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspFileEdits {
    pub edits: Vec<LspTextEdit>,
    #[serde(rename = "relativePath")]
    pub relative_path: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspFormattingInput {
    pub cwd: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "insertSpaces")]
    pub insert_spaces: Option<Option<bool>>,
    #[serde(rename = "relativePath")]
    pub relative_path: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "tabSize")]
    pub tab_size: Option<Option<NonNegativeInt>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspFormattingResult {
    pub edits: Vec<LspTextEdit>,
}

pub type LspHoverResult = Option<LspHoverResultInline>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspHoverResultInline {
    pub contents: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub range: Option<Option<LspRange>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspLocation {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "absolutePath")]
    pub absolute_path: Option<Option<TrimmedNonEmptyString>>,
    pub range: LspRange,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "relativePath")]
    pub relative_path: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspLocationsResult {
    pub locations: Vec<LspLocation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspPosition {
    pub character: NonNegativeInt,
    pub line: NonNegativeInt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspPositionInput {
    pub cwd: TrimmedNonEmptyString,
    pub position: LspPosition,
    #[serde(rename = "relativePath")]
    pub relative_path: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspRange {
    pub end: LspPosition,
    pub start: LspPosition,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspRenameInput {
    pub cwd: TrimmedNonEmptyString,
    #[serde(rename = "newName")]
    pub new_name: TrimmedNonEmptyString,
    pub position: LspPosition,
    #[serde(rename = "relativePath")]
    pub relative_path: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspResolveCompletionInput {
    pub cwd: TrimmedNonEmptyString,
    #[serde(rename = "relativePath")]
    pub relative_path: TrimmedNonEmptyString,
    #[serde(rename = "resolveData")]
    pub resolve_data: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspServerStatus {
    #[serde(rename = "displayName")]
    pub display_name: TrimmedNonEmptyString,
    #[serde(rename = "serverId")]
    pub server_id: TrimmedNonEmptyString,
    pub state: LspServerStatusState,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LspServerStatusState {
    #[serde(rename = "starting")]
    Starting,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "not_installed")]
    NotInstalled,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspServerStatusPayload {
    pub cwd: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspServerStatusResult {
    pub servers: Vec<LspServerStatus>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "supportedExtensions")]
    pub supported_extensions: Option<Option<Vec<String>>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspSignature {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub documentation: Option<Option<TrimmedNonEmptyString>>,
    pub label: TrimmedNonEmptyString,
    pub parameters: Vec<LspSignatureParameter>,
}

pub type LspSignatureHelpResult = Option<LspSignatureHelpResultInline>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspSignatureHelpResultInline {
    #[serde(rename = "activeParameter")]
    pub active_parameter: NonNegativeInt,
    #[serde(rename = "activeSignature")]
    pub active_signature: NonNegativeInt,
    pub signatures: Vec<LspSignature>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspSignatureParameter {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub documentation: Option<Option<TrimmedNonEmptyString>>,
    pub label: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspSubscribeDiagnosticsInput {
    pub cwd: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspTextEdit {
    #[serde(rename = "newText")]
    pub new_text: TrimmedNonEmptyString,
    pub range: LspRange,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LspWorkspaceEditResult {
    pub files: Vec<LspFileEdits>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MessageId(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelCapabilities {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "optionDescriptors")]
    pub option_descriptors: Option<Option<Vec<ProviderOptionDescriptor>>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelSelection {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "instanceId")]
    pub instance_id: Option<Option<serde_json::Value>>,
    pub model: serde_json::Value,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub options: Option<Option<serde_json::Value>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub provider: Option<Option<serde_json::Value>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Copy, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NonNegativeInt(pub i64);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrchestrationAggregateKind {
    #[serde(rename = "project")]
    Project,
    #[serde(rename = "thread")]
    Thread,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationCheckpointFile {
    pub additions: NonNegativeInt,
    pub deletions: NonNegativeInt,
    pub kind: TrimmedNonEmptyString,
    pub path: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrchestrationCheckpointStatus {
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "missing")]
    Missing,
    #[serde(rename = "error")]
    Error,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationCheckpointSummary {
    #[serde(rename = "assistantMessageId")]
    pub assistant_message_id: Option<MessageId>,
    #[serde(rename = "checkpointRef")]
    pub checkpoint_ref: CheckpointRef,
    #[serde(rename = "checkpointTurnCount")]
    pub checkpoint_turn_count: NonNegativeInt,
    #[serde(rename = "completedAt")]
    pub completed_at: TrimmedNonEmptyString,
    pub files: Vec<OrchestrationCheckpointFile>,
    pub status: OrchestrationCheckpointStatus,
    #[serde(rename = "turnId")]
    pub turn_id: TurnId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationDispatchCommandError {
    #[serde(rename = "_tag")]
    pub tag: OrchestrationDispatchCommandErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    pub message: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum OrchestrationDispatchCommandErrorTag {
    #[default]
    #[serde(rename = "OrchestrationDispatchCommandError")]
    OrchestrationDispatchCommandError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OrchestrationEvent {
    #[serde(rename = "project.created")]
    ProjectCreated {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventProjectCreatedAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ProjectCreatedPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "project.meta-updated")]
    ProjectMetaUpdated {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventProjectMetaUpdatedAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ProjectMetaUpdatedPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "project.deleted")]
    ProjectDeleted {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventProjectDeletedAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ProjectDeletedPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.created")]
    ThreadCreated {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadCreatedAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadCreatedPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.deleted")]
    ThreadDeleted {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadDeletedAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadDeletedPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.archived")]
    ThreadArchived {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadArchivedAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadArchivedPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.unarchived")]
    ThreadUnarchived {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadUnarchivedAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadUnarchivedPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.settled")]
    ThreadSettled {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadSettledAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadSettledPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.unsettled")]
    ThreadUnsettled {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadUnsettledAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadUnsettledPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.snoozed")]
    ThreadSnoozed {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadSnoozedAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadSnoozedPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.unsnoozed")]
    ThreadUnsnoozed {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadUnsnoozedAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadUnsnoozedPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.meta-updated")]
    ThreadMetaUpdated {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadMetaUpdatedAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadMetaUpdatedPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.runtime-mode-set")]
    ThreadRuntimeModeSet {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadRuntimeModeSetAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadRuntimeModeSetPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.interaction-mode-set")]
    ThreadInteractionModeSet {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadInteractionModeSetAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadInteractionModeSetPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.message-sent")]
    ThreadMessageSent {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadMessageSentAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadMessageSentPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.turn-start-requested")]
    ThreadTurnStartRequested {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadTurnStartRequestedAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadTurnStartRequestedPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.turn-interrupt-requested")]
    ThreadTurnInterruptRequested {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadTurnInterruptRequestedAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadTurnInterruptRequestedPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.approval-response-requested")]
    ThreadApprovalResponseRequested {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadApprovalResponseRequestedAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadApprovalResponseRequestedPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.user-input-response-requested")]
    ThreadUserInputResponseRequested {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadUserInputResponseRequestedAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: OrchestrationEventThreadUserInputResponseRequestedPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.checkpoint-revert-requested")]
    ThreadCheckpointRevertRequested {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadCheckpointRevertRequestedAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadCheckpointRevertRequestedPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.reverted")]
    ThreadReverted {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadRevertedAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadRevertedPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.session-stop-requested")]
    ThreadSessionStopRequested {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadSessionStopRequestedAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadSessionStopRequestedPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.session-set")]
    ThreadSessionSet {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadSessionSetAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadSessionSetPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.proposed-plan-upserted")]
    ThreadProposedPlanUpserted {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadProposedPlanUpsertedAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadProposedPlanUpsertedPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.turn-diff-completed")]
    ThreadTurnDiffCompleted {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadTurnDiffCompletedAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadTurnDiffCompletedPayload,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread.activity-appended")]
    ThreadActivityAppended {
        #[serde(rename = "aggregateId")]
        aggregate_id: OrchestrationEventThreadActivityAppendedAggregateId,
        #[serde(rename = "aggregateKind")]
        aggregate_kind: OrchestrationAggregateKind,
        #[serde(rename = "causationEventId")]
        causation_event_id: Option<EventId>,
        #[serde(rename = "commandId")]
        command_id: Option<CommandId>,
        #[serde(rename = "correlationId")]
        correlation_id: Option<CommandId>,
        #[serde(rename = "eventId")]
        event_id: EventId,
        metadata: OrchestrationEventMetadata,
        #[serde(rename = "occurredAt")]
        occurred_at: TrimmedNonEmptyString,
        payload: ThreadActivityAppendedPayload,
        sequence: NonNegativeInt,
    },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventProjectCreatedAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventProjectMetaUpdatedAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventProjectDeletedAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadCreatedAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadDeletedAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadArchivedAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadUnarchivedAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadSettledAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadUnsettledAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadSnoozedAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadUnsnoozedAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadMetaUpdatedAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadRuntimeModeSetAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadInteractionModeSetAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadMessageSentAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadTurnStartRequestedAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadTurnInterruptRequestedAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadApprovalResponseRequestedAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadUserInputResponseRequestedAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationEventThreadUserInputResponseRequestedPayload {
    pub answers: ProviderUserInputAnswers,
    #[serde(rename = "createdAt")]
    pub created_at: TrimmedNonEmptyString,
    #[serde(rename = "requestId")]
    pub request_id: ApprovalRequestId,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadCheckpointRevertRequestedAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadRevertedAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadSessionStopRequestedAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadSessionSetAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadProposedPlanUpsertedAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadTurnDiffCompletedAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationEventThreadActivityAppendedAggregateId {
    ProjectId(ProjectId),
    ThreadId(ThreadId),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationEventMetadata {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "adapterKey")]
    pub adapter_key: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "ingestedAt")]
    pub ingested_at: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "providerItemId")]
    pub provider_item_id: Option<Option<ProviderItemId>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "providerTurnId")]
    pub provider_turn_id: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "requestId")]
    pub request_id: Option<Option<ApprovalRequestId>>,
}

/// Void payload — send `{}` on the wire.
pub type OrchestrationGetArchivedShellSnapshotPayload = serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationGetFullThreadDiffError {
    #[serde(rename = "_tag")]
    pub tag: OrchestrationGetFullThreadDiffErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    pub message: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum OrchestrationGetFullThreadDiffErrorTag {
    #[default]
    #[serde(rename = "OrchestrationGetFullThreadDiffError")]
    OrchestrationGetFullThreadDiffError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationGetFullThreadDiffInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "ignoreWhitespace")]
    pub ignore_whitespace: Option<bool>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "toTurnCount")]
    pub to_turn_count: NonNegativeInt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationGetSnapshotError {
    #[serde(rename = "_tag")]
    pub tag: OrchestrationGetSnapshotErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    pub message: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum OrchestrationGetSnapshotErrorTag {
    #[default]
    #[serde(rename = "OrchestrationGetSnapshotError")]
    OrchestrationGetSnapshotError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationGetTurnDiffError {
    #[serde(rename = "_tag")]
    pub tag: OrchestrationGetTurnDiffErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    pub message: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum OrchestrationGetTurnDiffErrorTag {
    #[default]
    #[serde(rename = "OrchestrationGetTurnDiffError")]
    OrchestrationGetTurnDiffError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationLatestTurn {
    #[serde(rename = "assistantMessageId")]
    pub assistant_message_id: Option<MessageId>,
    #[serde(rename = "completedAt")]
    pub completed_at: Option<TrimmedNonEmptyString>,
    #[serde(rename = "requestedAt")]
    pub requested_at: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "sourceProposedPlan")]
    pub source_proposed_plan: Option<Option<OrchestrationLatestTurnSourceProposedPlan>>,
    #[serde(rename = "startedAt")]
    pub started_at: Option<TrimmedNonEmptyString>,
    pub state: OrchestrationLatestTurnState,
    #[serde(rename = "turnId")]
    pub turn_id: TurnId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationLatestTurnSourceProposedPlan {
    #[serde(rename = "planId")]
    pub plan_id: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrchestrationLatestTurnState {
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "interrupted")]
    Interrupted,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "error")]
    Error,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationMessage {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub attachments: Option<Option<Vec<ChatAttachment>>>,
    #[serde(rename = "createdAt")]
    pub created_at: TrimmedNonEmptyString,
    pub id: MessageId,
    pub role: OrchestrationMessageRole,
    pub streaming: bool,
    pub text: TrimmedNonEmptyString,
    #[serde(rename = "turnId")]
    pub turn_id: Option<TurnId>,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrchestrationMessageRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "system")]
    System,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationMessageSearchMatch {
    #[serde(rename = "messageId")]
    pub message_id: MessageId,
    pub role: OrchestrationMessageRole,
    pub snippet: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "threadTitle")]
    pub thread_title: TrimmedNonEmptyString,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationProjectShell {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "additionalRoots")]
    pub additional_roots: Option<Option<Vec<OrchestrationProjectShellAdditionalRoots>>>,
    #[serde(rename = "createdAt")]
    pub created_at: TrimmedNonEmptyString,
    #[serde(rename = "defaultModelSelection")]
    pub default_model_selection: Option<ModelSelection>,
    pub id: ProjectId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "repositoryIdentity")]
    pub repository_identity: Option<Option<Option<RepositoryIdentity>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "resolvedAdditionalRoots")]
    pub resolved_additional_roots: Option<Option<Vec<ResolvedWorkspaceRoot>>>,
    pub scripts: Vec<ProjectScript>,
    pub title: TrimmedNonEmptyString,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
    #[serde(rename = "workspaceRoot")]
    pub workspace_root: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum OrchestrationProjectShellAdditionalRoots {
    #[serde(rename = "project")]
    Project {
        #[serde(rename = "projectId")]
        project_id: String,
    },
    #[serde(rename = "path")]
    Path { path: String },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationProposedPlan {
    #[serde(rename = "createdAt")]
    pub created_at: TrimmedNonEmptyString,
    pub id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "implementationThreadId")]
    pub implementation_thread_id: Option<Option<Option<String>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "implementedAt")]
    pub implemented_at: Option<Option<Option<TrimmedNonEmptyString>>>,
    #[serde(rename = "planMarkdown")]
    pub plan_markdown: TrimmedNonEmptyString,
    #[serde(rename = "turnId")]
    pub turn_id: Option<TurnId>,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationReplayEventsError {
    #[serde(rename = "_tag")]
    pub tag: OrchestrationReplayEventsErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    pub message: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum OrchestrationReplayEventsErrorTag {
    #[default]
    #[serde(rename = "OrchestrationReplayEventsError")]
    OrchestrationReplayEventsError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationReplayEventsInput {
    #[serde(rename = "fromSequenceExclusive")]
    pub from_sequence_exclusive: NonNegativeInt,
}

pub type OrchestrationReplayEventsSuccess = Vec<OrchestrationEvent>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationSearchMessagesError {
    #[serde(rename = "_tag")]
    pub tag: OrchestrationSearchMessagesErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    pub message: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum OrchestrationSearchMessagesErrorTag {
    #[default]
    #[serde(rename = "OrchestrationSearchMessagesError")]
    OrchestrationSearchMessagesError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationSearchMessagesInput {
    pub limit: i64,
    pub query: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationSearchMessagesResult {
    pub matches: Vec<OrchestrationMessageSearchMatch>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationSession {
    #[serde(rename = "activeTurnId")]
    pub active_turn_id: Option<TurnId>,
    #[serde(rename = "lastError")]
    pub last_error: Option<TrimmedNonEmptyString>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "providerInstanceId")]
    pub provider_instance_id: Option<Option<ProviderInstanceId>>,
    #[serde(rename = "providerName")]
    pub provider_name: Option<TrimmedNonEmptyString>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "runtimeMode")]
    pub runtime_mode: Option<Option<RuntimeMode>>,
    pub status: OrchestrationSessionStatus,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrchestrationSessionStatus {
    #[serde(rename = "idle")]
    Idle,
    #[serde(rename = "starting")]
    Starting,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "interrupted")]
    Interrupted,
    #[serde(rename = "stopped")]
    Stopped,
    #[serde(rename = "error")]
    Error,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationShellSnapshot {
    pub projects: Vec<OrchestrationProjectShell>,
    #[serde(rename = "snapshotSequence")]
    pub snapshot_sequence: NonNegativeInt,
    pub threads: Vec<OrchestrationThreadShell>,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum OrchestrationShellStreamEvent {
    #[serde(rename = "project-upserted")]
    ProjectUpserted {
        project: OrchestrationProjectShell,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "project-removed")]
    ProjectRemoved {
        #[serde(rename = "projectId")]
        project_id: ProjectId,
        sequence: NonNegativeInt,
    },
    #[serde(rename = "thread-upserted")]
    ThreadUpserted {
        sequence: NonNegativeInt,
        thread: OrchestrationThreadShell,
    },
    #[serde(rename = "thread-removed")]
    ThreadRemoved {
        sequence: NonNegativeInt,
        #[serde(rename = "threadId")]
        thread_id: ThreadId,
    },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestrationShellStreamItem {
    Synchronized {
        kind: OrchestrationShellStreamItemSynchronizedKind,
    },
    Snapshot {
        kind: OrchestrationShellStreamItemSnapshotKind,
        snapshot: OrchestrationShellSnapshot,
    },
    OrchestrationShellStreamEvent(OrchestrationShellStreamEvent),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum OrchestrationShellStreamItemSynchronizedKind {
    #[default]
    #[serde(rename = "synchronized")]
    Synchronized,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum OrchestrationShellStreamItemSnapshotKind {
    #[default]
    #[serde(rename = "snapshot")]
    Snapshot,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationSubscribeShellInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "afterSequence")]
    pub after_sequence: Option<NonNegativeInt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "requestCompletionMarker")]
    pub request_completion_marker: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationSubscribeThreadInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "afterSequence")]
    pub after_sequence: Option<NonNegativeInt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "requestCompletionMarker")]
    pub request_completion_marker: Option<bool>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationThread {
    pub activities: Vec<OrchestrationThreadActivity>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "additionalRoots")]
    pub additional_roots: Option<Option<Vec<OrchestrationThreadAdditionalRoots>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "archivedAt")]
    pub archived_at: Option<Option<Option<TrimmedNonEmptyString>>>,
    pub branch: Option<TrimmedNonEmptyString>,
    pub checkpoints: Vec<OrchestrationCheckpointSummary>,
    #[serde(rename = "createdAt")]
    pub created_at: TrimmedNonEmptyString,
    #[serde(rename = "deletedAt")]
    pub deleted_at: Option<TrimmedNonEmptyString>,
    pub id: ThreadId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "interactionMode")]
    pub interaction_mode: Option<Option<ProviderInteractionMode>>,
    #[serde(rename = "latestTurn")]
    pub latest_turn: Option<OrchestrationLatestTurn>,
    pub messages: Vec<OrchestrationMessage>,
    #[serde(rename = "modelSelection")]
    pub model_selection: ModelSelection,
    #[serde(rename = "projectId")]
    pub project_id: ProjectId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "proposedPlans")]
    pub proposed_plans: Option<Option<Vec<OrchestrationThreadProposedPlans>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "resolvedAdditionalRoots")]
    pub resolved_additional_roots: Option<Option<Vec<ResolvedWorkspaceRoot>>>,
    #[serde(rename = "runtimeMode")]
    pub runtime_mode: RuntimeMode,
    pub session: Option<OrchestrationSession>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "settledAt")]
    pub settled_at: Option<Option<Option<TrimmedNonEmptyString>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "settledOverride")]
    pub settled_override: Option<Option<Option<OrchestrationThreadSettledOverride>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "snoozedAt")]
    pub snoozed_at: Option<Option<Option<TrimmedNonEmptyString>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "snoozedUntil")]
    pub snoozed_until: Option<Option<Option<TrimmedNonEmptyString>>>,
    pub title: TrimmedNonEmptyString,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
    #[serde(rename = "worktreePath")]
    pub worktree_path: Option<TrimmedNonEmptyString>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum OrchestrationThreadAdditionalRoots {
    #[serde(rename = "project")]
    Project {
        #[serde(rename = "projectId")]
        project_id: String,
    },
    #[serde(rename = "path")]
    Path { path: String },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationThreadProposedPlans {
    #[serde(rename = "createdAt")]
    pub created_at: TrimmedNonEmptyString,
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "implementationThreadId")]
    pub implementation_thread_id: Option<Option<Option<String>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "implementedAt")]
    pub implemented_at: Option<Option<Option<TrimmedNonEmptyString>>>,
    #[serde(rename = "planMarkdown")]
    pub plan_markdown: String,
    #[serde(rename = "turnId")]
    pub turn_id: Option<String>,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrchestrationThreadSettledOverride {
    #[serde(rename = "settled")]
    Settled,
    #[serde(rename = "active")]
    Active,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationThreadActivity {
    #[serde(rename = "createdAt")]
    pub created_at: TrimmedNonEmptyString,
    pub id: EventId,
    pub kind: TrimmedNonEmptyString,
    pub payload: serde_json::Value,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub sequence: Option<Option<NonNegativeInt>>,
    pub summary: TrimmedNonEmptyString,
    pub tone: OrchestrationThreadActivityTone,
    #[serde(rename = "turnId")]
    pub turn_id: Option<TurnId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrchestrationThreadActivityTone {
    #[serde(rename = "info")]
    Info,
    #[serde(rename = "tool")]
    Tool,
    #[serde(rename = "approval")]
    Approval,
    #[serde(rename = "error")]
    Error,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationThreadDetailSnapshot {
    #[serde(rename = "snapshotSequence")]
    pub snapshot_sequence: NonNegativeInt,
    pub thread: OrchestrationThread,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationThreadShell {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "additionalRoots")]
    pub additional_roots: Option<Option<Vec<OrchestrationThreadShellAdditionalRoots>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "archivedAt")]
    pub archived_at: Option<Option<Option<TrimmedNonEmptyString>>>,
    pub branch: Option<TrimmedNonEmptyString>,
    #[serde(rename = "createdAt")]
    pub created_at: TrimmedNonEmptyString,
    #[serde(rename = "hasActionableProposedPlan")]
    pub has_actionable_proposed_plan: bool,
    #[serde(rename = "hasPendingApprovals")]
    pub has_pending_approvals: bool,
    #[serde(rename = "hasPendingUserInput")]
    pub has_pending_user_input: bool,
    pub id: ThreadId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "interactionMode")]
    pub interaction_mode: Option<Option<ProviderInteractionMode>>,
    #[serde(rename = "latestTurn")]
    pub latest_turn: Option<OrchestrationLatestTurn>,
    #[serde(rename = "latestUserMessageAt")]
    pub latest_user_message_at: Option<TrimmedNonEmptyString>,
    #[serde(rename = "modelSelection")]
    pub model_selection: ModelSelection,
    #[serde(rename = "projectId")]
    pub project_id: ProjectId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "resolvedAdditionalRoots")]
    pub resolved_additional_roots: Option<Option<Vec<ResolvedWorkspaceRoot>>>,
    #[serde(rename = "runtimeMode")]
    pub runtime_mode: RuntimeMode,
    pub session: Option<OrchestrationSession>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "settledAt")]
    pub settled_at: Option<Option<Option<TrimmedNonEmptyString>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "settledOverride")]
    pub settled_override: Option<Option<Option<OrchestrationThreadShellSettledOverride>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "snoozedAt")]
    pub snoozed_at: Option<Option<Option<TrimmedNonEmptyString>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "snoozedUntil")]
    pub snoozed_until: Option<Option<Option<TrimmedNonEmptyString>>>,
    pub title: TrimmedNonEmptyString,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
    #[serde(rename = "worktreePath")]
    pub worktree_path: Option<TrimmedNonEmptyString>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum OrchestrationThreadShellAdditionalRoots {
    #[serde(rename = "project")]
    Project {
        #[serde(rename = "projectId")]
        project_id: String,
    },
    #[serde(rename = "path")]
    Path { path: String },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrchestrationThreadShellSettledOverride {
    #[serde(rename = "settled")]
    Settled,
    #[serde(rename = "active")]
    Active,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum OrchestrationThreadStreamItem {
    #[serde(rename = "synchronized")]
    Synchronized {},
    #[serde(rename = "snapshot")]
    Snapshot {
        snapshot: OrchestrationThreadDetailSnapshot,
    },
    #[serde(rename = "event")]
    Event { event: OrchestrationEvent },
    #[serde(rename = "ephemeral-delta")]
    EphemeralDelta {
        #[serde(rename = "createdAt")]
        created_at: TrimmedNonEmptyString,
        delta: TrimmedNonEmptyString,
        #[serde(rename = "messageId")]
        message_id: MessageId,
        offset: NonNegativeInt,
        #[serde(rename = "threadId")]
        thread_id: ThreadId,
        #[serde(rename = "turnId")]
        turn_id: Option<TurnId>,
    },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationTurnDiffRange {
    #[serde(rename = "fromTurnCount")]
    pub from_turn_count: NonNegativeInt,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "ignoreWhitespace")]
    pub ignore_whitespace: Option<bool>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "toTurnCount")]
    pub to_turn_count: NonNegativeInt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestrationTurnDiffRange1 {
    pub diff: TrimmedNonEmptyString,
    #[serde(rename = "fromTurnCount")]
    pub from_turn_count: NonNegativeInt,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "toTurnCount")]
    pub to_turn_count: NonNegativeInt,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Copy, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PositiveInt(pub i64);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewAutomationClientDisconnectedError {
    #[serde(rename = "_tag")]
    pub tag: PreviewAutomationClientDisconnectedErrorTag,
    #[serde(rename = "clientId")]
    pub client_id: TrimmedNonEmptyString,
    #[serde(rename = "connectionId")]
    pub connection_id: PreviewAutomationConnectionId,
    #[serde(rename = "environmentId")]
    pub environment_id: EnvironmentId,
    pub operation: PreviewAutomationOperation,
    #[serde(rename = "providerInstanceId")]
    pub provider_instance_id: ProviderInstanceId,
    #[serde(rename = "providerSessionId")]
    pub provider_session_id: TrimmedNonEmptyString,
    #[serde(rename = "requestId")]
    pub request_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "tabId")]
    pub tab_id: Option<Option<PreviewTabId>>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PreviewAutomationClientDisconnectedErrorTag {
    #[default]
    #[serde(rename = "PreviewAutomationClientDisconnectedError")]
    PreviewAutomationClientDisconnectedError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PreviewAutomationClientId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PreviewAutomationConnectionId(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewAutomationControlInterruptedError {
    #[serde(rename = "_tag")]
    pub tag: PreviewAutomationControlInterruptedErrorTag,
    pub cause: serde_json::Value,
    #[serde(rename = "clientId")]
    pub client_id: TrimmedNonEmptyString,
    #[serde(rename = "connectionId")]
    pub connection_id: PreviewAutomationConnectionId,
    #[serde(rename = "environmentId")]
    pub environment_id: EnvironmentId,
    pub operation: PreviewAutomationOperation,
    #[serde(rename = "providerInstanceId")]
    pub provider_instance_id: ProviderInstanceId,
    #[serde(rename = "providerSessionId")]
    pub provider_session_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "remoteDetailKind")]
    pub remote_detail_kind:
        Option<Option<PreviewAutomationControlInterruptedErrorRemoteDetailKind>>,
    #[serde(rename = "remoteMessageLength")]
    pub remote_message_length: i64,
    #[serde(rename = "remoteTag")]
    pub remote_tag: TrimmedNonEmptyString,
    #[serde(rename = "requestId")]
    pub request_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "tabId")]
    pub tab_id: Option<Option<PreviewTabId>>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PreviewAutomationControlInterruptedErrorTag {
    #[default]
    #[serde(rename = "PreviewAutomationControlInterruptedError")]
    PreviewAutomationControlInterruptedError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreviewAutomationControlInterruptedErrorRemoteDetailKind {
    #[serde(rename = "null")]
    Null,
    #[serde(rename = "array")]
    Array,
    #[serde(rename = "object")]
    Object,
    #[serde(rename = "string")]
    String,
    #[serde(rename = "number")]
    Number,
    #[serde(rename = "boolean")]
    Boolean,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PreviewAutomationError {
    PreviewAutomationUnavailableError(PreviewAutomationUnavailableError),
    PreviewAutomationNoAvailableHostError(PreviewAutomationNoAvailableHostError),
    PreviewAutomationUnsupportedClientError(PreviewAutomationUnsupportedClientError),
    PreviewAutomationTabNotFoundError(PreviewAutomationTabNotFoundError),
    PreviewAutomationTimeoutError(PreviewAutomationTimeoutError),
    PreviewAutomationControlInterruptedError(PreviewAutomationControlInterruptedError),
    PreviewAutomationExecutionError(PreviewAutomationExecutionError),
    PreviewAutomationInvalidSelectorError(PreviewAutomationInvalidSelectorError),
    PreviewAutomationTargetNotEditableError(PreviewAutomationTargetNotEditableError),
    PreviewAutomationResultTooLargeError(PreviewAutomationResultTooLargeError),
    PreviewAutomationClientDisconnectedError(PreviewAutomationClientDisconnectedError),
    PreviewAutomationRequestQueueClosedError(PreviewAutomationRequestQueueClosedError),
    PreviewAutomationRemoteUnavailableError(PreviewAutomationRemoteUnavailableError),
    PreviewAutomationMalformedResponseError(PreviewAutomationMalformedResponseError),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewAutomationExecutionError {
    #[serde(rename = "_tag")]
    pub tag: PreviewAutomationExecutionErrorTag,
    pub cause: serde_json::Value,
    #[serde(rename = "clientId")]
    pub client_id: TrimmedNonEmptyString,
    #[serde(rename = "connectionId")]
    pub connection_id: PreviewAutomationConnectionId,
    #[serde(rename = "environmentId")]
    pub environment_id: EnvironmentId,
    pub operation: PreviewAutomationOperation,
    #[serde(rename = "providerInstanceId")]
    pub provider_instance_id: ProviderInstanceId,
    #[serde(rename = "providerSessionId")]
    pub provider_session_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "remoteDetailKind")]
    pub remote_detail_kind: Option<Option<PreviewAutomationExecutionErrorRemoteDetailKind>>,
    #[serde(rename = "remoteMessageLength")]
    pub remote_message_length: i64,
    #[serde(rename = "remoteTag")]
    pub remote_tag: TrimmedNonEmptyString,
    #[serde(rename = "requestId")]
    pub request_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "tabId")]
    pub tab_id: Option<Option<PreviewTabId>>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PreviewAutomationExecutionErrorTag {
    #[default]
    #[serde(rename = "PreviewAutomationExecutionError")]
    PreviewAutomationExecutionError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreviewAutomationExecutionErrorRemoteDetailKind {
    #[serde(rename = "null")]
    Null,
    #[serde(rename = "array")]
    Array,
    #[serde(rename = "object")]
    Object,
    #[serde(rename = "string")]
    String,
    #[serde(rename = "number")]
    Number,
    #[serde(rename = "boolean")]
    Boolean,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

pub type PreviewAutomationFocusHostSuccess = ();

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewAutomationHost {
    #[serde(rename = "clientId")]
    pub client_id: PreviewAutomationClientId,
    #[serde(rename = "environmentId")]
    pub environment_id: EnvironmentId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "supportedOperations")]
    pub supported_operations: Option<Option<Vec<PreviewAutomationOperation>>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewAutomationHostFocus {
    #[serde(rename = "clientId")]
    pub client_id: PreviewAutomationClientId,
    #[serde(rename = "connectionId")]
    pub connection_id: PreviewAutomationConnectionId,
    #[serde(rename = "environmentId")]
    pub environment_id: EnvironmentId,
    pub focused: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewAutomationInvalidSelectorError {
    #[serde(rename = "_tag")]
    pub tag: PreviewAutomationInvalidSelectorErrorTag,
    pub cause: serde_json::Value,
    #[serde(rename = "clientId")]
    pub client_id: TrimmedNonEmptyString,
    #[serde(rename = "connectionId")]
    pub connection_id: PreviewAutomationConnectionId,
    #[serde(rename = "environmentId")]
    pub environment_id: EnvironmentId,
    pub operation: PreviewAutomationOperation,
    #[serde(rename = "providerInstanceId")]
    pub provider_instance_id: ProviderInstanceId,
    #[serde(rename = "providerSessionId")]
    pub provider_session_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "remoteDetailKind")]
    pub remote_detail_kind: Option<Option<PreviewAutomationInvalidSelectorErrorRemoteDetailKind>>,
    #[serde(rename = "remoteMessageLength")]
    pub remote_message_length: i64,
    #[serde(rename = "remoteTag")]
    pub remote_tag: TrimmedNonEmptyString,
    #[serde(rename = "requestId")]
    pub request_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "selectorKind")]
    pub selector_kind: Option<Option<PreviewAutomationInvalidSelectorErrorSelectorKind>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "selectorLength")]
    pub selector_length: Option<Option<i64>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "tabId")]
    pub tab_id: Option<Option<PreviewTabId>>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PreviewAutomationInvalidSelectorErrorTag {
    #[default]
    #[serde(rename = "PreviewAutomationInvalidSelectorError")]
    PreviewAutomationInvalidSelectorError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreviewAutomationInvalidSelectorErrorRemoteDetailKind {
    #[serde(rename = "null")]
    Null,
    #[serde(rename = "array")]
    Array,
    #[serde(rename = "object")]
    Object,
    #[serde(rename = "string")]
    String,
    #[serde(rename = "number")]
    Number,
    #[serde(rename = "boolean")]
    Boolean,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreviewAutomationInvalidSelectorErrorSelectorKind {
    #[serde(rename = "locator")]
    Locator,
    #[serde(rename = "selector")]
    Selector,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewAutomationMalformedResponseError {
    #[serde(rename = "_tag")]
    pub tag: PreviewAutomationMalformedResponseErrorTag,
    #[serde(rename = "clientId")]
    pub client_id: TrimmedNonEmptyString,
    #[serde(rename = "connectionId")]
    pub connection_id: PreviewAutomationConnectionId,
    #[serde(rename = "environmentId")]
    pub environment_id: EnvironmentId,
    pub operation: PreviewAutomationOperation,
    #[serde(rename = "providerInstanceId")]
    pub provider_instance_id: ProviderInstanceId,
    #[serde(rename = "providerSessionId")]
    pub provider_session_id: TrimmedNonEmptyString,
    #[serde(rename = "requestId")]
    pub request_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "tabId")]
    pub tab_id: Option<Option<PreviewTabId>>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PreviewAutomationMalformedResponseErrorTag {
    #[default]
    #[serde(rename = "PreviewAutomationMalformedResponseError")]
    PreviewAutomationMalformedResponseError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewAutomationNoAvailableHostError {
    #[serde(rename = "_tag")]
    pub tag: PreviewAutomationNoAvailableHostErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "clientId")]
    pub client_id: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "connectionId")]
    pub connection_id: Option<Option<PreviewAutomationConnectionId>>,
    #[serde(rename = "environmentId")]
    pub environment_id: EnvironmentId,
    pub operation: PreviewAutomationOperation,
    #[serde(rename = "providerInstanceId")]
    pub provider_instance_id: ProviderInstanceId,
    #[serde(rename = "providerSessionId")]
    pub provider_session_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "remoteDetailKind")]
    pub remote_detail_kind: Option<Option<PreviewAutomationNoAvailableHostErrorRemoteDetailKind>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "remoteMessageLength")]
    pub remote_message_length: Option<Option<i64>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "remoteTag")]
    pub remote_tag: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "requestId")]
    pub request_id: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "tabId")]
    pub tab_id: Option<Option<PreviewTabId>>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: Option<Option<i64>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PreviewAutomationNoAvailableHostErrorTag {
    #[default]
    #[serde(rename = "PreviewAutomationNoAvailableHostError")]
    PreviewAutomationNoAvailableHostError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreviewAutomationNoAvailableHostErrorRemoteDetailKind {
    #[serde(rename = "null")]
    Null,
    #[serde(rename = "array")]
    Array,
    #[serde(rename = "object")]
    Object,
    #[serde(rename = "string")]
    String,
    #[serde(rename = "number")]
    Number,
    #[serde(rename = "boolean")]
    Boolean,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreviewAutomationOperation {
    #[serde(rename = "status")]
    Status,
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "navigate")]
    Navigate,
    #[serde(rename = "snapshot")]
    Snapshot,
    #[serde(rename = "click")]
    Click,
    #[serde(rename = "type")]
    Type,
    #[serde(rename = "press")]
    Press,
    #[serde(rename = "scroll")]
    Scroll,
    #[serde(rename = "evaluate")]
    Evaluate,
    #[serde(rename = "waitFor")]
    WaitFor,
    #[serde(rename = "recordingStart")]
    RecordingStart,
    #[serde(rename = "recordingStop")]
    RecordingStop,
    #[serde(rename = "resize")]
    Resize,
    #[serde(rename = "setColorScheme")]
    SetColorScheme,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewAutomationRemoteUnavailableError {
    #[serde(rename = "_tag")]
    pub tag: PreviewAutomationRemoteUnavailableErrorTag,
    pub cause: serde_json::Value,
    #[serde(rename = "clientId")]
    pub client_id: TrimmedNonEmptyString,
    #[serde(rename = "connectionId")]
    pub connection_id: PreviewAutomationConnectionId,
    #[serde(rename = "environmentId")]
    pub environment_id: EnvironmentId,
    pub operation: PreviewAutomationOperation,
    #[serde(rename = "providerInstanceId")]
    pub provider_instance_id: ProviderInstanceId,
    #[serde(rename = "providerSessionId")]
    pub provider_session_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "remoteDetailKind")]
    pub remote_detail_kind: Option<Option<PreviewAutomationRemoteUnavailableErrorRemoteDetailKind>>,
    #[serde(rename = "remoteMessageLength")]
    pub remote_message_length: i64,
    #[serde(rename = "remoteTag")]
    pub remote_tag: TrimmedNonEmptyString,
    #[serde(rename = "requestId")]
    pub request_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "tabId")]
    pub tab_id: Option<Option<PreviewTabId>>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PreviewAutomationRemoteUnavailableErrorTag {
    #[default]
    #[serde(rename = "PreviewAutomationRemoteUnavailableError")]
    PreviewAutomationRemoteUnavailableError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreviewAutomationRemoteUnavailableErrorRemoteDetailKind {
    #[serde(rename = "null")]
    Null,
    #[serde(rename = "array")]
    Array,
    #[serde(rename = "object")]
    Object,
    #[serde(rename = "string")]
    String,
    #[serde(rename = "number")]
    Number,
    #[serde(rename = "boolean")]
    Boolean,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewAutomationRequest {
    pub input: serde_json::Value,
    pub operation: PreviewAutomationOperation,
    #[serde(rename = "requestId")]
    pub request_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "tabId")]
    pub tab_id: Option<Option<PreviewTabId>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "tabIdExplicit")]
    pub tab_id_explicit: Option<Option<bool>>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewAutomationRequestQueueClosedError {
    #[serde(rename = "_tag")]
    pub tag: PreviewAutomationRequestQueueClosedErrorTag,
    #[serde(rename = "clientId")]
    pub client_id: TrimmedNonEmptyString,
    #[serde(rename = "connectionId")]
    pub connection_id: PreviewAutomationConnectionId,
    #[serde(rename = "environmentId")]
    pub environment_id: EnvironmentId,
    pub operation: PreviewAutomationOperation,
    #[serde(rename = "providerInstanceId")]
    pub provider_instance_id: ProviderInstanceId,
    #[serde(rename = "providerSessionId")]
    pub provider_session_id: TrimmedNonEmptyString,
    #[serde(rename = "requestId")]
    pub request_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "tabId")]
    pub tab_id: Option<Option<PreviewTabId>>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PreviewAutomationRequestQueueClosedErrorTag {
    #[default]
    #[serde(rename = "PreviewAutomationRequestQueueClosedError")]
    PreviewAutomationRequestQueueClosedError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

pub type PreviewAutomationRespondSuccess = ();

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewAutomationResponse {
    #[serde(rename = "clientId")]
    pub client_id: PreviewAutomationClientId,
    #[serde(rename = "connectionId")]
    pub connection_id: PreviewAutomationConnectionId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub error: Option<Option<PreviewAutomationResponseError>>,
    pub ok: bool,
    #[serde(rename = "requestId")]
    pub request_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub result: Option<Option<serde_json::Value>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewAutomationResponseError {
    #[serde(rename = "_tag")]
    pub tag: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub detail: Option<Option<serde_json::Value>>,
    pub message: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewAutomationResultTooLargeError {
    #[serde(rename = "_tag")]
    pub tag: PreviewAutomationResultTooLargeErrorTag,
    pub cause: serde_json::Value,
    #[serde(rename = "clientId")]
    pub client_id: TrimmedNonEmptyString,
    #[serde(rename = "connectionId")]
    pub connection_id: PreviewAutomationConnectionId,
    #[serde(rename = "environmentId")]
    pub environment_id: EnvironmentId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "maximumBytes")]
    pub maximum_bytes: Option<Option<i64>>,
    pub operation: PreviewAutomationOperation,
    #[serde(rename = "providerInstanceId")]
    pub provider_instance_id: ProviderInstanceId,
    #[serde(rename = "providerSessionId")]
    pub provider_session_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "remoteDetailKind")]
    pub remote_detail_kind: Option<Option<PreviewAutomationResultTooLargeErrorRemoteDetailKind>>,
    #[serde(rename = "remoteMessageLength")]
    pub remote_message_length: i64,
    #[serde(rename = "remoteTag")]
    pub remote_tag: TrimmedNonEmptyString,
    #[serde(rename = "requestId")]
    pub request_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "tabId")]
    pub tab_id: Option<Option<PreviewTabId>>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PreviewAutomationResultTooLargeErrorTag {
    #[default]
    #[serde(rename = "PreviewAutomationResultTooLargeError")]
    PreviewAutomationResultTooLargeError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreviewAutomationResultTooLargeErrorRemoteDetailKind {
    #[serde(rename = "null")]
    Null,
    #[serde(rename = "array")]
    Array,
    #[serde(rename = "object")]
    Object,
    #[serde(rename = "string")]
    String,
    #[serde(rename = "number")]
    Number,
    #[serde(rename = "boolean")]
    Boolean,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PreviewAutomationStreamEvent {
    #[serde(rename = "connected")]
    Connected {
        #[serde(rename = "connectionId")]
        connection_id: PreviewAutomationConnectionId,
    },
    #[serde(rename = "request")]
    Request {
        #[serde(rename = "connectionId")]
        connection_id: PreviewAutomationConnectionId,
        request: PreviewAutomationRequest,
    },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewAutomationTabNotFoundError {
    #[serde(rename = "_tag")]
    pub tag: PreviewAutomationTabNotFoundErrorTag,
    pub cause: serde_json::Value,
    #[serde(rename = "clientId")]
    pub client_id: TrimmedNonEmptyString,
    #[serde(rename = "connectionId")]
    pub connection_id: PreviewAutomationConnectionId,
    #[serde(rename = "environmentId")]
    pub environment_id: EnvironmentId,
    pub operation: PreviewAutomationOperation,
    #[serde(rename = "providerInstanceId")]
    pub provider_instance_id: ProviderInstanceId,
    #[serde(rename = "providerSessionId")]
    pub provider_session_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "remoteDetailKind")]
    pub remote_detail_kind: Option<Option<PreviewAutomationTabNotFoundErrorRemoteDetailKind>>,
    #[serde(rename = "remoteMessageLength")]
    pub remote_message_length: i64,
    #[serde(rename = "remoteTag")]
    pub remote_tag: TrimmedNonEmptyString,
    #[serde(rename = "requestId")]
    pub request_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "tabId")]
    pub tab_id: Option<Option<PreviewTabId>>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PreviewAutomationTabNotFoundErrorTag {
    #[default]
    #[serde(rename = "PreviewAutomationTabNotFoundError")]
    PreviewAutomationTabNotFoundError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreviewAutomationTabNotFoundErrorRemoteDetailKind {
    #[serde(rename = "null")]
    Null,
    #[serde(rename = "array")]
    Array,
    #[serde(rename = "object")]
    Object,
    #[serde(rename = "string")]
    String,
    #[serde(rename = "number")]
    Number,
    #[serde(rename = "boolean")]
    Boolean,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewAutomationTargetNotEditableError {
    #[serde(rename = "_tag")]
    pub tag: PreviewAutomationTargetNotEditableErrorTag,
    pub cause: serde_json::Value,
    #[serde(rename = "clientId")]
    pub client_id: TrimmedNonEmptyString,
    #[serde(rename = "connectionId")]
    pub connection_id: PreviewAutomationConnectionId,
    #[serde(rename = "environmentId")]
    pub environment_id: EnvironmentId,
    pub operation: PreviewAutomationOperation,
    #[serde(rename = "providerInstanceId")]
    pub provider_instance_id: ProviderInstanceId,
    #[serde(rename = "providerSessionId")]
    pub provider_session_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "remoteDetailKind")]
    pub remote_detail_kind: Option<Option<PreviewAutomationTargetNotEditableErrorRemoteDetailKind>>,
    #[serde(rename = "remoteMessageLength")]
    pub remote_message_length: i64,
    #[serde(rename = "remoteTag")]
    pub remote_tag: TrimmedNonEmptyString,
    #[serde(rename = "requestId")]
    pub request_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "selectorKind")]
    pub selector_kind: Option<Option<PreviewAutomationTargetNotEditableErrorSelectorKind>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "selectorLength")]
    pub selector_length: Option<Option<i64>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "tabId")]
    pub tab_id: Option<Option<PreviewTabId>>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PreviewAutomationTargetNotEditableErrorTag {
    #[default]
    #[serde(rename = "PreviewAutomationTargetNotEditableError")]
    PreviewAutomationTargetNotEditableError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreviewAutomationTargetNotEditableErrorRemoteDetailKind {
    #[serde(rename = "null")]
    Null,
    #[serde(rename = "array")]
    Array,
    #[serde(rename = "object")]
    Object,
    #[serde(rename = "string")]
    String,
    #[serde(rename = "number")]
    Number,
    #[serde(rename = "boolean")]
    Boolean,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreviewAutomationTargetNotEditableErrorSelectorKind {
    #[serde(rename = "focused-element")]
    FocusedElement,
    #[serde(rename = "locator")]
    Locator,
    #[serde(rename = "selector")]
    Selector,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewAutomationTimeoutError {
    #[serde(rename = "_tag")]
    pub tag: PreviewAutomationTimeoutErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    #[serde(rename = "clientId")]
    pub client_id: TrimmedNonEmptyString,
    #[serde(rename = "connectionId")]
    pub connection_id: PreviewAutomationConnectionId,
    #[serde(rename = "environmentId")]
    pub environment_id: EnvironmentId,
    pub operation: PreviewAutomationOperation,
    #[serde(rename = "providerInstanceId")]
    pub provider_instance_id: ProviderInstanceId,
    #[serde(rename = "providerSessionId")]
    pub provider_session_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "remoteDetailKind")]
    pub remote_detail_kind: Option<Option<PreviewAutomationTimeoutErrorRemoteDetailKind>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "remoteMessageLength")]
    pub remote_message_length: Option<Option<i64>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "remoteTag")]
    pub remote_tag: Option<Option<TrimmedNonEmptyString>>,
    #[serde(rename = "requestId")]
    pub request_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "tabId")]
    pub tab_id: Option<Option<PreviewTabId>>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PreviewAutomationTimeoutErrorTag {
    #[default]
    #[serde(rename = "PreviewAutomationTimeoutError")]
    PreviewAutomationTimeoutError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreviewAutomationTimeoutErrorRemoteDetailKind {
    #[serde(rename = "null")]
    Null,
    #[serde(rename = "array")]
    Array,
    #[serde(rename = "object")]
    Object,
    #[serde(rename = "string")]
    String,
    #[serde(rename = "number")]
    Number,
    #[serde(rename = "boolean")]
    Boolean,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewAutomationUnavailableError {
    #[serde(rename = "_tag")]
    pub tag: PreviewAutomationUnavailableErrorTag,
    pub capability: PreviewAutomationUnavailableErrorCapability,
    #[serde(rename = "environmentId")]
    pub environment_id: EnvironmentId,
    #[serde(rename = "providerInstanceId")]
    pub provider_instance_id: ProviderInstanceId,
    #[serde(rename = "providerSessionId")]
    pub provider_session_id: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PreviewAutomationUnavailableErrorTag {
    #[default]
    #[serde(rename = "PreviewAutomationUnavailableError")]
    PreviewAutomationUnavailableError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PreviewAutomationUnavailableErrorCapability {
    #[default]
    #[serde(rename = "preview")]
    Preview,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewAutomationUnsupportedClientError {
    #[serde(rename = "_tag")]
    pub tag: PreviewAutomationUnsupportedClientErrorTag,
    pub cause: serde_json::Value,
    #[serde(rename = "clientId")]
    pub client_id: TrimmedNonEmptyString,
    #[serde(rename = "connectionId")]
    pub connection_id: PreviewAutomationConnectionId,
    #[serde(rename = "environmentId")]
    pub environment_id: EnvironmentId,
    pub operation: PreviewAutomationOperation,
    #[serde(rename = "providerInstanceId")]
    pub provider_instance_id: ProviderInstanceId,
    #[serde(rename = "providerSessionId")]
    pub provider_session_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "remoteDetailKind")]
    pub remote_detail_kind: Option<Option<PreviewAutomationUnsupportedClientErrorRemoteDetailKind>>,
    #[serde(rename = "remoteMessageLength")]
    pub remote_message_length: i64,
    #[serde(rename = "remoteTag")]
    pub remote_tag: TrimmedNonEmptyString,
    #[serde(rename = "requestId")]
    pub request_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "tabId")]
    pub tab_id: Option<Option<PreviewTabId>>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PreviewAutomationUnsupportedClientErrorTag {
    #[default]
    #[serde(rename = "PreviewAutomationUnsupportedClientError")]
    PreviewAutomationUnsupportedClientError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreviewAutomationUnsupportedClientErrorRemoteDetailKind {
    #[serde(rename = "null")]
    Null,
    #[serde(rename = "array")]
    Array,
    #[serde(rename = "object")]
    Object,
    #[serde(rename = "string")]
    String,
    #[serde(rename = "number")]
    Number,
    #[serde(rename = "boolean")]
    Boolean,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewCloseInput {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "tabId")]
    pub tab_id: Option<Option<PreviewTabId>>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
}

pub type PreviewCloseSuccess = ();

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PreviewError {
    PreviewSessionLookupError(PreviewSessionLookupError),
    PreviewInvalidUrlError(PreviewInvalidUrlError),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PreviewEvent {
    #[serde(rename = "opened")]
    Opened {
        #[serde(rename = "createdAt")]
        created_at: TrimmedNonEmptyString,
        revision: PositiveInt,
        #[serde(rename = "serverEpoch")]
        server_epoch: TrimmedNonEmptyString,
        snapshot: PreviewSessionSnapshot,
        #[serde(rename = "tabId")]
        tab_id: PreviewTabId,
        #[serde(rename = "threadId")]
        thread_id: TrimmedNonEmptyString,
    },
    #[serde(rename = "navigated")]
    Navigated {
        #[serde(rename = "createdAt")]
        created_at: TrimmedNonEmptyString,
        revision: PositiveInt,
        #[serde(rename = "serverEpoch")]
        server_epoch: TrimmedNonEmptyString,
        snapshot: PreviewSessionSnapshot,
        #[serde(rename = "tabId")]
        tab_id: PreviewTabId,
        #[serde(rename = "threadId")]
        thread_id: TrimmedNonEmptyString,
    },
    #[serde(rename = "resized")]
    Resized {
        #[serde(rename = "createdAt")]
        created_at: TrimmedNonEmptyString,
        revision: PositiveInt,
        #[serde(rename = "serverEpoch")]
        server_epoch: TrimmedNonEmptyString,
        snapshot: PreviewSessionSnapshot,
        #[serde(rename = "tabId")]
        tab_id: PreviewTabId,
        #[serde(rename = "threadId")]
        thread_id: TrimmedNonEmptyString,
    },
    #[serde(rename = "failed")]
    Failed {
        code: i64,
        #[serde(rename = "createdAt")]
        created_at: TrimmedNonEmptyString,
        description: TrimmedNonEmptyString,
        revision: PositiveInt,
        #[serde(rename = "serverEpoch")]
        server_epoch: TrimmedNonEmptyString,
        #[serde(rename = "tabId")]
        tab_id: PreviewTabId,
        #[serde(rename = "threadId")]
        thread_id: TrimmedNonEmptyString,
        title: String,
        url: TrimmedNonEmptyString,
    },
    #[serde(rename = "closed")]
    Closed {
        #[serde(rename = "createdAt")]
        created_at: TrimmedNonEmptyString,
        revision: PositiveInt,
        #[serde(rename = "serverEpoch")]
        server_epoch: TrimmedNonEmptyString,
        #[serde(rename = "tabId")]
        tab_id: PreviewTabId,
        #[serde(rename = "threadId")]
        thread_id: TrimmedNonEmptyString,
    },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewInvalidUrlError {
    #[serde(rename = "_tag")]
    pub tag: PreviewInvalidUrlErrorTag,
    pub cause: serde_json::Value,
    #[serde(rename = "inputLength")]
    pub input_length: DurationMillis,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub protocol: Option<Option<TrimmedNonEmptyString>>,
    pub reason: PreviewInvalidUrlErrorReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PreviewInvalidUrlErrorTag {
    #[default]
    #[serde(rename = "PreviewInvalidUrlError")]
    PreviewInvalidUrlError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreviewInvalidUrlErrorReason {
    #[serde(rename = "empty")]
    Empty,
    #[serde(rename = "parse")]
    Parse,
    #[serde(rename = "unsupported-protocol")]
    UnsupportedProtocol,
    #[serde(rename = "unexpected")]
    Unexpected,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewListInput {
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewListResult {
    pub revision: NonNegativeInt,
    #[serde(rename = "serverEpoch")]
    pub server_epoch: TrimmedNonEmptyString,
    pub sessions: Vec<PreviewSessionSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "_tag")]
pub enum PreviewNavStatus {
    #[serde(rename = "Idle")]
    Idle {},
    #[serde(rename = "Loading")]
    Loading {
        title: String,
        url: TrimmedNonEmptyString,
    },
    #[serde(rename = "Success")]
    Success {
        title: String,
        url: TrimmedNonEmptyString,
    },
    #[serde(rename = "LoadFailed")]
    LoadFailed {
        code: i64,
        description: TrimmedNonEmptyString,
        title: String,
        url: TrimmedNonEmptyString,
    },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewNavigateInput {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "resolvedTitle")]
    pub resolved_title: Option<Option<String>>,
    #[serde(rename = "tabId")]
    pub tab_id: PreviewTabId,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    pub url: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewOpenInput {
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub url: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewRefreshInput {
    #[serde(rename = "tabId")]
    pub tab_id: PreviewTabId,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
}

pub type PreviewRefreshSuccess = ();

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewReportStatusInput {
    #[serde(rename = "canGoBack")]
    pub can_go_back: bool,
    #[serde(rename = "canGoForward")]
    pub can_go_forward: bool,
    #[serde(rename = "navStatus")]
    pub nav_status: PreviewNavStatus,
    #[serde(rename = "tabId")]
    pub tab_id: PreviewTabId,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
}

pub type PreviewReportStatusSuccess = ();

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewResizeInput {
    #[serde(rename = "tabId")]
    pub tab_id: PreviewTabId,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    pub viewport: PreviewViewportSetting,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewSessionLookupError {
    #[serde(rename = "_tag")]
    pub tag: PreviewSessionLookupErrorTag,
    #[serde(rename = "tabId")]
    pub tab_id: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PreviewSessionLookupErrorTag {
    #[default]
    #[serde(rename = "PreviewSessionLookupError")]
    PreviewSessionLookupError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewSessionSnapshot {
    #[serde(rename = "canGoBack")]
    pub can_go_back: bool,
    #[serde(rename = "canGoForward")]
    pub can_go_forward: bool,
    #[serde(rename = "navStatus")]
    pub nav_status: PreviewNavStatus,
    #[serde(rename = "tabId")]
    pub tab_id: PreviewTabId,
    #[serde(rename = "threadId")]
    pub thread_id: TrimmedNonEmptyString,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub viewport: Option<Option<PreviewViewportSetting>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PreviewTabId(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PreviewViewportSetting {
    Fill {
        #[serde(rename = "_tag")]
        tag: PreviewViewportSettingFillTag,
    },
    PreviewViewportSize(PreviewViewportSize),
    PreviewViewportSize1(PreviewViewportSize1),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PreviewViewportSettingFillTag {
    #[default]
    #[serde(rename = "fill")]
    Fill,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewViewportSize {
    #[serde(rename = "_tag")]
    pub tag: PreviewViewportSizeTag,
    pub height: i64,
    pub width: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PreviewViewportSizeTag {
    #[default]
    #[serde(rename = "freeform")]
    Freeform,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewViewportSize1 {
    #[serde(rename = "_tag")]
    pub tag: PreviewViewportSize1Tag,
    pub height: i64,
    #[serde(rename = "presetId")]
    pub preset_id: PreviewViewportSize1PresetId,
    pub width: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PreviewViewportSize1Tag {
    #[default]
    #[serde(rename = "preset")]
    Preset,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreviewViewportSize1PresetId {
    #[serde(rename = "iphone-se")]
    IphoneSe,
    #[serde(rename = "iphone-xr")]
    IphoneXr,
    #[serde(rename = "iphone-12-pro")]
    Iphone12Pro,
    #[serde(rename = "iphone-14-pro-max")]
    Iphone14ProMax,
    #[serde(rename = "pixel-7")]
    Pixel7,
    #[serde(rename = "samsung-galaxy-s8-plus")]
    SamsungGalaxyS8Plus,
    #[serde(rename = "samsung-galaxy-s20-ultra")]
    SamsungGalaxyS20Ultra,
    #[serde(rename = "ipad-mini")]
    IpadMini,
    #[serde(rename = "ipad-air")]
    IpadAir,
    #[serde(rename = "ipad-pro")]
    IpadPro,
    #[serde(rename = "surface-pro-7")]
    SurfacePro7,
    #[serde(rename = "surface-duo")]
    SurfaceDuo,
    #[serde(rename = "galaxy-z-fold-5")]
    GalaxyZFold5,
    #[serde(rename = "asus-zenbook-fold")]
    AsusZenbookFold,
    #[serde(rename = "samsung-galaxy-a51-71")]
    SamsungGalaxyA5171,
    #[serde(rename = "nest-hub")]
    NestHub,
    #[serde(rename = "nest-hub-max")]
    NestHubMax,
    #[serde(rename = "desktop-1920x1080")]
    Desktop1920x1080,
    #[serde(rename = "desktop-1440x900")]
    Desktop1440x900,
    #[serde(rename = "laptop-1366x768")]
    Laptop1366x768,
    #[serde(rename = "laptop-1280x800")]
    Laptop1280x800,
    #[serde(rename = "ipad-pro-11")]
    IpadPro11,
    #[serde(rename = "iphone-15-pro")]
    Iphone15Pro,
    #[serde(rename = "pixel-8")]
    Pixel8,
    #[serde(rename = "galaxy-s24")]
    GalaxyS24,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectCopyEntryError {
    #[serde(rename = "_tag")]
    pub tag: ProjectCopyEntryErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub failure: Option<Option<ProjectFileFailure>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "fromCwd")]
    pub from_cwd: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "fromRelativePath")]
    pub from_relative_path: Option<Option<TrimmedNonEmptyString>>,
    pub message: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub operation: Option<Option<ProjectFileOperation>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "operationPath")]
    pub operation_path: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "resolvedPath")]
    pub resolved_path: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "resolvedWorkspaceRoot")]
    pub resolved_workspace_root: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "toCwd")]
    pub to_cwd: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "toRelativePath")]
    pub to_relative_path: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ProjectCopyEntryErrorTag {
    #[default]
    #[serde(rename = "ProjectCopyEntryError")]
    ProjectCopyEntryError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectCopyEntryInput {
    #[serde(rename = "fromCwd")]
    pub from_cwd: TrimmedNonEmptyString,
    #[serde(rename = "fromRelativePath")]
    pub from_relative_path: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub overwrite: Option<Option<bool>>,
    #[serde(rename = "toCwd")]
    pub to_cwd: TrimmedNonEmptyString,
    #[serde(rename = "toRelativePath")]
    pub to_relative_path: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectCopyEntryResult {
    #[serde(rename = "relativePath")]
    pub relative_path: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectCreateCommand {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "additionalRoots")]
    pub additional_roots: Option<Option<Vec<WorkspaceRootRef>>>,
    #[serde(rename = "commandId")]
    pub command_id: CommandId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "createWorkspaceRootIfMissing")]
    pub create_workspace_root_if_missing: Option<Option<bool>>,
    #[serde(rename = "createdAt")]
    pub created_at: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "defaultModelSelection")]
    pub default_model_selection: Option<Option<Option<ModelSelection>>>,
    #[serde(rename = "projectId")]
    pub project_id: ProjectId,
    pub title: TrimmedNonEmptyString,
    pub r#type: ProjectCreateCommandType,
    #[serde(rename = "workspaceRoot")]
    pub workspace_root: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ProjectCreateCommandType {
    #[default]
    #[serde(rename = "project.create")]
    ProjectCreate,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectCreatedPayload {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "additionalRoots")]
    pub additional_roots: Option<Option<Vec<ProjectCreatedPayloadAdditionalRoots>>>,
    #[serde(rename = "createdAt")]
    pub created_at: TrimmedNonEmptyString,
    #[serde(rename = "defaultModelSelection")]
    pub default_model_selection: Option<ModelSelection>,
    #[serde(rename = "projectId")]
    pub project_id: ProjectId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "repositoryIdentity")]
    pub repository_identity: Option<Option<Option<RepositoryIdentity>>>,
    pub scripts: Vec<ProjectScript>,
    pub title: TrimmedNonEmptyString,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
    #[serde(rename = "workspaceRoot")]
    pub workspace_root: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ProjectCreatedPayloadAdditionalRoots {
    #[serde(rename = "project")]
    Project {
        #[serde(rename = "projectId")]
        project_id: String,
    },
    #[serde(rename = "path")]
    Path { path: String },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectDeletedPayload {
    #[serde(rename = "deletedAt")]
    pub deleted_at: TrimmedNonEmptyString,
    #[serde(rename = "projectId")]
    pub project_id: ProjectId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProjectEntriesFailure {
    #[serde(rename = "workspace_root_not_found")]
    WorkspaceRootNotFound,
    #[serde(rename = "workspace_root_create_failed")]
    WorkspaceRootCreateFailed,
    #[serde(rename = "workspace_root_stat_failed")]
    WorkspaceRootStatFailed,
    #[serde(rename = "workspace_root_not_directory")]
    WorkspaceRootNotDirectory,
    #[serde(rename = "search_index_create_failed")]
    SearchIndexCreateFailed,
    #[serde(rename = "search_index_scan_timed_out")]
    SearchIndexScanTimedOut,
    #[serde(rename = "search_index_search_failed")]
    SearchIndexSearchFailed,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectEntry {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub ignored: Option<Option<bool>>,
    pub kind: ProjectEntryKind,
    pub path: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProjectEntryKind {
    #[serde(rename = "file")]
    File,
    #[serde(rename = "directory")]
    Directory,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProjectFileFailure {
    #[serde(rename = "workspace_path_outside_root")]
    WorkspacePathOutsideRoot,
    #[serde(rename = "resolved_path_outside_root")]
    ResolvedPathOutsideRoot,
    #[serde(rename = "path_not_file")]
    PathNotFile,
    #[serde(rename = "binary_file")]
    BinaryFile,
    #[serde(rename = "operation_failed")]
    OperationFailed,
    #[serde(rename = "stale_revision")]
    StaleRevision,
    #[serde(rename = "path_exists")]
    PathExists,
    #[serde(rename = "path_not_found")]
    PathNotFound,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProjectFileOperation {
    #[serde(rename = "realpath-workspace-root")]
    RealpathWorkspaceRoot,
    #[serde(rename = "realpath-target")]
    RealpathTarget,
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "stat")]
    Stat,
    #[serde(rename = "read")]
    Read,
    #[serde(rename = "close")]
    Close,
    #[serde(rename = "make-directory")]
    MakeDirectory,
    #[serde(rename = "write-file")]
    WriteFile,
    #[serde(rename = "exists")]
    Exists,
    #[serde(rename = "rename")]
    Rename,
    #[serde(rename = "remove")]
    Remove,
    #[serde(rename = "copy")]
    Copy,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectId(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectListEntriesError {
    #[serde(rename = "_tag")]
    pub tag: ProjectListEntriesErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cwd: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub detail: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub failure: Option<Option<ProjectEntriesFailure>>,
    pub message: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "normalizedCwd")]
    pub normalized_cwd: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub timeout: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ProjectListEntriesErrorTag {
    #[default]
    #[serde(rename = "ProjectListEntriesError")]
    ProjectListEntriesError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectListEntriesInput {
    pub cwd: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectListEntriesResult {
    pub entries: Vec<ProjectEntry>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectMetaUpdatedPayload {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "additionalRoots")]
    pub additional_roots: Option<Option<Vec<WorkspaceRootRef>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "defaultModelSelection")]
    pub default_model_selection: Option<Option<Option<ModelSelection>>>,
    #[serde(rename = "projectId")]
    pub project_id: ProjectId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "repositoryIdentity")]
    pub repository_identity: Option<Option<Option<RepositoryIdentity>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub scripts: Option<Option<Vec<ProjectScript>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub title: Option<Option<TrimmedNonEmptyString>>,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "workspaceRoot")]
    pub workspace_root: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectMutateEntryError {
    #[serde(rename = "_tag")]
    pub tag: ProjectMutateEntryErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cwd: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub failure: Option<Option<ProjectFileFailure>>,
    pub message: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub operation: Option<Option<ProjectFileOperation>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "operationPath")]
    pub operation_path: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "relativePath")]
    pub relative_path: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "resolvedPath")]
    pub resolved_path: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "resolvedWorkspaceRoot")]
    pub resolved_workspace_root: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "toRelativePath")]
    pub to_relative_path: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ProjectMutateEntryErrorTag {
    #[default]
    #[serde(rename = "ProjectMutateEntryError")]
    ProjectMutateEntryError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "_tag")]
pub enum ProjectMutateEntryInput {
    #[serde(rename = "create")]
    Create {
        cwd: TrimmedNonEmptyString,
        kind: ProjectMutateEntryInputCreateKind,
        #[serde(rename = "relativePath")]
        relative_path: TrimmedNonEmptyString,
    },
    #[serde(rename = "rename")]
    Rename {
        cwd: TrimmedNonEmptyString,
        #[serde(rename = "fromRelativePath")]
        from_relative_path: TrimmedNonEmptyString,
        #[serde(rename = "toRelativePath")]
        to_relative_path: TrimmedNonEmptyString,
    },
    #[serde(rename = "delete")]
    Delete {
        cwd: TrimmedNonEmptyString,
        #[serde(rename = "relativePath")]
        relative_path: TrimmedNonEmptyString,
    },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProjectMutateEntryInputCreateKind {
    #[serde(rename = "file")]
    File,
    #[serde(rename = "directory")]
    Directory,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectMutateEntryResult {
    #[serde(rename = "relativePath")]
    pub relative_path: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectReadFileError {
    #[serde(rename = "_tag")]
    pub tag: ProjectReadFileErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cwd: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub failure: Option<Option<ProjectFileFailure>>,
    pub message: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub operation: Option<Option<ProjectFileOperation>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "operationPath")]
    pub operation_path: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "relativePath")]
    pub relative_path: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "resolvedPath")]
    pub resolved_path: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "resolvedWorkspaceRoot")]
    pub resolved_workspace_root: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ProjectReadFileErrorTag {
    #[default]
    #[serde(rename = "ProjectReadFileError")]
    ProjectReadFileError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectReadFileInput {
    pub cwd: TrimmedNonEmptyString,
    #[serde(rename = "relativePath")]
    pub relative_path: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectReadFileResult {
    #[serde(rename = "byteLength")]
    pub byte_length: NonNegativeInt,
    pub contents: TrimmedNonEmptyString,
    #[serde(rename = "relativePath")]
    pub relative_path: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub revision: Option<Option<TrimmedNonEmptyString>>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectScript {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "autoOpenPreview")]
    pub auto_open_preview: Option<Option<bool>>,
    pub command: TrimmedNonEmptyString,
    pub icon: ProjectScriptIcon,
    pub id: TrimmedNonEmptyString,
    pub name: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "previewUrl")]
    pub preview_url: Option<Option<TrimmedNonEmptyString>>,
    #[serde(rename = "runOnWorktreeCreate")]
    pub run_on_worktree_create: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProjectScriptIcon {
    #[serde(rename = "play")]
    Play,
    #[serde(rename = "test")]
    Test,
    #[serde(rename = "lint")]
    Lint,
    #[serde(rename = "configure")]
    Configure,
    #[serde(rename = "build")]
    Build,
    #[serde(rename = "debug")]
    Debug,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectSearchContentError {
    #[serde(rename = "_tag")]
    pub tag: ProjectSearchContentErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cwd: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub detail: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub failure: Option<Option<ProjectSearchContentFailure>>,
    pub message: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ProjectSearchContentErrorTag {
    #[default]
    #[serde(rename = "ProjectSearchContentError")]
    ProjectSearchContentError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProjectSearchContentFailure {
    #[serde(rename = "workspace_root_not_found")]
    WorkspaceRootNotFound,
    #[serde(rename = "invalid_pattern")]
    InvalidPattern,
    #[serde(rename = "search_spawn_failed")]
    SearchSpawnFailed,
    #[serde(rename = "search_failed")]
    SearchFailed,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectSearchContentInput {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "caseSensitive")]
    pub case_sensitive: Option<Option<bool>>,
    pub cwd: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "excludeGlob")]
    pub exclude_glob: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "includeGlob")]
    pub include_glob: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "maxResults")]
    pub max_results: Option<Option<i64>>,
    pub query: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub regex: Option<Option<bool>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "wholeWord")]
    pub whole_word: Option<Option<bool>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectSearchContentMatch {
    pub line: PositiveInt,
    #[serde(rename = "lineText")]
    pub line_text: TrimmedNonEmptyString,
    #[serde(rename = "lineTruncated")]
    pub line_truncated: bool,
    #[serde(rename = "matchEnd")]
    pub match_end: NonNegativeInt,
    #[serde(rename = "matchStart")]
    pub match_start: NonNegativeInt,
    pub path: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectSearchContentResult {
    #[serde(rename = "fileCount")]
    pub file_count: NonNegativeInt,
    pub matches: Vec<ProjectSearchContentMatch>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectSearchEntriesError {
    #[serde(rename = "_tag")]
    pub tag: ProjectSearchEntriesErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cwd: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub detail: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub failure: Option<Option<ProjectEntriesFailure>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub limit: Option<Option<PositiveInt>>,
    pub message: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "normalizedCwd")]
    pub normalized_cwd: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "queryLength")]
    pub query_length: Option<Option<NonNegativeInt>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub timeout: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ProjectSearchEntriesErrorTag {
    #[default]
    #[serde(rename = "ProjectSearchEntriesError")]
    ProjectSearchEntriesError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectSearchEntriesInput {
    pub cwd: TrimmedNonEmptyString,
    pub limit: i64,
    pub query: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectSearchEntriesResult {
    pub entries: Vec<ProjectEntry>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectWatchError {
    #[serde(rename = "_tag")]
    pub tag: ProjectWatchErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cwd: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub failure: Option<Option<ProjectWatchFailure>>,
    pub message: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ProjectWatchErrorTag {
    #[default]
    #[serde(rename = "ProjectWatchError")]
    ProjectWatchError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProjectWatchFailure {
    #[serde(rename = "workspace_root_not_found")]
    WorkspaceRootNotFound,
    #[serde(rename = "watch_failed")]
    WatchFailed,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectWatchInput {
    pub cwd: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "_tag")]
pub enum ProjectWatchStreamEvent {
    #[serde(rename = "changes")]
    Changes { paths: Vec<TrimmedNonEmptyString> },
    #[serde(rename = "overflow")]
    Overflow {},
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectWriteFileError {
    #[serde(rename = "_tag")]
    pub tag: ProjectWriteFileErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "actualRevision")]
    pub actual_revision: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cwd: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "expectedRevision")]
    pub expected_revision: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub failure: Option<Option<ProjectFileFailure>>,
    pub message: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub operation: Option<Option<ProjectFileOperation>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "operationPath")]
    pub operation_path: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "relativePath")]
    pub relative_path: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "resolvedPath")]
    pub resolved_path: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "resolvedWorkspaceRoot")]
    pub resolved_workspace_root: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ProjectWriteFileErrorTag {
    #[default]
    #[serde(rename = "ProjectWriteFileError")]
    ProjectWriteFileError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectWriteFileInput {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "baseRevision")]
    pub base_revision: Option<Option<TrimmedNonEmptyString>>,
    pub contents: TrimmedNonEmptyString,
    pub cwd: TrimmedNonEmptyString,
    #[serde(rename = "relativePath")]
    pub relative_path: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectWriteFileResult {
    #[serde(rename = "relativePath")]
    pub relative_path: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub revision: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderApprovalDecision {
    #[serde(rename = "accept")]
    Accept,
    #[serde(rename = "acceptForSession")]
    AcceptForSession,
    #[serde(rename = "decline")]
    Decline,
    #[serde(rename = "cancel")]
    Cancel,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderDriverKind(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderInstanceConfig {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "accentColor")]
    pub accent_color: Option<Option<TrimmedNonEmptyString>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "displayName")]
    pub display_name: Option<Option<TrimmedNonEmptyString>>,
    pub driver: ProviderDriverKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<Vec<ProviderInstanceEnvironmentVariable>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderInstanceEnvironmentVariable {
    pub name: ProviderInstanceEnvironmentVariableName,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub sensitive: Option<Option<bool>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub value: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "valueRedacted")]
    pub value_redacted: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderInstanceEnvironmentVariableName(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderInstanceId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderInteractionMode {
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "plan")]
    Plan,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderItemId(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderOptionChoice {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub description: Option<Option<TrimmedNonEmptyString>>,
    pub id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "isDefault")]
    pub is_default: Option<Option<bool>>,
    pub label: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProviderOptionDescriptor {
    SelectProviderOptionDescriptor(SelectProviderOptionDescriptor),
    BooleanProviderOptionDescriptor(BooleanProviderOptionDescriptor),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderOptionSelection {
    pub id: TrimmedNonEmptyString,
    pub value: ProviderOptionSelectionValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProviderOptionSelectionValue {
    TrimmedNonEmptyString(TrimmedNonEmptyString),
    V1(bool),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

pub type ProviderUserInputAnswers = serde_json::Map<String, serde_json::Value>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayClientInstallFailedError {
    #[serde(rename = "_tag")]
    pub tag: RelayClientInstallFailedErrorTag,
    pub message: TrimmedNonEmptyString,
    pub reason: RelayClientInstallFailureReasonSchema,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum RelayClientInstallFailedErrorTag {
    #[default]
    #[serde(rename = "RelayClientInstallFailedError")]
    RelayClientInstallFailedError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelayClientInstallFailureReasonSchema {
    #[serde(rename = "download_failed")]
    DownloadFailed,
    #[serde(rename = "invalid_checksum")]
    InvalidChecksum,
    #[serde(rename = "install_locked")]
    InstallLocked,
    #[serde(rename = "override_missing")]
    OverrideMissing,
    #[serde(rename = "unsupported_platform")]
    UnsupportedPlatform,
    #[serde(rename = "validation_failed")]
    ValidationFailed,
    #[serde(rename = "write_failed")]
    WriteFailed,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RelayClientInstallProgressEventSchema {
    #[serde(rename = "progress")]
    Progress {
        stage: RelayClientInstallProgressStageSchema,
    },
    #[serde(rename = "complete")]
    Complete { status: RelayClientStatusSchema },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelayClientInstallProgressStageSchema {
    #[serde(rename = "checking")]
    Checking,
    #[serde(rename = "waiting_for_lock")]
    WaitingForLock,
    #[serde(rename = "downloading")]
    Downloading,
    #[serde(rename = "verifying")]
    Verifying,
    #[serde(rename = "installing")]
    Installing,
    #[serde(rename = "validating")]
    Validating,
    #[serde(rename = "activating")]
    Activating,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum RelayClientStatusSchema {
    #[serde(rename = "available")]
    Available {
        #[serde(rename = "executablePath")]
        executable_path: TrimmedNonEmptyString,
        source: RelayClientStatusSchemaAvailableSource,
        version: TrimmedNonEmptyString,
    },
    #[serde(rename = "missing")]
    Missing { version: TrimmedNonEmptyString },
    #[serde(rename = "unsupported")]
    Unsupported {
        arch: TrimmedNonEmptyString,
        platform: TrimmedNonEmptyString,
        version: TrimmedNonEmptyString,
    },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelayClientStatusSchemaAvailableSource {
    #[serde(rename = "override")]
    Override,
    #[serde(rename = "managed")]
    Managed,
    #[serde(rename = "path")]
    Path,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepositoryIdentity {
    #[serde(rename = "canonicalKey")]
    pub canonical_key: TrimmedNonEmptyString,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "displayName")]
    pub display_name: Option<TrimmedNonEmptyString>,
    pub locator: RepositoryIdentityLocator,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<TrimmedNonEmptyString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<TrimmedNonEmptyString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<TrimmedNonEmptyString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "rootPath")]
    pub root_path: Option<TrimmedNonEmptyString>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepositoryIdentityLocator {
    #[serde(rename = "remoteName")]
    pub remote_name: TrimmedNonEmptyString,
    #[serde(rename = "remoteUrl")]
    pub remote_url: TrimmedNonEmptyString,
    pub source: RepositoryIdentityLocatorSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum RepositoryIdentityLocatorSource {
    #[default]
    #[serde(rename = "git-remote")]
    GitRemote,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedKeybindingRule {
    pub command: KeybindingCommand,
    pub shortcut: KeybindingShortcut,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "whenAst")]
    pub when_ast: Option<Option<KeybindingWhenNode>>,
}

pub type ResolvedKeybindingsConfig = Vec<ResolvedKeybindingRule>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedWorkspaceRoot {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub path: Option<Option<TrimmedNonEmptyString>>,
    pub r#ref: WorkspaceRootRef,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "repositoryIdentity")]
    pub repository_identity: Option<Option<Option<RepositoryIdentity>>>,
    pub status: ResolvedWorkspaceRootStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResolvedWorkspaceRootStatus {
    #[serde(rename = "ok")]
    Ok,
    #[serde(rename = "missing-project")]
    MissingProject,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ReviewDiffPreviewError {
    VcsError(VcsError),
    GitCommandError(GitCommandError),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewDiffPreviewInput {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "baseRef")]
    pub base_ref: Option<Option<TrimmedNonEmptyString>>,
    pub cwd: TrimmedNonEmptyString,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "ignoreWhitespace")]
    pub ignore_whitespace: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewDiffPreviewResult {
    pub cwd: TrimmedNonEmptyString,
    #[serde(rename = "generatedAt")]
    pub generated_at: TrimmedNonEmptyString,
    pub sources: Vec<ReviewDiffPreviewSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewDiffPreviewSource {
    #[serde(rename = "baseRef")]
    pub base_ref: Option<TrimmedNonEmptyString>,
    pub diff: TrimmedNonEmptyString,
    #[serde(rename = "diffHash")]
    pub diff_hash: TrimmedNonEmptyString,
    #[serde(rename = "headRef")]
    pub head_ref: Option<TrimmedNonEmptyString>,
    pub id: TrimmedNonEmptyString,
    pub kind: ReviewDiffPreviewSourceKind,
    pub title: TrimmedNonEmptyString,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReviewDiffPreviewSourceKind {
    #[serde(rename = "working-tree")]
    WorkingTree,
    #[serde(rename = "branch-range")]
    BranchRange,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuntimeMode {
    #[serde(rename = "approval-required")]
    ApprovalRequired,
    #[serde(rename = "auto-accept-edits")]
    AutoAcceptEdits,
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "full-access")]
    FullAccess,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SCRIPTRUNCOMMANDPATTERN(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectProviderOptionDescriptor {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "currentValue")]
    pub current_value: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub description: Option<Option<TrimmedNonEmptyString>>,
    pub id: TrimmedNonEmptyString,
    pub label: TrimmedNonEmptyString,
    pub options: Vec<ProviderOptionChoice>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "promptInjectedValues")]
    pub prompt_injected_values: Option<Option<Vec<TrimmedNonEmptyString>>>,
    pub r#type: SelectProviderOptionDescriptorType,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SelectProviderOptionDescriptorType {
    #[default]
    #[serde(rename = "select")]
    Select,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServerAuthBootstrapMethod {
    #[serde(rename = "desktop-bootstrap")]
    DesktopBootstrap,
    #[serde(rename = "one-time-token")]
    OneTimeToken,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerAuthDescriptor {
    #[serde(rename = "bootstrapMethods")]
    pub bootstrap_methods: Vec<ServerAuthBootstrapMethod>,
    pub policy: ServerAuthPolicy,
    #[serde(rename = "sessionCookieName")]
    pub session_cookie_name: TrimmedNonEmptyString,
    #[serde(rename = "sessionMethods")]
    pub session_methods: Vec<ServerAuthSessionMethod>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServerAuthPolicy {
    #[serde(rename = "desktop-managed-local")]
    DesktopManagedLocal,
    #[serde(rename = "loopback-browser")]
    LoopbackBrowser,
    #[serde(rename = "remote-reachable")]
    RemoteReachable,
    #[serde(rename = "unsafe-no-auth")]
    UnsafeNoAuth,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServerAuthSessionMethod {
    #[serde(rename = "browser-session-cookie")]
    BrowserSessionCookie,
    #[serde(rename = "bearer-access-token")]
    BearerAccessToken,
    #[serde(rename = "dpop-access-token")]
    DpopAccessToken,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerConfig {
    pub auth: ServerAuthDescriptor,
    #[serde(rename = "availableEditors")]
    pub available_editors: Vec<EditorId>,
    pub cwd: TrimmedNonEmptyString,
    pub environment: ExecutionEnvironmentDescriptor,
    pub issues: Vec<ServerConfigIssue>,
    pub keybindings: ResolvedKeybindingsConfig,
    #[serde(rename = "keybindingsConfigPath")]
    pub keybindings_config_path: TrimmedNonEmptyString,
    pub observability: ServerObservability,
    pub providers: ServerProviders,
    pub settings: ServerSettings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "shellResumeCompletionMarker")]
    pub shell_resume_completion_marker: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "threadResumeCompletionMarker")]
    pub thread_resume_completion_marker: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ServerConfigIssue {
    #[serde(rename = "keybindings.malformed-config")]
    KeybindingsMalformedConfig { message: TrimmedNonEmptyString },
    #[serde(rename = "keybindings.invalid-entry")]
    KeybindingsInvalidEntry {
        index: DurationMillis,
        message: TrimmedNonEmptyString,
    },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerConfigKeybindingsUpdatedPayload {
    pub issues: Vec<ServerConfigIssue>,
    pub keybindings: ResolvedKeybindingsConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerConfigProviderStatusesPayload {
    pub providers: ServerProviders,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerConfigSettingsUpdatedPayload {
    pub settings: ServerSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ServerConfigStreamEvent {
    ServerConfigStreamSnapshotEvent(ServerConfigStreamSnapshotEvent),
    ServerConfigStreamKeybindingsUpdatedEvent(ServerConfigStreamKeybindingsUpdatedEvent),
    ServerConfigStreamProviderStatusesEvent(ServerConfigStreamProviderStatusesEvent),
    ServerConfigStreamSettingsUpdatedEvent(ServerConfigStreamSettingsUpdatedEvent),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerConfigStreamKeybindingsUpdatedEvent {
    pub payload: ServerConfigKeybindingsUpdatedPayload,
    pub r#type: ServerConfigStreamKeybindingsUpdatedEventType,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ServerConfigStreamKeybindingsUpdatedEventType {
    #[default]
    #[serde(rename = "keybindingsUpdated")]
    KeybindingsUpdated,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerConfigStreamProviderStatusesEvent {
    pub payload: ServerConfigProviderStatusesPayload,
    pub r#type: ServerConfigStreamProviderStatusesEventType,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ServerConfigStreamProviderStatusesEventType {
    #[default]
    #[serde(rename = "providerStatuses")]
    ProviderStatuses,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerConfigStreamSettingsUpdatedEvent {
    pub payload: ServerConfigSettingsUpdatedPayload,
    pub r#type: ServerConfigStreamSettingsUpdatedEventType,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ServerConfigStreamSettingsUpdatedEventType {
    #[default]
    #[serde(rename = "settingsUpdated")]
    SettingsUpdated,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerConfigStreamSnapshotEvent {
    pub config: ServerConfig,
    pub r#type: ServerConfigStreamSnapshotEventType,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ServerConfigStreamSnapshotEventType {
    #[default]
    #[serde(rename = "snapshot")]
    Snapshot,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

/// Void payload — send `{}` on the wire.
pub type ServerDiscoverSourceControlPayload = serde_json::Value;

/// Void payload — send `{}` on the wire.
pub type ServerGetConfigPayload = serde_json::Value;

/// Void payload — send `{}` on the wire.
pub type ServerGetProcessDiagnosticsPayload = serde_json::Value;

/// Void payload — send `{}` on the wire.
pub type ServerGetSettingsPayload = serde_json::Value;

/// Void payload — send `{}` on the wire.
pub type ServerGetTraceDiagnosticsPayload = serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerLifecycleReadyPayload {
    pub at: TrimmedNonEmptyString,
    pub environment: ExecutionEnvironmentDescriptor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ServerLifecycleStreamEvent {
    ServerLifecycleStreamWelcomeEvent(ServerLifecycleStreamWelcomeEvent),
    ServerLifecycleStreamReadyEvent(ServerLifecycleStreamReadyEvent),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerLifecycleStreamReadyEvent {
    pub payload: ServerLifecycleReadyPayload,
    pub sequence: NonNegativeInt,
    pub r#type: ServerLifecycleStreamReadyEventType,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ServerLifecycleStreamReadyEventType {
    #[default]
    #[serde(rename = "ready")]
    Ready,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerLifecycleStreamWelcomeEvent {
    pub payload: ServerLifecycleWelcomePayload,
    pub sequence: NonNegativeInt,
    pub r#type: ServerLifecycleStreamWelcomeEventType,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ServerLifecycleStreamWelcomeEventType {
    #[default]
    #[serde(rename = "welcome")]
    Welcome,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerLifecycleWelcomePayload {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "bootstrapProjectId")]
    pub bootstrap_project_id: Option<Option<ProjectId>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "bootstrapThreadId")]
    pub bootstrap_thread_id: Option<Option<ThreadId>>,
    pub cwd: TrimmedNonEmptyString,
    pub environment: ExecutionEnvironmentDescriptor,
    #[serde(rename = "projectName")]
    pub project_name: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerObservability {
    #[serde(rename = "localTracingEnabled")]
    pub local_tracing_enabled: bool,
    #[serde(rename = "logsDirectoryPath")]
    pub logs_directory_path: TrimmedNonEmptyString,
    #[serde(rename = "otlpMetricsEnabled")]
    pub otlp_metrics_enabled: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "otlpMetricsUrl")]
    pub otlp_metrics_url: Option<Option<TrimmedNonEmptyString>>,
    #[serde(rename = "otlpTracesEnabled")]
    pub otlp_traces_enabled: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "otlpTracesUrl")]
    pub otlp_traces_url: Option<Option<TrimmedNonEmptyString>>,
}

/// Void payload — send `{}` on the wire.
pub type ServerProbePayload = serde_json::Value;

/// Void payload — send `{}` on the wire.
pub type ServerProbeSuccess = serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProcessDiagnosticsEntry {
    #[serde(rename = "childPids")]
    pub child_pids: Vec<PositiveInt>,
    pub command: TrimmedNonEmptyString,
    #[serde(rename = "cpuPercent")]
    pub cpu_percent: DurationMillis,
    pub depth: NonNegativeInt,
    pub elapsed: TrimmedNonEmptyString,
    pub pgid: EffectOption<i64>,
    pub pid: PositiveInt,
    pub ppid: NonNegativeInt,
    #[serde(rename = "rssBytes")]
    pub rss_bytes: NonNegativeInt,
    pub status: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProcessDiagnosticsResult {
    pub error: EffectOption<ServerProcessDiagnosticsResultError>,
    #[serde(rename = "processCount")]
    pub process_count: NonNegativeInt,
    pub processes: Vec<ServerProcessDiagnosticsEntry>,
    #[serde(rename = "readAt")]
    pub read_at: TrimmedNonEmptyString,
    #[serde(rename = "serverPid")]
    pub server_pid: PositiveInt,
    #[serde(rename = "totalCpuPercent")]
    pub total_cpu_percent: DurationMillis,
    #[serde(rename = "totalRssBytes")]
    pub total_rss_bytes: NonNegativeInt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProcessDiagnosticsResultError {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProcessResourceHistoryBucket {
    #[serde(rename = "avgCpuPercent")]
    pub avg_cpu_percent: DurationMillis,
    #[serde(rename = "endedAt")]
    pub ended_at: TrimmedNonEmptyString,
    #[serde(rename = "maxCpuPercent")]
    pub max_cpu_percent: DurationMillis,
    #[serde(rename = "maxProcessCount")]
    pub max_process_count: NonNegativeInt,
    #[serde(rename = "maxRssBytes")]
    pub max_rss_bytes: NonNegativeInt,
    #[serde(rename = "startedAt")]
    pub started_at: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServerProcessResourceHistoryFailureTag {
    #[serde(rename = "ProcessDiagnosticsQueryTimeoutError")]
    ProcessDiagnosticsQueryTimeoutError,
    #[serde(rename = "ProcessDiagnosticsQueryFailedError")]
    ProcessDiagnosticsQueryFailedError,
    #[serde(rename = "ProcessDiagnosticsServerProcessSignalError")]
    ProcessDiagnosticsServerProcessSignalError,
    #[serde(rename = "ProcessDiagnosticsNotDescendantError")]
    ProcessDiagnosticsNotDescendantError,
    #[serde(rename = "ProcessDiagnosticsSignalFailedError")]
    ProcessDiagnosticsSignalFailedError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProcessResourceHistoryInput {
    #[serde(rename = "bucketMs")]
    pub bucket_ms: NonNegativeInt,
    #[serde(rename = "windowMs")]
    pub window_ms: NonNegativeInt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProcessResourceHistoryResult {
    #[serde(rename = "bucketMs")]
    pub bucket_ms: NonNegativeInt,
    pub buckets: Vec<ServerProcessResourceHistoryBucket>,
    pub error: EffectOption<ServerProcessResourceHistoryResultError>,
    #[serde(rename = "readAt")]
    pub read_at: TrimmedNonEmptyString,
    #[serde(rename = "retainedSampleCount")]
    pub retained_sample_count: NonNegativeInt,
    #[serde(rename = "sampleIntervalMs")]
    pub sample_interval_ms: NonNegativeInt,
    #[serde(rename = "topProcesses")]
    pub top_processes: Vec<ServerProcessResourceHistorySummary>,
    #[serde(rename = "totalCpuSecondsApprox")]
    pub total_cpu_seconds_approx: DurationMillis,
    #[serde(rename = "windowMs")]
    pub window_ms: NonNegativeInt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProcessResourceHistoryResultError {
    #[serde(rename = "failureTag")]
    pub failure_tag: ServerProcessResourceHistoryFailureTag,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProcessResourceHistorySummary {
    #[serde(rename = "avgCpuPercent")]
    pub avg_cpu_percent: DurationMillis,
    pub command: TrimmedNonEmptyString,
    #[serde(rename = "cpuSecondsApprox")]
    pub cpu_seconds_approx: DurationMillis,
    #[serde(rename = "currentCpuPercent")]
    pub current_cpu_percent: DurationMillis,
    #[serde(rename = "currentRssBytes")]
    pub current_rss_bytes: NonNegativeInt,
    pub depth: NonNegativeInt,
    #[serde(rename = "firstSeenAt")]
    pub first_seen_at: TrimmedNonEmptyString,
    #[serde(rename = "isServerRoot")]
    pub is_server_root: bool,
    #[serde(rename = "lastSeenAt")]
    pub last_seen_at: TrimmedNonEmptyString,
    #[serde(rename = "maxCpuPercent")]
    pub max_cpu_percent: DurationMillis,
    #[serde(rename = "maxRssBytes")]
    pub max_rss_bytes: NonNegativeInt,
    pub pid: PositiveInt,
    pub ppid: NonNegativeInt,
    #[serde(rename = "processKey")]
    pub process_key: TrimmedNonEmptyString,
    #[serde(rename = "sampleCount")]
    pub sample_count: NonNegativeInt,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServerProcessSignal {
    #[serde(rename = "SIGINT")]
    SIGINT,
    #[serde(rename = "SIGKILL")]
    SIGKILL,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProvider {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "accentColor")]
    pub accent_color: Option<Option<TrimmedNonEmptyString>>,
    pub auth: ServerProviderAuth,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub availability: Option<Option<ServerProviderAvailability>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "badgeLabel")]
    pub badge_label: Option<Option<TrimmedNonEmptyString>>,
    #[serde(rename = "checkedAt")]
    pub checked_at: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub continuation: Option<Option<ServerProviderContinuation>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "displayName")]
    pub display_name: Option<Option<TrimmedNonEmptyString>>,
    pub driver: ProviderDriverKind,
    pub enabled: bool,
    pub installed: bool,
    #[serde(rename = "instanceId")]
    pub instance_id: ProviderInstanceId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub message: Option<Option<TrimmedNonEmptyString>>,
    pub models: Vec<ServerProviderModel>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "requiresNewThreadForModelChange")]
    pub requires_new_thread_for_model_change: Option<Option<bool>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "showInteractionModeToggle")]
    pub show_interaction_mode_toggle: Option<Option<bool>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub skills: Option<Option<Vec<ServerProviderSkills>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "slashCommands")]
    pub slash_commands: Option<Option<Vec<ServerProviderSlashCommands>>>,
    pub status: ServerProviderState,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "unavailableReason")]
    pub unavailable_reason: Option<Option<TrimmedNonEmptyString>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "updateState")]
    pub update_state: Option<ServerProviderUpdateState>,
    pub version: Option<TrimmedNonEmptyString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "versionAdvisory")]
    pub version_advisory: Option<ServerProviderVersionAdvisory>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProviderSkills {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub description: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "displayName")]
    pub display_name: Option<Option<String>>,
    pub enabled: bool,
    pub name: String,
    pub path: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub scope: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "shortDescription")]
    pub short_description: Option<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProviderSlashCommandsInput {
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProviderSlashCommands {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub description: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub input: Option<Option<ServerProviderSlashCommandsInput>>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProviderUpdateState {
    #[serde(rename = "finishedAt")]
    pub finished_at: Option<String>,
    pub message: Option<TrimmedNonEmptyString>,
    pub output: Option<String>,
    #[serde(rename = "startedAt")]
    pub started_at: Option<String>,
    pub status: ServerProviderUpdateStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProviderVersionAdvisory {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "canUpdate")]
    pub can_update: Option<Option<bool>>,
    #[serde(rename = "checkedAt")]
    pub checked_at: Option<String>,
    #[serde(rename = "currentVersion")]
    pub current_version: Option<TrimmedNonEmptyString>,
    #[serde(rename = "latestVersion")]
    pub latest_version: Option<TrimmedNonEmptyString>,
    pub message: Option<TrimmedNonEmptyString>,
    pub status: ServerProviderVersionAdvisoryStatus,
    #[serde(rename = "updateCommand")]
    pub update_command: Option<TrimmedNonEmptyString>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProviderAuth {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub email: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub label: Option<Option<TrimmedNonEmptyString>>,
    pub status: ServerProviderAuthStatus,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub r#type: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServerProviderAuthStatus {
    #[serde(rename = "authenticated")]
    Authenticated,
    #[serde(rename = "unauthenticated")]
    Unauthenticated,
    #[serde(rename = "unknown")]
    UnknownX,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServerProviderAvailability {
    #[serde(rename = "available")]
    Available,
    #[serde(rename = "unavailable")]
    Unavailable,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProviderContinuation {
    #[serde(rename = "groupKey")]
    pub group_key: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProviderModel {
    pub capabilities: Option<ModelCapabilities>,
    #[serde(rename = "isCustom")]
    pub is_custom: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "isDefault")]
    pub is_default: Option<Option<bool>>,
    pub name: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "shortName")]
    pub short_name: Option<Option<TrimmedNonEmptyString>>,
    pub slug: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "subProvider")]
    pub sub_provider: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServerProviderState {
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "warning")]
    Warning,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "disabled")]
    Disabled,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProviderUpdateError {
    #[serde(rename = "_tag")]
    pub tag: ServerProviderUpdateErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    pub provider: ProviderDriverKind,
    pub reason: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ServerProviderUpdateErrorTag {
    #[default]
    #[serde(rename = "ServerProviderUpdateError")]
    ServerProviderUpdateError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProviderUpdateInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "instanceId")]
    pub instance_id: Option<ProviderInstanceId>,
    pub provider: ProviderDriverKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServerProviderUpdateStatus {
    #[serde(rename = "idle")]
    Idle,
    #[serde(rename = "queued")]
    Queued,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "succeeded")]
    Succeeded,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "unchanged")]
    Unchanged,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProviderUpdatedPayload {
    pub providers: ServerProviders,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServerProviderVersionAdvisoryStatus {
    #[serde(rename = "unknown")]
    UnknownX,
    #[serde(rename = "current")]
    Current,
    #[serde(rename = "behind_latest")]
    BehindLatest,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

pub type ServerProviders = Vec<ServerProvider>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerRefreshProvidersPayload {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "instanceId")]
    pub instance_id: Option<Option<ProviderInstanceId>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerRemoveKeybindingInput {
    pub command: KeybindingCommand,
    pub key: KeybindingValue,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub when: Option<Option<KeybindingWhen>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerRemoveKeybindingResult {
    pub issues: Vec<ServerConfigIssue>,
    pub keybindings: ResolvedKeybindingsConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSelfUpdateError {
    #[serde(rename = "_tag")]
    pub tag: ServerSelfUpdateErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    pub reason: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ServerSelfUpdateErrorTag {
    #[default]
    #[serde(rename = "ServerSelfUpdateError")]
    ServerSelfUpdateError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSelfUpdateInput {
    #[serde(rename = "targetVersion")]
    pub target_version: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServerSelfUpdateMethod {
    #[serde(rename = "boot-service")]
    BootService,
    #[serde(rename = "respawn")]
    Respawn,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSelfUpdateResult {
    pub method: ServerSelfUpdateMethod,
    #[serde(rename = "targetVersion")]
    pub target_version: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettings {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "addProjectBaseDirectory")]
    pub add_project_base_directory: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "autoCompactEnabled")]
    pub auto_compact_enabled: Option<Option<bool>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "autoCompactThresholdTokens")]
    pub auto_compact_threshold_tokens: Option<AutoCompactThresholdTokens>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "automaticGitFetchInterval")]
    pub automatic_git_fetch_interval: Option<Option<DurationMillis>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "defaultThreadEnvMode")]
    pub default_thread_env_mode: Option<Option<ThreadEnvMode>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "enableAssistantStreaming")]
    pub enable_assistant_streaming: Option<Option<bool>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "enableProviderUpdateChecks")]
    pub enable_provider_update_checks: Option<Option<bool>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "knowledgeGraph")]
    pub knowledge_graph: Option<Option<ServerSettingsKnowledgeGraph>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "languageServers")]
    pub language_servers: Option<Option<Vec<ServerSettingsLanguageServers>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "newWorktreesStartFromOrigin")]
    pub new_worktrees_start_from_origin: Option<Option<bool>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub observability: Option<Option<ServerSettingsObservability>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "providerInstances")]
    pub provider_instances: Option<Option<BTreeMap<String, ServerSettingsProviderInstances>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub providers: Option<Option<ServerSettingsProviders>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "textGenerationModelSelection")]
    pub text_generation_model_selection: Option<Option<ServerSettingsTextGenerationModelSelection>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "threadReferenceMaxChars")]
    pub thread_reference_max_chars: Option<ThreadReferenceMaxChars>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsKnowledgeGraph {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "autoRebuild")]
    pub auto_rebuild: Option<Option<bool>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub enabled: Option<Option<bool>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "graphifyPath")]
    pub graphify_path: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "maxStoreMegabytes")]
    pub max_store_megabytes: Option<Option<NonNegativeInt>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "retentionDays")]
    pub retention_days: Option<Option<NonNegativeInt>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsLanguageServers {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub args: Option<Option<Vec<String>>>,
    pub command: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub extensions: Vec<String>,
    #[serde(rename = "languageId")]
    pub language_id: String,
    #[serde(rename = "serverId")]
    pub server_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsObservability {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "otlpMetricsUrl")]
    pub otlp_metrics_url: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "otlpTracesUrl")]
    pub otlp_traces_url: Option<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsProviderInstancesEnvironment {
    pub name: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub sensitive: Option<Option<bool>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub value: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "valueRedacted")]
    pub value_redacted: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsProviderInstances {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "accentColor")]
    pub accent_color: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "displayName")]
    pub display_name: Option<Option<String>>,
    pub driver: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<Vec<ServerSettingsProviderInstancesEnvironment>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsProvidersClaudeAgent {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "binaryPath")]
    pub binary_path: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "customModels")]
    pub custom_models: Option<Option<Vec<String>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub enabled: Option<Option<bool>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "homePath")]
    pub home_path: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "launchArgs")]
    pub launch_args: Option<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsProvidersCodex {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "binaryPath")]
    pub binary_path: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "customModels")]
    pub custom_models: Option<Option<Vec<String>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub enabled: Option<Option<bool>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "homePath")]
    pub home_path: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "launchArgs")]
    pub launch_args: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "shadowHomePath")]
    pub shadow_home_path: Option<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsProvidersCursor {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "apiEndpoint")]
    pub api_endpoint: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "binaryPath")]
    pub binary_path: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "customModels")]
    pub custom_models: Option<Option<Vec<String>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub enabled: Option<Option<bool>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsProvidersGrok {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "binaryPath")]
    pub binary_path: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "customModels")]
    pub custom_models: Option<Option<Vec<String>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub enabled: Option<Option<bool>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsProvidersOpencode {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "binaryPath")]
    pub binary_path: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "customModels")]
    pub custom_models: Option<Option<Vec<String>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub enabled: Option<Option<bool>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "serverPassword")]
    pub server_password: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "serverUrl")]
    pub server_url: Option<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsProviders {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "claudeAgent")]
    pub claude_agent: Option<Option<ServerSettingsProvidersClaudeAgent>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub codex: Option<Option<ServerSettingsProvidersCodex>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cursor: Option<Option<ServerSettingsProvidersCursor>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub grok: Option<Option<ServerSettingsProvidersGrok>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub opencode: Option<Option<ServerSettingsProvidersOpencode>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsTextGenerationModelSelection {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "instanceId")]
    pub instance_id: Option<Option<serde_json::Value>>,
    pub model: serde_json::Value,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub options: Option<Option<serde_json::Value>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub provider: Option<Option<serde_json::Value>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsError {
    #[serde(rename = "_tag")]
    pub tag: ServerSettingsErrorTag,
    pub cause: serde_json::Value,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "environmentVariable")]
    pub environment_variable: Option<Option<String>>,
    pub operation: ServerSettingsOperation,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "providerInstanceId")]
    pub provider_instance_id: Option<Option<String>>,
    #[serde(rename = "settingsPath")]
    pub settings_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ServerSettingsErrorTag {
    #[default]
    #[serde(rename = "ServerSettingsError")]
    ServerSettingsError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServerSettingsOperation {
    #[serde(rename = "normalize")]
    Normalize,
    #[serde(rename = "check-exists")]
    CheckExists,
    #[serde(rename = "read-file")]
    ReadFile,
    #[serde(rename = "read-secret")]
    ReadSecret,
    #[serde(rename = "remove-secret")]
    RemoveSecret,
    #[serde(rename = "remove-stale-secret")]
    RemoveStaleSecret,
    #[serde(rename = "write-secret")]
    WriteSecret,
    #[serde(rename = "write-file")]
    WriteFile,
    #[serde(rename = "prepare-directory")]
    PrepareDirectory,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "addProjectBaseDirectory")]
    pub add_project_base_directory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "autoCompactEnabled")]
    pub auto_compact_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "autoCompactThresholdTokens")]
    pub auto_compact_threshold_tokens: Option<AutoCompactThresholdTokens2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "automaticGitFetchInterval")]
    pub automatic_git_fetch_interval: Option<DurationMillis>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "defaultThreadEnvMode")]
    pub default_thread_env_mode: Option<ServerSettingsPatchDefaultThreadEnvMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "enableAssistantStreaming")]
    pub enable_assistant_streaming: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "enableProviderUpdateChecks")]
    pub enable_provider_update_checks: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "knowledgeGraph")]
    pub knowledge_graph: Option<ServerSettingsPatchKnowledgeGraph>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "languageServers")]
    pub language_servers: Option<Vec<CustomLanguageServer>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "newWorktreesStartFromOrigin")]
    pub new_worktrees_start_from_origin: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observability: Option<ServerSettingsPatchObservability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "providerInstances")]
    pub provider_instances: Option<BTreeMap<String, ProviderInstanceConfig>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub providers: Option<ServerSettingsPatchProviders>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "textGenerationModelSelection")]
    pub text_generation_model_selection: Option<ServerSettingsPatchTextGenerationModelSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "threadReferenceMaxChars")]
    pub thread_reference_max_chars: Option<ThreadReferenceMaxChars2>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServerSettingsPatchDefaultThreadEnvMode {
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "worktree")]
    Worktree,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsPatchKnowledgeGraph {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "autoRebuild")]
    pub auto_rebuild: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "graphifyPath")]
    pub graphify_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "maxStoreMegabytes")]
    pub max_store_megabytes: Option<NonNegativeInt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "retentionDays")]
    pub retention_days: Option<NonNegativeInt>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsPatchObservability {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "otlpMetricsUrl")]
    pub otlp_metrics_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "otlpTracesUrl")]
    pub otlp_traces_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsPatchProvidersClaudeAgent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "binaryPath")]
    pub binary_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "customModels")]
    pub custom_models: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "homePath")]
    pub home_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "launchArgs")]
    pub launch_args: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsPatchProvidersCodex {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "binaryPath")]
    pub binary_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "customModels")]
    pub custom_models: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "homePath")]
    pub home_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "launchArgs")]
    pub launch_args: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "shadowHomePath")]
    pub shadow_home_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsPatchProvidersCursor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "apiEndpoint")]
    pub api_endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "binaryPath")]
    pub binary_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "customModels")]
    pub custom_models: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsPatchProvidersGrok {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "binaryPath")]
    pub binary_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "customModels")]
    pub custom_models: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsPatchProvidersOpencode {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "binaryPath")]
    pub binary_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "customModels")]
    pub custom_models: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "serverPassword")]
    pub server_password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "serverUrl")]
    pub server_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsPatchProviders {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "claudeAgent")]
    pub claude_agent: Option<ServerSettingsPatchProvidersClaudeAgent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<ServerSettingsPatchProvidersCodex>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<ServerSettingsPatchProvidersCursor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grok: Option<ServerSettingsPatchProvidersGrok>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencode: Option<ServerSettingsPatchProvidersOpencode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettingsPatchTextGenerationModelSelection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "instanceId")]
    pub instance_id: Option<ProviderInstanceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<TrimmedNonEmptyString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSignalProcessInput {
    pub pid: PositiveInt,
    pub signal: ServerProcessSignal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSignalProcessResult {
    pub message: EffectOption<String>,
    pub pid: PositiveInt,
    pub signal: ServerProcessSignal,
    pub signaled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServerTraceDiagnosticsErrorKind {
    #[serde(rename = "trace-file-not-found")]
    TraceFileNotFound,
    #[serde(rename = "trace-file-read-failed")]
    TraceFileReadFailed,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerTraceDiagnosticsFailureSummary {
    pub cause: TrimmedNonEmptyString,
    pub count: NonNegativeInt,
    #[serde(rename = "lastSeenAt")]
    pub last_seen_at: TrimmedNonEmptyString,
    pub name: TrimmedNonEmptyString,
    #[serde(rename = "spanId")]
    pub span_id: TrimmedNonEmptyString,
    #[serde(rename = "traceId")]
    pub trace_id: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerTraceDiagnosticsLogEvent {
    pub level: TrimmedNonEmptyString,
    pub message: TrimmedNonEmptyString,
    #[serde(rename = "seenAt")]
    pub seen_at: TrimmedNonEmptyString,
    #[serde(rename = "spanId")]
    pub span_id: TrimmedNonEmptyString,
    #[serde(rename = "spanName")]
    pub span_name: TrimmedNonEmptyString,
    #[serde(rename = "traceId")]
    pub trace_id: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerTraceDiagnosticsRecentFailure {
    pub cause: TrimmedNonEmptyString,
    #[serde(rename = "durationMs")]
    pub duration_ms: DurationMillis,
    #[serde(rename = "endedAt")]
    pub ended_at: TrimmedNonEmptyString,
    pub name: TrimmedNonEmptyString,
    #[serde(rename = "spanId")]
    pub span_id: TrimmedNonEmptyString,
    #[serde(rename = "traceId")]
    pub trace_id: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerTraceDiagnosticsResult {
    #[serde(rename = "commonFailures")]
    pub common_failures: Vec<ServerTraceDiagnosticsFailureSummary>,
    pub error: EffectOption<ServerTraceDiagnosticsResultError>,
    #[serde(rename = "failureCount")]
    pub failure_count: NonNegativeInt,
    #[serde(rename = "firstSpanAt")]
    pub first_span_at: EffectOption<TrimmedNonEmptyString>,
    #[serde(rename = "interruptionCount")]
    pub interruption_count: NonNegativeInt,
    #[serde(rename = "lastSpanAt")]
    pub last_span_at: EffectOption<TrimmedNonEmptyString>,
    #[serde(rename = "latestFailures")]
    pub latest_failures: Vec<ServerTraceDiagnosticsRecentFailure>,
    #[serde(rename = "latestWarningAndErrorLogs")]
    pub latest_warning_and_error_logs: Vec<ServerTraceDiagnosticsLogEvent>,
    #[serde(rename = "logLevelCounts")]
    pub log_level_counts: BTreeMap<String, NonNegativeInt>,
    #[serde(rename = "parseErrorCount")]
    pub parse_error_count: NonNegativeInt,
    #[serde(rename = "partialFailure")]
    pub partial_failure: EffectOption<bool>,
    #[serde(rename = "readAt")]
    pub read_at: TrimmedNonEmptyString,
    #[serde(rename = "recordCount")]
    pub record_count: NonNegativeInt,
    #[serde(rename = "scannedFilePaths")]
    pub scanned_file_paths: Vec<TrimmedNonEmptyString>,
    #[serde(rename = "slowSpanCount")]
    pub slow_span_count: NonNegativeInt,
    #[serde(rename = "slowSpanThresholdMs")]
    pub slow_span_threshold_ms: NonNegativeInt,
    #[serde(rename = "slowestSpans")]
    pub slowest_spans: Vec<ServerTraceDiagnosticsSpanOccurrence>,
    #[serde(rename = "topSpansByCount")]
    pub top_spans_by_count: Vec<ServerTraceDiagnosticsSpanSummary>,
    #[serde(rename = "traceFilePath")]
    pub trace_file_path: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerTraceDiagnosticsResultError {
    pub kind: ServerTraceDiagnosticsErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerTraceDiagnosticsSpanOccurrence {
    #[serde(rename = "durationMs")]
    pub duration_ms: DurationMillis,
    #[serde(rename = "endedAt")]
    pub ended_at: TrimmedNonEmptyString,
    pub name: TrimmedNonEmptyString,
    #[serde(rename = "spanId")]
    pub span_id: TrimmedNonEmptyString,
    #[serde(rename = "traceId")]
    pub trace_id: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerTraceDiagnosticsSpanSummary {
    #[serde(rename = "averageDurationMs")]
    pub average_duration_ms: DurationMillis,
    pub count: NonNegativeInt,
    #[serde(rename = "failureCount")]
    pub failure_count: NonNegativeInt,
    #[serde(rename = "maxDurationMs")]
    pub max_duration_ms: DurationMillis,
    pub name: TrimmedNonEmptyString,
    #[serde(rename = "totalDurationMs")]
    pub total_duration_ms: DurationMillis,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerUpdateSettingsPayload {
    pub patch: ServerSettingsPatch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerUpsertKeybindingInput {
    pub command: KeybindingCommand,
    pub key: KeybindingValue,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub replace: Option<Option<ServerRemoveKeybindingInput>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub when: Option<Option<KeybindingWhen>>,
}

pub type ShellOpenInEditorSuccess = ();

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceControlCloneProtocol {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "ssh")]
    Ssh,
    #[serde(rename = "https")]
    Https,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceControlCloneRepositoryInput {
    #[serde(rename = "destinationPath")]
    pub destination_path: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub protocol: Option<Option<SourceControlCloneProtocol>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub provider: Option<Option<SourceControlProviderKind>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "remoteUrl")]
    pub remote_url: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub repository: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceControlCloneRepositoryResult {
    pub cwd: TrimmedNonEmptyString,
    #[serde(rename = "remoteUrl")]
    pub remote_url: TrimmedNonEmptyString,
    pub repository: Option<SourceControlRepositoryInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceControlDiscoveryResult {
    #[serde(rename = "sourceControlProviders")]
    pub source_control_providers: Vec<SourceControlProviderDiscoveryItem>,
    #[serde(rename = "versionControlSystems")]
    pub version_control_systems: Vec<VcsDiscoveryItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceControlDiscoveryStatus {
    #[serde(rename = "available")]
    Available,
    #[serde(rename = "missing")]
    Missing,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceControlProviderAuth {
    pub account: EffectOption<String>,
    pub detail: EffectOption<String>,
    pub host: EffectOption<String>,
    pub status: SourceControlProviderAuthStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceControlProviderAuthStatus {
    #[serde(rename = "authenticated")]
    Authenticated,
    #[serde(rename = "unauthenticated")]
    Unauthenticated,
    #[serde(rename = "unknown")]
    UnknownX,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceControlProviderDiscoveryItem {
    pub auth: SourceControlProviderAuth,
    pub detail: EffectOption<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub executable: Option<Option<TrimmedNonEmptyString>>,
    #[serde(rename = "installHint")]
    pub install_hint: TrimmedNonEmptyString,
    pub kind: SourceControlProviderKind,
    pub label: TrimmedNonEmptyString,
    pub status: SourceControlDiscoveryStatus,
    pub version: EffectOption<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceControlProviderError {
    #[serde(rename = "_tag")]
    pub tag: SourceControlProviderErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub command: Option<Option<TrimmedNonEmptyString>>,
    pub cwd: TrimmedNonEmptyString,
    pub detail: TrimmedNonEmptyString,
    pub operation: TrimmedNonEmptyString,
    pub provider: SourceControlProviderKind,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub reference: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub repository: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SourceControlProviderErrorTag {
    #[default]
    #[serde(rename = "SourceControlProviderError")]
    SourceControlProviderError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceControlProviderInfo {
    #[serde(rename = "baseUrl")]
    pub base_url: TrimmedNonEmptyString,
    pub kind: SourceControlProviderKind,
    pub name: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceControlProviderKind {
    #[serde(rename = "github")]
    Github,
    #[serde(rename = "gitlab")]
    Gitlab,
    #[serde(rename = "azure-devops")]
    AzureDevops,
    #[serde(rename = "bitbucket")]
    Bitbucket,
    #[serde(rename = "unknown")]
    UnknownX,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceControlPublishRepositoryInput {
    pub cwd: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub protocol: Option<Option<SourceControlCloneProtocol>>,
    pub provider: SourceControlProviderKind,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "remoteName")]
    pub remote_name: Option<Option<TrimmedNonEmptyString>>,
    pub repository: TrimmedNonEmptyString,
    pub visibility: SourceControlRepositoryVisibility,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceControlPublishRepositoryResult {
    pub branch: TrimmedNonEmptyString,
    #[serde(rename = "remoteName")]
    pub remote_name: TrimmedNonEmptyString,
    #[serde(rename = "remoteUrl")]
    pub remote_url: TrimmedNonEmptyString,
    pub repository: SourceControlRepositoryInfo,
    pub status: SourceControlPublishStatus,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "upstreamBranch")]
    pub upstream_branch: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceControlPublishStatus {
    #[serde(rename = "pushed")]
    Pushed,
    #[serde(rename = "remote_added")]
    RemoteAdded,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceControlRepositoryError {
    #[serde(rename = "_tag")]
    pub tag: SourceControlRepositoryErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    pub detail: TrimmedNonEmptyString,
    pub operation: TrimmedNonEmptyString,
    pub provider: SourceControlProviderKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SourceControlRepositoryErrorTag {
    #[default]
    #[serde(rename = "SourceControlRepositoryError")]
    SourceControlRepositoryError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceControlRepositoryInfo {
    #[serde(rename = "nameWithOwner")]
    pub name_with_owner: TrimmedNonEmptyString,
    pub provider: SourceControlProviderKind,
    #[serde(rename = "sshUrl")]
    pub ssh_url: TrimmedNonEmptyString,
    pub url: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceControlRepositoryLookupInput {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cwd: Option<Option<TrimmedNonEmptyString>>,
    pub provider: SourceControlProviderKind,
    pub repository: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceControlRepositoryVisibility {
    #[serde(rename = "private")]
    Private,
    #[serde(rename = "public")]
    Public,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

/// Void payload — send `{}` on the wire.
pub type SubscribeAuthAccessPayload = serde_json::Value;

/// Void payload — send `{}` on the wire.
pub type SubscribeDiscoveredLocalServersPayload = serde_json::Value;

/// Void payload — send `{}` on the wire.
pub type SubscribePreviewEventsPayload = serde_json::Value;

/// Void payload — send `{}` on the wire.
pub type SubscribeServerConfigPayload = serde_json::Value;

/// Void payload — send `{}` on the wire.
pub type SubscribeServerLifecyclePayload = serde_json::Value;

/// Void payload — send `{}` on the wire.
pub type SubscribeTerminalEventsPayload = serde_json::Value;

/// Void payload — send `{}` on the wire.
pub type SubscribeTerminalMetadataPayload = serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalAttachInput {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cols: Option<Option<i64>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cwd: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub env: Option<Option<serde_json::Map<String, serde_json::Value>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "restartIfNotRunning")]
    pub restart_if_not_running: Option<Option<bool>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub rows: Option<Option<i64>>,
    #[serde(rename = "terminalId")]
    pub terminal_id: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "worktreePath")]
    pub worktree_path: Option<Option<Option<TrimmedNonEmptyString>>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TerminalAttachStreamEvent {
    #[serde(rename = "snapshot")]
    Snapshot { snapshot: TerminalSessionSnapshot },
    #[serde(rename = "output")]
    Output {
        data: TrimmedNonEmptyString,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        sequence: Option<Option<i64>>,
        #[serde(rename = "terminalId")]
        terminal_id: String,
        #[serde(rename = "threadId")]
        thread_id: String,
    },
    #[serde(rename = "exited")]
    Exited {
        #[serde(rename = "exitCode")]
        exit_code: Option<i64>,
        #[serde(rename = "exitSignal")]
        exit_signal: Option<i64>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        sequence: Option<Option<i64>>,
        #[serde(rename = "terminalId")]
        terminal_id: String,
        #[serde(rename = "threadId")]
        thread_id: String,
    },
    #[serde(rename = "closed")]
    Closed {
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        sequence: Option<Option<i64>>,
        #[serde(rename = "terminalId")]
        terminal_id: String,
        #[serde(rename = "threadId")]
        thread_id: String,
    },
    #[serde(rename = "error")]
    Error {
        message: String,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        sequence: Option<Option<i64>>,
        #[serde(rename = "terminalId")]
        terminal_id: String,
        #[serde(rename = "threadId")]
        thread_id: String,
    },
    #[serde(rename = "cleared")]
    Cleared {
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        sequence: Option<Option<i64>>,
        #[serde(rename = "terminalId")]
        terminal_id: String,
        #[serde(rename = "threadId")]
        thread_id: String,
    },
    #[serde(rename = "restarted")]
    Restarted {
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        sequence: Option<Option<i64>>,
        snapshot: TerminalSessionSnapshot,
        #[serde(rename = "terminalId")]
        terminal_id: String,
        #[serde(rename = "threadId")]
        thread_id: String,
    },
    #[serde(rename = "activity")]
    Activity {
        #[serde(rename = "hasRunningSubprocess")]
        has_running_subprocess: bool,
        label: String,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        sequence: Option<Option<i64>>,
        #[serde(rename = "terminalId")]
        terminal_id: String,
        #[serde(rename = "threadId")]
        thread_id: String,
    },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalClearInput {
    #[serde(rename = "terminalId")]
    pub terminal_id: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: TrimmedNonEmptyString,
}

pub type TerminalClearSuccess = ();

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalCloseInput {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "deleteHistory")]
    pub delete_history: Option<Option<bool>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "terminalId")]
    pub terminal_id: Option<Option<TrimmedNonEmptyString>>,
    #[serde(rename = "threadId")]
    pub thread_id: TrimmedNonEmptyString,
}

pub type TerminalCloseSuccess = ();

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TerminalCwdError {
    TerminalCwdNotFoundError(TerminalCwdNotFoundError),
    TerminalCwdNotDirectoryError(TerminalCwdNotDirectoryError),
    TerminalCwdStatError(TerminalCwdStatError),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalCwdNotDirectoryError {
    #[serde(rename = "_tag")]
    pub tag: TerminalCwdNotDirectoryErrorTag,
    pub cwd: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TerminalCwdNotDirectoryErrorTag {
    #[default]
    #[serde(rename = "TerminalCwdNotDirectoryError")]
    TerminalCwdNotDirectoryError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalCwdNotFoundError {
    #[serde(rename = "_tag")]
    pub tag: TerminalCwdNotFoundErrorTag,
    pub cwd: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TerminalCwdNotFoundErrorTag {
    #[default]
    #[serde(rename = "TerminalCwdNotFoundError")]
    TerminalCwdNotFoundError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalCwdStatError {
    #[serde(rename = "_tag")]
    pub tag: TerminalCwdStatErrorTag,
    pub cause: serde_json::Value,
    pub cwd: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TerminalCwdStatErrorTag {
    #[default]
    #[serde(rename = "TerminalCwdStatError")]
    TerminalCwdStatError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TerminalError {
    TerminalCwdError(TerminalCwdError),
    TerminalHistoryError(TerminalHistoryError),
    TerminalSessionLookupError(TerminalSessionLookupError),
    TerminalNotRunningError(TerminalNotRunningError),
    TerminalWriteError(TerminalWriteError),
    TerminalResizeError(TerminalResizeError),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TerminalEvent {
    #[serde(rename = "started")]
    Started {
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        sequence: Option<Option<i64>>,
        snapshot: TerminalSessionSnapshot,
        #[serde(rename = "terminalId")]
        terminal_id: String,
        #[serde(rename = "threadId")]
        thread_id: String,
    },
    #[serde(rename = "output")]
    Output {
        data: TrimmedNonEmptyString,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        sequence: Option<Option<i64>>,
        #[serde(rename = "terminalId")]
        terminal_id: String,
        #[serde(rename = "threadId")]
        thread_id: String,
    },
    #[serde(rename = "exited")]
    Exited {
        #[serde(rename = "exitCode")]
        exit_code: Option<i64>,
        #[serde(rename = "exitSignal")]
        exit_signal: Option<i64>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        sequence: Option<Option<i64>>,
        #[serde(rename = "terminalId")]
        terminal_id: String,
        #[serde(rename = "threadId")]
        thread_id: String,
    },
    #[serde(rename = "closed")]
    Closed {
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        sequence: Option<Option<i64>>,
        #[serde(rename = "terminalId")]
        terminal_id: String,
        #[serde(rename = "threadId")]
        thread_id: String,
    },
    #[serde(rename = "error")]
    Error {
        message: String,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        sequence: Option<Option<i64>>,
        #[serde(rename = "terminalId")]
        terminal_id: String,
        #[serde(rename = "threadId")]
        thread_id: String,
    },
    #[serde(rename = "cleared")]
    Cleared {
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        sequence: Option<Option<i64>>,
        #[serde(rename = "terminalId")]
        terminal_id: String,
        #[serde(rename = "threadId")]
        thread_id: String,
    },
    #[serde(rename = "restarted")]
    Restarted {
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        sequence: Option<Option<i64>>,
        snapshot: TerminalSessionSnapshot,
        #[serde(rename = "terminalId")]
        terminal_id: String,
        #[serde(rename = "threadId")]
        thread_id: String,
    },
    #[serde(rename = "activity")]
    Activity {
        #[serde(rename = "hasRunningSubprocess")]
        has_running_subprocess: bool,
        label: String,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::support::double_option"
        )]
        sequence: Option<Option<i64>>,
        #[serde(rename = "terminalId")]
        terminal_id: String,
        #[serde(rename = "threadId")]
        thread_id: String,
    },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalHistoryError {
    #[serde(rename = "_tag")]
    pub tag: TerminalHistoryErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    pub operation: TerminalHistoryErrorOperation,
    #[serde(rename = "terminalId")]
    pub terminal_id: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TerminalHistoryErrorTag {
    #[default]
    #[serde(rename = "TerminalHistoryError")]
    TerminalHistoryError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TerminalHistoryErrorOperation {
    #[serde(rename = "read")]
    Read,
    #[serde(rename = "truncate")]
    Truncate,
    #[serde(rename = "migrate")]
    Migrate,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TerminalMetadataStreamEvent {
    #[serde(rename = "snapshot")]
    Snapshot { terminals: Vec<TerminalSummary> },
    #[serde(rename = "upsert")]
    Upsert { terminal: TerminalSummary },
    #[serde(rename = "remove")]
    Remove {
        #[serde(rename = "terminalId")]
        terminal_id: String,
        #[serde(rename = "threadId")]
        thread_id: String,
    },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalNotRunningError {
    #[serde(rename = "_tag")]
    pub tag: TerminalNotRunningErrorTag,
    #[serde(rename = "terminalId")]
    pub terminal_id: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TerminalNotRunningErrorTag {
    #[default]
    #[serde(rename = "TerminalNotRunningError")]
    TerminalNotRunningError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalOpenInput {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cols: Option<Option<i64>>,
    pub cwd: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub env: Option<Option<serde_json::Map<String, serde_json::Value>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub rows: Option<Option<i64>>,
    #[serde(rename = "terminalId")]
    pub terminal_id: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "worktreePath")]
    pub worktree_path: Option<Option<Option<TrimmedNonEmptyString>>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalResizeError {
    #[serde(rename = "_tag")]
    pub tag: TerminalResizeErrorTag,
    pub cause: serde_json::Value,
    pub cols: i64,
    pub rows: i64,
    #[serde(rename = "terminalId")]
    pub terminal_id: TrimmedNonEmptyString,
    #[serde(rename = "terminalPid")]
    pub terminal_pid: DurationMillis,
    #[serde(rename = "threadId")]
    pub thread_id: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TerminalResizeErrorTag {
    #[default]
    #[serde(rename = "TerminalResizeError")]
    TerminalResizeError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalResizeInput {
    pub cols: i64,
    pub rows: i64,
    #[serde(rename = "terminalId")]
    pub terminal_id: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: TrimmedNonEmptyString,
}

pub type TerminalResizeSuccess = ();

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalRestartInput {
    pub cols: i64,
    pub cwd: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub env: Option<Option<serde_json::Map<String, serde_json::Value>>>,
    pub rows: i64,
    #[serde(rename = "terminalId")]
    pub terminal_id: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "worktreePath")]
    pub worktree_path: Option<Option<Option<TrimmedNonEmptyString>>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalSessionLookupError {
    #[serde(rename = "_tag")]
    pub tag: TerminalSessionLookupErrorTag,
    #[serde(rename = "terminalId")]
    pub terminal_id: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TerminalSessionLookupErrorTag {
    #[default]
    #[serde(rename = "TerminalSessionLookupError")]
    TerminalSessionLookupError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalSessionSnapshot {
    pub cwd: String,
    #[serde(rename = "exitCode")]
    pub exit_code: Option<i64>,
    #[serde(rename = "exitSignal")]
    pub exit_signal: Option<i64>,
    pub history: TrimmedNonEmptyString,
    pub label: String,
    pub pid: Option<i64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub sequence: Option<Option<i64>>,
    pub status: TerminalSessionStatus,
    #[serde(rename = "terminalId")]
    pub terminal_id: String,
    #[serde(rename = "threadId")]
    pub thread_id: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
    #[serde(rename = "worktreePath")]
    pub worktree_path: Option<TrimmedNonEmptyString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TerminalSessionStatus {
    #[serde(rename = "starting")]
    Starting,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "exited")]
    Exited,
    #[serde(rename = "error")]
    Error,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalSummary {
    pub cwd: String,
    #[serde(rename = "exitCode")]
    pub exit_code: Option<i64>,
    #[serde(rename = "exitSignal")]
    pub exit_signal: Option<i64>,
    #[serde(rename = "hasRunningSubprocess")]
    pub has_running_subprocess: bool,
    pub label: String,
    pub pid: Option<i64>,
    pub status: TerminalSessionStatus,
    #[serde(rename = "terminalId")]
    pub terminal_id: String,
    #[serde(rename = "threadId")]
    pub thread_id: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
    #[serde(rename = "worktreePath")]
    pub worktree_path: Option<TrimmedNonEmptyString>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalWriteError {
    #[serde(rename = "_tag")]
    pub tag: TerminalWriteErrorTag,
    pub cause: serde_json::Value,
    #[serde(rename = "terminalId")]
    pub terminal_id: TrimmedNonEmptyString,
    #[serde(rename = "terminalPid")]
    pub terminal_pid: DurationMillis,
    #[serde(rename = "threadId")]
    pub thread_id: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TerminalWriteErrorTag {
    #[default]
    #[serde(rename = "TerminalWriteError")]
    TerminalWriteError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalWriteInput {
    pub data: String,
    #[serde(rename = "terminalId")]
    pub terminal_id: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: TrimmedNonEmptyString,
}

pub type TerminalWriteSuccess = ();

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextGenerationError {
    #[serde(rename = "_tag")]
    pub tag: TextGenerationErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    pub detail: TrimmedNonEmptyString,
    pub operation: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TextGenerationErrorTag {
    #[default]
    #[serde(rename = "TextGenerationError")]
    TextGenerationError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadActivityAppendedPayload {
    pub activity: OrchestrationThreadActivity,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadApprovalResponseRequestedPayload {
    #[serde(rename = "createdAt")]
    pub created_at: TrimmedNonEmptyString,
    pub decision: ProviderApprovalDecision,
    #[serde(rename = "requestId")]
    pub request_id: ApprovalRequestId,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadArchivedPayload {
    #[serde(rename = "archivedAt")]
    pub archived_at: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadCheckpointRevertRequestedPayload {
    #[serde(rename = "createdAt")]
    pub created_at: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "turnCount")]
    pub turn_count: NonNegativeInt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadCreatedPayload {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "additionalRoots")]
    pub additional_roots: Option<Option<Vec<ThreadCreatedPayloadAdditionalRoots>>>,
    pub branch: Option<TrimmedNonEmptyString>,
    #[serde(rename = "createdAt")]
    pub created_at: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "interactionMode")]
    pub interaction_mode: Option<Option<ProviderInteractionMode>>,
    #[serde(rename = "modelSelection")]
    pub model_selection: ModelSelection,
    #[serde(rename = "projectId")]
    pub project_id: ProjectId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "runtimeMode")]
    pub runtime_mode: Option<Option<RuntimeMode>>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    pub title: TrimmedNonEmptyString,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
    #[serde(rename = "worktreePath")]
    pub worktree_path: Option<TrimmedNonEmptyString>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ThreadCreatedPayloadAdditionalRoots {
    #[serde(rename = "project")]
    Project {
        #[serde(rename = "projectId")]
        project_id: String,
    },
    #[serde(rename = "path")]
    Path { path: String },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadDeletedPayload {
    #[serde(rename = "deletedAt")]
    pub deleted_at: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThreadEnvMode {
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "worktree")]
    Worktree,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThreadId(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadInteractionModeSetPayload {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "interactionMode")]
    pub interaction_mode: Option<Option<ProviderInteractionMode>>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadMessageSentPayload {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub attachments: Option<Option<Vec<ChatAttachment>>>,
    #[serde(rename = "createdAt")]
    pub created_at: TrimmedNonEmptyString,
    #[serde(rename = "messageId")]
    pub message_id: MessageId,
    pub role: OrchestrationMessageRole,
    pub streaming: bool,
    pub text: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "turnId")]
    pub turn_id: Option<TurnId>,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadMetaUpdatedPayload {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "additionalRoots")]
    pub additional_roots: Option<Option<Vec<WorkspaceRootRef>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub branch: Option<Option<Option<TrimmedNonEmptyString>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "modelSelection")]
    pub model_selection: Option<Option<ModelSelection>>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub title: Option<Option<TrimmedNonEmptyString>>,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "worktreePath")]
    pub worktree_path: Option<Option<Option<TrimmedNonEmptyString>>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadProposedPlanUpsertedPayload {
    #[serde(rename = "proposedPlan")]
    pub proposed_plan: OrchestrationProposedPlan,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
}

pub type ThreadReferenceMaxChars = Option<ThreadReferenceMaxChars1>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Copy, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThreadReferenceMaxChars1(pub i64);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Copy, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThreadReferenceMaxChars2(pub i64);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadRevertedPayload {
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "turnCount")]
    pub turn_count: NonNegativeInt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadRuntimeModeSetPayload {
    #[serde(rename = "runtimeMode")]
    pub runtime_mode: RuntimeMode,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadSessionSetPayload {
    pub session: OrchestrationSession,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadSessionStopRequestedPayload {
    #[serde(rename = "createdAt")]
    pub created_at: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadSettledPayload {
    #[serde(rename = "settledAt")]
    pub settled_at: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadSnoozedPayload {
    #[serde(rename = "snoozedAt")]
    pub snoozed_at: TrimmedNonEmptyString,
    #[serde(rename = "snoozedUntil")]
    pub snoozed_until: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadTurnDiffCompletedPayload {
    #[serde(rename = "assistantMessageId")]
    pub assistant_message_id: Option<MessageId>,
    #[serde(rename = "checkpointRef")]
    pub checkpoint_ref: CheckpointRef,
    #[serde(rename = "checkpointTurnCount")]
    pub checkpoint_turn_count: NonNegativeInt,
    #[serde(rename = "completedAt")]
    pub completed_at: TrimmedNonEmptyString,
    pub files: Vec<OrchestrationCheckpointFile>,
    pub status: OrchestrationCheckpointStatus,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "turnId")]
    pub turn_id: TurnId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadTurnInterruptRequestedPayload {
    #[serde(rename = "createdAt")]
    pub created_at: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "turnId")]
    pub turn_id: Option<Option<TurnId>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadTurnStartRequestedPayload {
    #[serde(rename = "createdAt")]
    pub created_at: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "interactionMode")]
    pub interaction_mode: Option<Option<ProviderInteractionMode>>,
    #[serde(rename = "messageId")]
    pub message_id: MessageId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "modelSelection")]
    pub model_selection: Option<Option<ModelSelection>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "runtimeMode")]
    pub runtime_mode: Option<Option<RuntimeMode>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "sourceProposedPlan")]
    pub source_proposed_plan: Option<Option<ThreadTurnStartRequestedPayloadSourceProposedPlan>>,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "titleSeed")]
    pub title_seed: Option<Option<TrimmedNonEmptyString>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadTurnStartRequestedPayloadSourceProposedPlan {
    #[serde(rename = "planId")]
    pub plan_id: TrimmedNonEmptyString,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadUnarchivedPayload {
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadUnsettledPayload {
    pub reason: ThreadUnsettledPayloadReason,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThreadUnsettledPayloadReason {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "activity")]
    Activity,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadUnsnoozedPayload {
    pub reason: ThreadUnsnoozedPayloadReason,
    #[serde(rename = "threadId")]
    pub thread_id: ThreadId,
    #[serde(rename = "updatedAt")]
    pub updated_at: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThreadUnsnoozedPayloadReason {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "activity")]
    Activity,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TrimmedNonEmptyString(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TurnId(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsBaselineBlob {
    pub contents: Option<TrimmedNonEmptyString>,
    pub oid: Option<TrimmedNonEmptyString>,
    pub status: VcsBaselineBlobStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VcsBaselineBlobStatus {
    #[serde(rename = "ok")]
    Ok,
    #[serde(rename = "absent")]
    Absent,
    #[serde(rename = "binary")]
    Binary,
    #[serde(rename = "too-large")]
    TooLarge,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsCreateRefInput {
    pub cwd: TrimmedNonEmptyString,
    #[serde(rename = "refName")]
    pub ref_name: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "switchRef")]
    pub switch_ref: Option<Option<bool>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsCreateRefResult {
    #[serde(rename = "refName")]
    pub ref_name: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsCreateWorktreeInput {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "baseRefName")]
    pub base_ref_name: Option<Option<TrimmedNonEmptyString>>,
    pub cwd: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "newRefName")]
    pub new_ref_name: Option<Option<TrimmedNonEmptyString>>,
    pub path: Option<TrimmedNonEmptyString>,
    #[serde(rename = "refName")]
    pub ref_name: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsCreateWorktreeResult {
    pub worktree: VcsCreateWorktreeResultWorktree,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsCreateWorktreeResultWorktree {
    pub path: TrimmedNonEmptyString,
    #[serde(rename = "refName")]
    pub ref_name: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsDiscoveryItem {
    pub detail: EffectOption<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub executable: Option<Option<TrimmedNonEmptyString>>,
    pub implemented: bool,
    #[serde(rename = "installHint")]
    pub install_hint: TrimmedNonEmptyString,
    pub kind: VcsDriverKind,
    pub label: TrimmedNonEmptyString,
    pub status: SourceControlDiscoveryStatus,
    pub version: EffectOption<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VcsDriverKind {
    #[serde(rename = "git")]
    Git,
    #[serde(rename = "jj")]
    Jj,
    #[serde(rename = "unknown")]
    UnknownX,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VcsError {
    VcsProcessSpawnError(VcsProcessSpawnError),
    VcsProcessExitError(VcsProcessExitError),
    VcsProcessTimeoutError(VcsProcessTimeoutError),
    VcsProcessStdinWriteError(VcsProcessStdinWriteError),
    VcsProcessOutputReadError(VcsProcessOutputReadError),
    VcsProcessOutputLimitError(VcsProcessOutputLimitError),
    VcsProcessMissingExitCodeError(VcsProcessMissingExitCodeError),
    VcsRepositoryDetectionError(VcsRepositoryDetectionError),
    VcsUnsupportedOperationError(VcsUnsupportedOperationError),
    /// Forward compatibility: a member this build does not know.
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsFileBaselineInput {
    pub cwd: TrimmedNonEmptyString,
    #[serde(rename = "relativePath")]
    pub relative_path: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsFileBaselineResult {
    pub head: VcsBaselineBlob,
    pub index: VcsBaselineBlob,
    #[serde(rename = "renamedFrom")]
    pub renamed_from: Option<TrimmedNonEmptyString>,
    pub repository: VcsFileBaselineResultRepository,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VcsFileBaselineResultRepository {
    #[serde(rename = "ok")]
    Ok,
    #[serde(rename = "no-repository")]
    NoRepository,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VcsFileStatusCode {
    #[serde(rename = "modified")]
    Modified,
    #[serde(rename = "added")]
    Added,
    #[serde(rename = "deleted")]
    Deleted,
    #[serde(rename = "renamed")]
    Renamed,
    #[serde(rename = "untracked")]
    Untracked,
    #[serde(rename = "conflicted")]
    Conflicted,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsFileStatusEntry {
    pub path: TrimmedNonEmptyString,
    pub staged: bool,
    pub status: VcsFileStatusCode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsFileStatusesInput {
    pub cwd: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsFileStatusesResult {
    pub entries: Vec<VcsFileStatusEntry>,
    pub repository: VcsFileStatusesResultRepository,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VcsFileStatusesResultRepository {
    #[serde(rename = "ok")]
    Ok,
    #[serde(rename = "no-repository")]
    NoRepository,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsInitInput {
    pub cwd: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub kind: Option<Option<VcsDriverKind>>,
}

pub type VcsInitSuccess = ();

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsListRefsInput {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cursor: Option<Option<NonNegativeInt>>,
    pub cwd: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "includeMatchingRemoteRefs")]
    pub include_matching_remote_refs: Option<Option<bool>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub limit: Option<Option<i64>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub query: Option<Option<TrimmedNonEmptyString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "refKind")]
    pub ref_kind: Option<Option<VcsListRefsInputRefKind>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VcsListRefsInputRefKind {
    #[serde(rename = "all")]
    All,
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "remote")]
    Remote,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsListRefsResult {
    #[serde(rename = "hasPrimaryRemote")]
    pub has_primary_remote: bool,
    #[serde(rename = "isRepo")]
    pub is_repo: bool,
    #[serde(rename = "nextCursor")]
    pub next_cursor: Option<NonNegativeInt>,
    pub refs: Vec<VcsRef>,
    #[serde(rename = "totalCount")]
    pub total_count: NonNegativeInt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsProcessExitError {
    #[serde(rename = "_tag")]
    pub tag: VcsProcessExitErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "argumentCount")]
    pub argument_count: Option<Option<NonNegativeInt>>,
    pub command: TrimmedNonEmptyString,
    pub cwd: TrimmedNonEmptyString,
    pub detail: TrimmedNonEmptyString,
    #[serde(rename = "exitCode")]
    pub exit_code: DurationMillis,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "failureKind")]
    pub failure_kind: Option<Option<VcsProcessExitFailureKind>>,
    pub operation: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "stderrLength")]
    pub stderr_length: Option<Option<NonNegativeInt>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "stderrTruncated")]
    pub stderr_truncated: Option<Option<bool>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum VcsProcessExitErrorTag {
    #[default]
    #[serde(rename = "VcsProcessExitError")]
    VcsProcessExitError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VcsProcessExitFailureKind {
    #[serde(rename = "authentication")]
    Authentication,
    #[serde(rename = "not-found")]
    NotFound,
    #[serde(rename = "command-failed")]
    CommandFailed,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsProcessMissingExitCodeError {
    #[serde(rename = "_tag")]
    pub tag: VcsProcessMissingExitCodeErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "argumentCount")]
    pub argument_count: Option<Option<NonNegativeInt>>,
    pub command: TrimmedNonEmptyString,
    pub cwd: TrimmedNonEmptyString,
    pub operation: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum VcsProcessMissingExitCodeErrorTag {
    #[default]
    #[serde(rename = "VcsProcessMissingExitCodeError")]
    VcsProcessMissingExitCodeError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsProcessOutputLimitError {
    #[serde(rename = "_tag")]
    pub tag: VcsProcessOutputLimitErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "argumentCount")]
    pub argument_count: Option<Option<NonNegativeInt>>,
    pub command: TrimmedNonEmptyString,
    pub cwd: TrimmedNonEmptyString,
    #[serde(rename = "maxBytes")]
    pub max_bytes: NonNegativeInt,
    #[serde(rename = "observedBytes")]
    pub observed_bytes: NonNegativeInt,
    pub operation: TrimmedNonEmptyString,
    pub stream: VcsProcessOutputLimitErrorStream,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum VcsProcessOutputLimitErrorTag {
    #[default]
    #[serde(rename = "VcsProcessOutputLimitError")]
    VcsProcessOutputLimitError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VcsProcessOutputLimitErrorStream {
    #[serde(rename = "stdout")]
    Stdout,
    #[serde(rename = "stderr")]
    Stderr,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsProcessOutputReadError {
    #[serde(rename = "_tag")]
    pub tag: VcsProcessOutputReadErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "argumentCount")]
    pub argument_count: Option<Option<NonNegativeInt>>,
    pub cause: serde_json::Value,
    pub command: TrimmedNonEmptyString,
    pub cwd: TrimmedNonEmptyString,
    pub operation: TrimmedNonEmptyString,
    pub stream: VcsProcessOutputReadErrorStream,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum VcsProcessOutputReadErrorTag {
    #[default]
    #[serde(rename = "VcsProcessOutputReadError")]
    VcsProcessOutputReadError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VcsProcessOutputReadErrorStream {
    #[serde(rename = "stdout")]
    Stdout,
    #[serde(rename = "stderr")]
    Stderr,
    #[serde(rename = "exitCode")]
    ExitCode,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsProcessSpawnError {
    #[serde(rename = "_tag")]
    pub tag: VcsProcessSpawnErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "argumentCount")]
    pub argument_count: Option<Option<NonNegativeInt>>,
    pub cause: serde_json::Value,
    pub command: TrimmedNonEmptyString,
    pub cwd: TrimmedNonEmptyString,
    pub operation: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum VcsProcessSpawnErrorTag {
    #[default]
    #[serde(rename = "VcsProcessSpawnError")]
    VcsProcessSpawnError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsProcessStdinWriteError {
    #[serde(rename = "_tag")]
    pub tag: VcsProcessStdinWriteErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "argumentCount")]
    pub argument_count: Option<Option<NonNegativeInt>>,
    pub cause: serde_json::Value,
    pub command: TrimmedNonEmptyString,
    pub cwd: TrimmedNonEmptyString,
    pub operation: TrimmedNonEmptyString,
    #[serde(rename = "stdinBytes")]
    pub stdin_bytes: NonNegativeInt,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum VcsProcessStdinWriteErrorTag {
    #[default]
    #[serde(rename = "VcsProcessStdinWriteError")]
    VcsProcessStdinWriteError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsProcessTimeoutError {
    #[serde(rename = "_tag")]
    pub tag: VcsProcessTimeoutErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "argumentCount")]
    pub argument_count: Option<Option<NonNegativeInt>>,
    pub command: TrimmedNonEmptyString,
    pub cwd: TrimmedNonEmptyString,
    pub operation: TrimmedNonEmptyString,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: DurationMillis,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum VcsProcessTimeoutErrorTag {
    #[default]
    #[serde(rename = "VcsProcessTimeoutError")]
    VcsProcessTimeoutError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsPullInput {
    pub cwd: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsPullResult {
    #[serde(rename = "refName")]
    pub ref_name: TrimmedNonEmptyString,
    pub status: VcsPullResultStatus,
    #[serde(rename = "upstreamRef")]
    pub upstream_ref: Option<TrimmedNonEmptyString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VcsPullResultStatus {
    #[serde(rename = "pulled")]
    Pulled,
    #[serde(rename = "skipped_up_to_date")]
    SkippedUpToDate,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsRef {
    pub current: bool,
    #[serde(rename = "isDefault")]
    pub is_default: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "isRemote")]
    pub is_remote: Option<Option<bool>>,
    pub name: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "remoteName")]
    pub remote_name: Option<Option<TrimmedNonEmptyString>>,
    #[serde(rename = "worktreePath")]
    pub worktree_path: Option<TrimmedNonEmptyString>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsRemoveWorktreeInput {
    pub cwd: TrimmedNonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub force: Option<Option<bool>>,
    pub path: TrimmedNonEmptyString,
}

pub type VcsRemoveWorktreeSuccess = ();

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsRepositoryDetectionError {
    #[serde(rename = "_tag")]
    pub tag: VcsRepositoryDetectionErrorTag,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    pub cause: Option<Option<serde_json::Value>>,
    pub cwd: TrimmedNonEmptyString,
    pub detail: TrimmedNonEmptyString,
    pub operation: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum VcsRepositoryDetectionErrorTag {
    #[default]
    #[serde(rename = "VcsRepositoryDetectionError")]
    VcsRepositoryDetectionError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsStatusInput {
    pub cwd: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsStatusLocalResult {
    #[serde(rename = "hasPrimaryRemote")]
    pub has_primary_remote: bool,
    #[serde(rename = "hasWorkingTreeChanges")]
    pub has_working_tree_changes: bool,
    #[serde(rename = "isDefaultRef")]
    pub is_default_ref: bool,
    #[serde(rename = "isRepo")]
    pub is_repo: bool,
    #[serde(rename = "refName")]
    pub ref_name: Option<TrimmedNonEmptyString>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "sourceControlProvider")]
    pub source_control_provider: Option<Option<SourceControlProviderInfo>>,
    #[serde(rename = "workingTree")]
    pub working_tree: VcsStatusLocalResultWorkingTree,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsStatusLocalResultWorkingTreeFiles {
    pub deletions: NonNegativeInt,
    pub insertions: NonNegativeInt,
    pub path: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsStatusLocalResultWorkingTree {
    pub deletions: NonNegativeInt,
    pub files: Vec<VcsStatusLocalResultWorkingTreeFiles>,
    pub insertions: NonNegativeInt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsStatusRemoteResult {
    #[serde(rename = "aheadCount")]
    pub ahead_count: NonNegativeInt,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "aheadOfDefaultCount")]
    pub ahead_of_default_count: Option<Option<NonNegativeInt>>,
    #[serde(rename = "behindCount")]
    pub behind_count: NonNegativeInt,
    #[serde(rename = "hasUpstream")]
    pub has_upstream: bool,
    pub pr: Option<VcsStatusRemoteResultPr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VcsStatusRemoteResultPrState {
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "closed")]
    Closed,
    #[serde(rename = "merged")]
    Merged,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsStatusRemoteResultPr {
    #[serde(rename = "baseRef")]
    pub base_ref: TrimmedNonEmptyString,
    #[serde(rename = "headRef")]
    pub head_ref: TrimmedNonEmptyString,
    pub number: PositiveInt,
    pub state: VcsStatusRemoteResultPrState,
    pub title: TrimmedNonEmptyString,
    pub url: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsStatusResult {
    #[serde(rename = "aheadCount")]
    pub ahead_count: NonNegativeInt,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "aheadOfDefaultCount")]
    pub ahead_of_default_count: Option<Option<NonNegativeInt>>,
    #[serde(rename = "behindCount")]
    pub behind_count: NonNegativeInt,
    #[serde(rename = "hasPrimaryRemote")]
    pub has_primary_remote: bool,
    #[serde(rename = "hasUpstream")]
    pub has_upstream: bool,
    #[serde(rename = "hasWorkingTreeChanges")]
    pub has_working_tree_changes: bool,
    #[serde(rename = "isDefaultRef")]
    pub is_default_ref: bool,
    #[serde(rename = "isRepo")]
    pub is_repo: bool,
    pub pr: Option<VcsStatusResultPr>,
    #[serde(rename = "refName")]
    pub ref_name: Option<TrimmedNonEmptyString>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::support::double_option"
    )]
    #[serde(rename = "sourceControlProvider")]
    pub source_control_provider: Option<Option<SourceControlProviderInfo>>,
    #[serde(rename = "workingTree")]
    pub working_tree: VcsStatusResultWorkingTree,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VcsStatusResultPrState {
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "closed")]
    Closed,
    #[serde(rename = "merged")]
    Merged,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsStatusResultPr {
    #[serde(rename = "baseRef")]
    pub base_ref: TrimmedNonEmptyString,
    #[serde(rename = "headRef")]
    pub head_ref: TrimmedNonEmptyString,
    pub number: PositiveInt,
    pub state: VcsStatusResultPrState,
    pub title: TrimmedNonEmptyString,
    pub url: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsStatusResultWorkingTreeFiles {
    pub deletions: NonNegativeInt,
    pub insertions: NonNegativeInt,
    pub path: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsStatusResultWorkingTree {
    pub deletions: NonNegativeInt,
    pub files: Vec<VcsStatusResultWorkingTreeFiles>,
    pub insertions: NonNegativeInt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "_tag")]
pub enum VcsStatusStreamEvent {
    #[serde(rename = "snapshot")]
    Snapshot {
        local: VcsStatusLocalResult,
        remote: Option<VcsStatusRemoteResult>,
    },
    #[serde(rename = "localUpdated")]
    LocalUpdated { local: VcsStatusLocalResult },
    #[serde(rename = "remoteUpdated")]
    RemoteUpdated {
        remote: Option<VcsStatusRemoteResult>,
    },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsSwitchRefInput {
    pub cwd: TrimmedNonEmptyString,
    #[serde(rename = "refName")]
    pub ref_name: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsSwitchRefResult {
    #[serde(rename = "refName")]
    pub ref_name: Option<TrimmedNonEmptyString>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcsUnsupportedOperationError {
    #[serde(rename = "_tag")]
    pub tag: VcsUnsupportedOperationErrorTag,
    pub detail: TrimmedNonEmptyString,
    pub kind: VcsDriverKind,
    pub operation: TrimmedNonEmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum VcsUnsupportedOperationErrorTag {
    #[default]
    #[serde(rename = "VcsUnsupportedOperationError")]
    VcsUnsupportedOperationError,
    /// Forward compatibility: a literal this build does not know.
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum WorkspaceRootRef {
    #[serde(rename = "project")]
    Project {
        #[serde(rename = "projectId")]
        project_id: ProjectId,
    },
    #[serde(rename = "path")]
    Path { path: TrimmedNonEmptyString },
    /// Forward compatibility: an event kind this build does not know.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

/// Typed table of every WebSocket RPC method (see `RpcMethod`).
pub mod methods {
    use crate::support::RpcMethod;
    use serde::{Deserialize, Serialize};

    pub struct AssetsCreateUrl;

    impl RpcMethod for AssetsCreateUrl {
        const TAG: &'static str = "assets.createUrl";
        const STREAM: bool = false;
        type Payload = super::AssetCreateUrlInput;
        type Success = super::AssetCreateUrlResult;
        type Error = AssetsCreateUrlError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum AssetsCreateUrlError {
        AssetAccessError(super::AssetAccessError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ClaudeGetBinaryStatus;

    impl RpcMethod for ClaudeGetBinaryStatus {
        const TAG: &'static str = "claude.getBinaryStatus";
        const STREAM: bool = false;
        type Payload = super::ClaudeGetBinaryStatusPayload;
        type Success = super::ClaudeBinaryStatusSchema;
        type Error = ClaudeGetBinaryStatusError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ClaudeGetBinaryStatusError {
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ClaudeInstallBinary;

    impl RpcMethod for ClaudeInstallBinary {
        const TAG: &'static str = "claude.installBinary";
        const STREAM: bool = true;
        type Payload = super::ClaudeInstallBinaryPayload;
        type Success = super::ClaudeBinaryInstallProgressEventSchema;
        type Error = ClaudeInstallBinaryError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ClaudeInstallBinaryError {
        ClaudeBinaryInstallFailedError(super::ClaudeBinaryInstallFailedError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct CloudGetRelayClientStatus;

    impl RpcMethod for CloudGetRelayClientStatus {
        const TAG: &'static str = "cloud.getRelayClientStatus";
        const STREAM: bool = false;
        type Payload = super::CloudGetRelayClientStatusPayload;
        type Success = super::RelayClientStatusSchema;
        type Error = CloudGetRelayClientStatusError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum CloudGetRelayClientStatusError {
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct CloudInstallRelayClient;

    impl RpcMethod for CloudInstallRelayClient {
        const TAG: &'static str = "cloud.installRelayClient";
        const STREAM: bool = true;
        type Payload = super::CloudInstallRelayClientPayload;
        type Success = super::RelayClientInstallProgressEventSchema;
        type Error = CloudInstallRelayClientError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum CloudInstallRelayClientError {
        RelayClientInstallFailedError(super::RelayClientInstallFailedError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct FilesystemBrowse;

    impl RpcMethod for FilesystemBrowse {
        const TAG: &'static str = "filesystem.browse";
        const STREAM: bool = false;
        type Payload = super::FilesystemBrowseInput;
        type Success = super::FilesystemBrowseResult;
        type Error = FilesystemBrowseErrorX;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum FilesystemBrowseErrorX {
        FilesystemBrowseError(super::FilesystemBrowseError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct GitPreparePullRequestThread;

    impl RpcMethod for GitPreparePullRequestThread {
        const TAG: &'static str = "git.preparePullRequestThread";
        const STREAM: bool = false;
        type Payload = super::GitPreparePullRequestThreadInput;
        type Success = super::GitPreparePullRequestThreadResult;
        type Error = GitPreparePullRequestThreadError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum GitPreparePullRequestThreadError {
        GitManagerServiceError(super::GitManagerServiceError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct GitResolvePullRequest;

    impl RpcMethod for GitResolvePullRequest {
        const TAG: &'static str = "git.resolvePullRequest";
        const STREAM: bool = false;
        type Payload = super::GitPullRequestRefInput;
        type Success = super::GitResolvePullRequestResult;
        type Error = GitResolvePullRequestError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum GitResolvePullRequestError {
        GitManagerServiceError(super::GitManagerServiceError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct GitRunStackedAction;

    impl RpcMethod for GitRunStackedAction {
        const TAG: &'static str = "git.runStackedAction";
        const STREAM: bool = true;
        type Payload = super::GitRunStackedActionInput;
        type Success = super::GitActionProgressEvent;
        type Error = GitRunStackedActionError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum GitRunStackedActionError {
        GitManagerServiceError(super::GitManagerServiceError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct GraphBuild;

    impl RpcMethod for GraphBuild {
        const TAG: &'static str = "graph.build";
        const STREAM: bool = false;
        type Payload = super::GraphBuildInput;
        type Success = super::GraphBuildStatus;
        type Error = GraphBuildError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum GraphBuildError {
        GraphDisabledError(super::GraphDisabledError),
        GraphWorkspaceUnknownError(super::GraphWorkspaceUnknownError),
        GraphStorePathError(super::GraphStorePathError),
        ServerSettingsError(super::ServerSettingsError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        GraphRuntimeUnavailableError(super::GraphRuntimeUnavailableError),
        GraphCommandFailedError(super::GraphCommandFailedError),
        Unknown(serde_json::Value),
    }

    pub struct GraphExplain;

    impl RpcMethod for GraphExplain {
        const TAG: &'static str = "graph.explain";
        const STREAM: bool = false;
        type Payload = super::GraphNodeQueryInput;
        type Success = super::GraphExplanation;
        type Error = GraphExplainError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum GraphExplainError {
        GraphDisabledError(super::GraphDisabledError),
        GraphWorkspaceUnknownError(super::GraphWorkspaceUnknownError),
        GraphStorePathError(super::GraphStorePathError),
        ServerSettingsError(super::ServerSettingsError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        GraphNotBuiltError(super::GraphNotBuiltError),
        GraphNodeNotFoundError(super::GraphNodeNotFoundError),
        Unknown(serde_json::Value),
    }

    pub struct GraphInstallRuntime;

    impl RpcMethod for GraphInstallRuntime {
        const TAG: &'static str = "graph.installRuntime";
        const STREAM: bool = true;
        type Payload = super::GraphInstallRuntimeInput;
        type Success = super::GraphInstallEvent;
        type Error = GraphInstallRuntimeError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum GraphInstallRuntimeError {
        GraphDisabledError(super::GraphDisabledError),
        GraphRuntimeUnavailableError(super::GraphRuntimeUnavailableError),
        GraphCommandFailedError(super::GraphCommandFailedError),
        ServerSettingsError(super::ServerSettingsError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct GraphPath;

    impl RpcMethod for GraphPath {
        const TAG: &'static str = "graph.path";
        const STREAM: bool = false;
        type Payload = super::GraphPathInput;
        type Success = super::GraphPathResult;
        type Error = GraphPathError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum GraphPathError {
        GraphDisabledError(super::GraphDisabledError),
        GraphWorkspaceUnknownError(super::GraphWorkspaceUnknownError),
        GraphStorePathError(super::GraphStorePathError),
        ServerSettingsError(super::ServerSettingsError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        GraphNotBuiltError(super::GraphNotBuiltError),
        GraphNodeNotFoundError(super::GraphNodeNotFoundError),
        Unknown(serde_json::Value),
    }

    pub struct GraphQuery;

    impl RpcMethod for GraphQuery {
        const TAG: &'static str = "graph.query";
        const STREAM: bool = false;
        type Payload = super::GraphQueryInput;
        type Success = super::GraphSearchResult;
        type Error = GraphQueryError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum GraphQueryError {
        GraphDisabledError(super::GraphDisabledError),
        GraphWorkspaceUnknownError(super::GraphWorkspaceUnknownError),
        GraphStorePathError(super::GraphStorePathError),
        ServerSettingsError(super::ServerSettingsError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        GraphNotBuiltError(super::GraphNotBuiltError),
        Unknown(serde_json::Value),
    }

    pub struct GraphRuntimeStatus;

    impl RpcMethod for GraphRuntimeStatus {
        const TAG: &'static str = "graph.runtimeStatus";
        const STREAM: bool = false;
        type Payload = super::GraphRuntimeStatusPayload;
        type Success = super::GraphRuntimeStatus;
        type Error = GraphRuntimeStatusError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum GraphRuntimeStatusError {
        ServerSettingsError(super::ServerSettingsError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct GraphSnapshot;

    impl RpcMethod for GraphSnapshot {
        const TAG: &'static str = "graph.snapshot";
        const STREAM: bool = false;
        type Payload = super::GraphWorkspaceInput;
        type Success = super::GraphSnapshot;
        type Error = GraphSnapshotError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum GraphSnapshotError {
        GraphDisabledError(super::GraphDisabledError),
        GraphWorkspaceUnknownError(super::GraphWorkspaceUnknownError),
        GraphStorePathError(super::GraphStorePathError),
        ServerSettingsError(super::ServerSettingsError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        GraphNotBuiltError(super::GraphNotBuiltError),
        Unknown(serde_json::Value),
    }

    pub struct GraphStatus;

    impl RpcMethod for GraphStatus {
        const TAG: &'static str = "graph.status";
        const STREAM: bool = false;
        type Payload = super::GraphWorkspaceInput;
        type Success = super::GraphStatus;
        type Error = GraphStatusError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum GraphStatusError {
        GraphDisabledError(super::GraphDisabledError),
        GraphWorkspaceUnknownError(super::GraphWorkspaceUnknownError),
        GraphStorePathError(super::GraphStorePathError),
        ServerSettingsError(super::ServerSettingsError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct GraphSubgraph;

    impl RpcMethod for GraphSubgraph {
        const TAG: &'static str = "graph.subgraph";
        const STREAM: bool = false;
        type Payload = super::GraphSubgraphInput;
        type Success = super::GraphSubgraph;
        type Error = GraphSubgraphError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum GraphSubgraphError {
        GraphDisabledError(super::GraphDisabledError),
        GraphWorkspaceUnknownError(super::GraphWorkspaceUnknownError),
        GraphStorePathError(super::GraphStorePathError),
        ServerSettingsError(super::ServerSettingsError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        GraphNotBuiltError(super::GraphNotBuiltError),
        Unknown(serde_json::Value),
    }

    pub struct LspCompletion;

    impl RpcMethod for LspCompletion {
        const TAG: &'static str = "lsp.completion";
        const STREAM: bool = false;
        type Payload = super::LspPositionInput;
        type Success = super::LspCompletionResult;
        type Error = LspCompletionError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum LspCompletionError {
        LspError(super::LspError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct LspDefinition;

    impl RpcMethod for LspDefinition {
        const TAG: &'static str = "lsp.definition";
        const STREAM: bool = false;
        type Payload = super::LspPositionInput;
        type Success = super::LspLocationsResult;
        type Error = LspDefinitionError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum LspDefinitionError {
        LspError(super::LspError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct LspDidChange;

    impl RpcMethod for LspDidChange {
        const TAG: &'static str = "lsp.didChange";
        const STREAM: bool = false;
        type Payload = super::LspDidChangeInput;
        type Success = super::LspDidChangeSuccess;
        type Error = LspDidChangeError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum LspDidChangeError {
        LspError(super::LspError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct LspDidClose;

    impl RpcMethod for LspDidClose {
        const TAG: &'static str = "lsp.didClose";
        const STREAM: bool = false;
        type Payload = super::LspDocumentInput;
        type Success = super::LspDidCloseSuccess;
        type Error = LspDidCloseError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum LspDidCloseError {
        LspError(super::LspError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct LspDidOpen;

    impl RpcMethod for LspDidOpen {
        const TAG: &'static str = "lsp.didOpen";
        const STREAM: bool = false;
        type Payload = super::LspDidOpenInput;
        type Success = super::LspDidOpenSuccess;
        type Error = LspDidOpenError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum LspDidOpenError {
        LspError(super::LspError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct LspFormat;

    impl RpcMethod for LspFormat {
        const TAG: &'static str = "lsp.format";
        const STREAM: bool = false;
        type Payload = super::LspFormattingInput;
        type Success = super::LspFormattingResult;
        type Error = LspFormatError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum LspFormatError {
        LspError(super::LspError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct LspHover;

    impl RpcMethod for LspHover {
        const TAG: &'static str = "lsp.hover";
        const STREAM: bool = false;
        type Payload = super::LspPositionInput;
        type Success = super::LspHoverResult;
        type Error = LspHoverError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum LspHoverError {
        LspError(super::LspError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct LspReferences;

    impl RpcMethod for LspReferences {
        const TAG: &'static str = "lsp.references";
        const STREAM: bool = false;
        type Payload = super::LspPositionInput;
        type Success = super::LspLocationsResult;
        type Error = LspReferencesError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum LspReferencesError {
        LspError(super::LspError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct LspRename;

    impl RpcMethod for LspRename {
        const TAG: &'static str = "lsp.rename";
        const STREAM: bool = false;
        type Payload = super::LspRenameInput;
        type Success = super::LspWorkspaceEditResult;
        type Error = LspRenameError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum LspRenameError {
        LspError(super::LspError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct LspResolveCompletion;

    impl RpcMethod for LspResolveCompletion {
        const TAG: &'static str = "lsp.resolveCompletion";
        const STREAM: bool = false;
        type Payload = super::LspResolveCompletionInput;
        type Success = super::LspCompletionItem;
        type Error = LspResolveCompletionError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum LspResolveCompletionError {
        LspError(super::LspError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct LspServerStatus;

    impl RpcMethod for LspServerStatus {
        const TAG: &'static str = "lsp.serverStatus";
        const STREAM: bool = false;
        type Payload = super::LspServerStatusPayload;
        type Success = super::LspServerStatusResult;
        type Error = LspServerStatusError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum LspServerStatusError {
        LspError(super::LspError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct LspSignatureHelp;

    impl RpcMethod for LspSignatureHelp {
        const TAG: &'static str = "lsp.signatureHelp";
        const STREAM: bool = false;
        type Payload = super::LspPositionInput;
        type Success = super::LspSignatureHelpResult;
        type Error = LspSignatureHelpError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum LspSignatureHelpError {
        LspError(super::LspError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct LspSubscribeDiagnostics;

    impl RpcMethod for LspSubscribeDiagnostics {
        const TAG: &'static str = "lsp.subscribeDiagnostics";
        const STREAM: bool = true;
        type Payload = super::LspSubscribeDiagnosticsInput;
        type Success = super::LspDiagnosticsStreamEvent;
        type Error = LspSubscribeDiagnosticsError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum LspSubscribeDiagnosticsError {
        LspError(super::LspError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct OrchestrationDispatchCommand;

    impl RpcMethod for OrchestrationDispatchCommand {
        const TAG: &'static str = "orchestration.dispatchCommand";
        const STREAM: bool = false;
        type Payload = super::ClientOrchestrationCommand;
        type Success = super::DispatchResult;
        type Error = OrchestrationDispatchCommandErrorX;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum OrchestrationDispatchCommandErrorX {
        OrchestrationDispatchCommandError(super::OrchestrationDispatchCommandError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct OrchestrationGetArchivedShellSnapshot;

    impl RpcMethod for OrchestrationGetArchivedShellSnapshot {
        const TAG: &'static str = "orchestration.getArchivedShellSnapshot";
        const STREAM: bool = false;
        type Payload = super::OrchestrationGetArchivedShellSnapshotPayload;
        type Success = super::OrchestrationShellSnapshot;
        type Error = OrchestrationGetArchivedShellSnapshotError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum OrchestrationGetArchivedShellSnapshotError {
        OrchestrationGetSnapshotError(super::OrchestrationGetSnapshotError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct OrchestrationGetFullThreadDiff;

    impl RpcMethod for OrchestrationGetFullThreadDiff {
        const TAG: &'static str = "orchestration.getFullThreadDiff";
        const STREAM: bool = false;
        type Payload = super::OrchestrationGetFullThreadDiffInput;
        type Success = super::OrchestrationTurnDiffRange1;
        type Error = OrchestrationGetFullThreadDiffErrorX;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum OrchestrationGetFullThreadDiffErrorX {
        OrchestrationGetFullThreadDiffError(super::OrchestrationGetFullThreadDiffError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct OrchestrationGetTurnDiff;

    impl RpcMethod for OrchestrationGetTurnDiff {
        const TAG: &'static str = "orchestration.getTurnDiff";
        const STREAM: bool = false;
        type Payload = super::OrchestrationTurnDiffRange;
        type Success = super::OrchestrationTurnDiffRange1;
        type Error = OrchestrationGetTurnDiffErrorX;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum OrchestrationGetTurnDiffErrorX {
        OrchestrationGetTurnDiffError(super::OrchestrationGetTurnDiffError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct OrchestrationReplayEvents;

    impl RpcMethod for OrchestrationReplayEvents {
        const TAG: &'static str = "orchestration.replayEvents";
        const STREAM: bool = false;
        type Payload = super::OrchestrationReplayEventsInput;
        type Success = super::OrchestrationReplayEventsSuccess;
        type Error = OrchestrationReplayEventsErrorX;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum OrchestrationReplayEventsErrorX {
        OrchestrationReplayEventsError(super::OrchestrationReplayEventsError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct OrchestrationSearchMessages;

    impl RpcMethod for OrchestrationSearchMessages {
        const TAG: &'static str = "orchestration.searchMessages";
        const STREAM: bool = false;
        type Payload = super::OrchestrationSearchMessagesInput;
        type Success = super::OrchestrationSearchMessagesResult;
        type Error = OrchestrationSearchMessagesErrorX;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum OrchestrationSearchMessagesErrorX {
        OrchestrationSearchMessagesError(super::OrchestrationSearchMessagesError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct OrchestrationSubscribeShell;

    impl RpcMethod for OrchestrationSubscribeShell {
        const TAG: &'static str = "orchestration.subscribeShell";
        const STREAM: bool = true;
        type Payload = super::OrchestrationSubscribeShellInput;
        type Success = super::OrchestrationShellStreamItem;
        type Error = OrchestrationSubscribeShellError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum OrchestrationSubscribeShellError {
        OrchestrationGetSnapshotError(super::OrchestrationGetSnapshotError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct OrchestrationSubscribeThread;

    impl RpcMethod for OrchestrationSubscribeThread {
        const TAG: &'static str = "orchestration.subscribeThread";
        const STREAM: bool = true;
        type Payload = super::OrchestrationSubscribeThreadInput;
        type Success = super::OrchestrationThreadStreamItem;
        type Error = OrchestrationSubscribeThreadError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum OrchestrationSubscribeThreadError {
        OrchestrationGetSnapshotError(super::OrchestrationGetSnapshotError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct PreviewClose;

    impl RpcMethod for PreviewClose {
        const TAG: &'static str = "preview.close";
        const STREAM: bool = false;
        type Payload = super::PreviewCloseInput;
        type Success = super::PreviewCloseSuccess;
        type Error = PreviewCloseError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum PreviewCloseError {
        PreviewError(super::PreviewError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct PreviewList;

    impl RpcMethod for PreviewList {
        const TAG: &'static str = "preview.list";
        const STREAM: bool = false;
        type Payload = super::PreviewListInput;
        type Success = super::PreviewListResult;
        type Error = PreviewListError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum PreviewListError {
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct PreviewNavigate;

    impl RpcMethod for PreviewNavigate {
        const TAG: &'static str = "preview.navigate";
        const STREAM: bool = false;
        type Payload = super::PreviewNavigateInput;
        type Success = super::PreviewSessionSnapshot;
        type Error = PreviewNavigateError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum PreviewNavigateError {
        PreviewError(super::PreviewError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct PreviewOpen;

    impl RpcMethod for PreviewOpen {
        const TAG: &'static str = "preview.open";
        const STREAM: bool = false;
        type Payload = super::PreviewOpenInput;
        type Success = super::PreviewSessionSnapshot;
        type Error = PreviewOpenError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum PreviewOpenError {
        PreviewError(super::PreviewError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct PreviewRefresh;

    impl RpcMethod for PreviewRefresh {
        const TAG: &'static str = "preview.refresh";
        const STREAM: bool = false;
        type Payload = super::PreviewRefreshInput;
        type Success = super::PreviewRefreshSuccess;
        type Error = PreviewRefreshError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum PreviewRefreshError {
        PreviewError(super::PreviewError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct PreviewReportStatus;

    impl RpcMethod for PreviewReportStatus {
        const TAG: &'static str = "preview.reportStatus";
        const STREAM: bool = false;
        type Payload = super::PreviewReportStatusInput;
        type Success = super::PreviewReportStatusSuccess;
        type Error = PreviewReportStatusError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum PreviewReportStatusError {
        PreviewError(super::PreviewError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct PreviewResize;

    impl RpcMethod for PreviewResize {
        const TAG: &'static str = "preview.resize";
        const STREAM: bool = false;
        type Payload = super::PreviewResizeInput;
        type Success = super::PreviewSessionSnapshot;
        type Error = PreviewResizeError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum PreviewResizeError {
        PreviewError(super::PreviewError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct PreviewAutomationConnect;

    impl RpcMethod for PreviewAutomationConnect {
        const TAG: &'static str = "previewAutomation.connect";
        const STREAM: bool = true;
        type Payload = super::PreviewAutomationHost;
        type Success = super::PreviewAutomationStreamEvent;
        type Error = PreviewAutomationConnectError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum PreviewAutomationConnectError {
        PreviewAutomationError(super::PreviewAutomationError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct PreviewAutomationFocusHost;

    impl RpcMethod for PreviewAutomationFocusHost {
        const TAG: &'static str = "previewAutomation.focusHost";
        const STREAM: bool = false;
        type Payload = super::PreviewAutomationHostFocus;
        type Success = super::PreviewAutomationFocusHostSuccess;
        type Error = PreviewAutomationFocusHostError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum PreviewAutomationFocusHostError {
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct PreviewAutomationRespond;

    impl RpcMethod for PreviewAutomationRespond {
        const TAG: &'static str = "previewAutomation.respond";
        const STREAM: bool = false;
        type Payload = super::PreviewAutomationResponse;
        type Success = super::PreviewAutomationRespondSuccess;
        type Error = PreviewAutomationRespondError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum PreviewAutomationRespondError {
        PreviewAutomationError(super::PreviewAutomationError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ProjectsCopyEntry;

    impl RpcMethod for ProjectsCopyEntry {
        const TAG: &'static str = "projects.copyEntry";
        const STREAM: bool = false;
        type Payload = super::ProjectCopyEntryInput;
        type Success = super::ProjectCopyEntryResult;
        type Error = ProjectsCopyEntryError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ProjectsCopyEntryError {
        ProjectCopyEntryError(super::ProjectCopyEntryError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ProjectsListEntries;

    impl RpcMethod for ProjectsListEntries {
        const TAG: &'static str = "projects.listEntries";
        const STREAM: bool = false;
        type Payload = super::ProjectListEntriesInput;
        type Success = super::ProjectListEntriesResult;
        type Error = ProjectsListEntriesError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ProjectsListEntriesError {
        ProjectListEntriesError(super::ProjectListEntriesError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ProjectsMutateEntry;

    impl RpcMethod for ProjectsMutateEntry {
        const TAG: &'static str = "projects.mutateEntry";
        const STREAM: bool = false;
        type Payload = super::ProjectMutateEntryInput;
        type Success = super::ProjectMutateEntryResult;
        type Error = ProjectsMutateEntryError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ProjectsMutateEntryError {
        ProjectMutateEntryError(super::ProjectMutateEntryError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ProjectsReadFile;

    impl RpcMethod for ProjectsReadFile {
        const TAG: &'static str = "projects.readFile";
        const STREAM: bool = false;
        type Payload = super::ProjectReadFileInput;
        type Success = super::ProjectReadFileResult;
        type Error = ProjectsReadFileError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ProjectsReadFileError {
        ProjectReadFileError(super::ProjectReadFileError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ProjectsSearchContent;

    impl RpcMethod for ProjectsSearchContent {
        const TAG: &'static str = "projects.searchContent";
        const STREAM: bool = false;
        type Payload = super::ProjectSearchContentInput;
        type Success = super::ProjectSearchContentResult;
        type Error = ProjectsSearchContentError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ProjectsSearchContentError {
        ProjectSearchContentError(super::ProjectSearchContentError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ProjectsSearchEntries;

    impl RpcMethod for ProjectsSearchEntries {
        const TAG: &'static str = "projects.searchEntries";
        const STREAM: bool = false;
        type Payload = super::ProjectSearchEntriesInput;
        type Success = super::ProjectSearchEntriesResult;
        type Error = ProjectsSearchEntriesError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ProjectsSearchEntriesError {
        ProjectSearchEntriesError(super::ProjectSearchEntriesError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ProjectsSubscribeWorkspaceChanges;

    impl RpcMethod for ProjectsSubscribeWorkspaceChanges {
        const TAG: &'static str = "projects.subscribeWorkspaceChanges";
        const STREAM: bool = true;
        type Payload = super::ProjectWatchInput;
        type Success = super::ProjectWatchStreamEvent;
        type Error = ProjectsSubscribeWorkspaceChangesError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ProjectsSubscribeWorkspaceChangesError {
        ProjectWatchError(super::ProjectWatchError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ProjectsWriteFile;

    impl RpcMethod for ProjectsWriteFile {
        const TAG: &'static str = "projects.writeFile";
        const STREAM: bool = false;
        type Payload = super::ProjectWriteFileInput;
        type Success = super::ProjectWriteFileResult;
        type Error = ProjectsWriteFileError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ProjectsWriteFileError {
        ProjectWriteFileError(super::ProjectWriteFileError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ReviewGetDiffPreview;

    impl RpcMethod for ReviewGetDiffPreview {
        const TAG: &'static str = "review.getDiffPreview";
        const STREAM: bool = false;
        type Payload = super::ReviewDiffPreviewInput;
        type Success = super::ReviewDiffPreviewResult;
        type Error = ReviewGetDiffPreviewError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ReviewGetDiffPreviewError {
        ReviewDiffPreviewError(super::ReviewDiffPreviewError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ServerDiscoverSourceControl;

    impl RpcMethod for ServerDiscoverSourceControl {
        const TAG: &'static str = "server.discoverSourceControl";
        const STREAM: bool = false;
        type Payload = super::ServerDiscoverSourceControlPayload;
        type Success = super::SourceControlDiscoveryResult;
        type Error = ServerDiscoverSourceControlError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ServerDiscoverSourceControlError {
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ServerGetConfig;

    impl RpcMethod for ServerGetConfig {
        const TAG: &'static str = "server.getConfig";
        const STREAM: bool = false;
        type Payload = super::ServerGetConfigPayload;
        type Success = super::ServerConfig;
        type Error = ServerGetConfigError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ServerGetConfigError {
        KeybindingsConfigParseError(super::KeybindingsConfigParseError),
        ServerSettingsError(super::ServerSettingsError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ServerGetProcessDiagnostics;

    impl RpcMethod for ServerGetProcessDiagnostics {
        const TAG: &'static str = "server.getProcessDiagnostics";
        const STREAM: bool = false;
        type Payload = super::ServerGetProcessDiagnosticsPayload;
        type Success = super::ServerProcessDiagnosticsResult;
        type Error = ServerGetProcessDiagnosticsError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ServerGetProcessDiagnosticsError {
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ServerGetProcessResourceHistory;

    impl RpcMethod for ServerGetProcessResourceHistory {
        const TAG: &'static str = "server.getProcessResourceHistory";
        const STREAM: bool = false;
        type Payload = super::ServerProcessResourceHistoryInput;
        type Success = super::ServerProcessResourceHistoryResult;
        type Error = ServerGetProcessResourceHistoryError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ServerGetProcessResourceHistoryError {
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ServerGetSettings;

    impl RpcMethod for ServerGetSettings {
        const TAG: &'static str = "server.getSettings";
        const STREAM: bool = false;
        type Payload = super::ServerGetSettingsPayload;
        type Success = super::ServerSettings;
        type Error = ServerGetSettingsError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ServerGetSettingsError {
        ServerSettingsError(super::ServerSettingsError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ServerGetTraceDiagnostics;

    impl RpcMethod for ServerGetTraceDiagnostics {
        const TAG: &'static str = "server.getTraceDiagnostics";
        const STREAM: bool = false;
        type Payload = super::ServerGetTraceDiagnosticsPayload;
        type Success = super::ServerTraceDiagnosticsResult;
        type Error = ServerGetTraceDiagnosticsError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ServerGetTraceDiagnosticsError {
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ServerProbe;

    impl RpcMethod for ServerProbe {
        const TAG: &'static str = "server.probe";
        const STREAM: bool = false;
        type Payload = super::ServerProbePayload;
        type Success = super::ServerProbeSuccess;
        type Error = ServerProbeError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ServerProbeError {
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ServerRefreshProviders;

    impl RpcMethod for ServerRefreshProviders {
        const TAG: &'static str = "server.refreshProviders";
        const STREAM: bool = false;
        type Payload = super::ServerRefreshProvidersPayload;
        type Success = super::ServerProviderUpdatedPayload;
        type Error = ServerRefreshProvidersError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ServerRefreshProvidersError {
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ServerRemoveKeybinding;

    impl RpcMethod for ServerRemoveKeybinding {
        const TAG: &'static str = "server.removeKeybinding";
        const STREAM: bool = false;
        type Payload = super::ServerRemoveKeybindingInput;
        type Success = super::ServerRemoveKeybindingResult;
        type Error = ServerRemoveKeybindingError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ServerRemoveKeybindingError {
        KeybindingsConfigParseError(super::KeybindingsConfigParseError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ServerSignalProcess;

    impl RpcMethod for ServerSignalProcess {
        const TAG: &'static str = "server.signalProcess";
        const STREAM: bool = false;
        type Payload = super::ServerSignalProcessInput;
        type Success = super::ServerSignalProcessResult;
        type Error = ServerSignalProcessError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ServerSignalProcessError {
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ServerUpdateProvider;

    impl RpcMethod for ServerUpdateProvider {
        const TAG: &'static str = "server.updateProvider";
        const STREAM: bool = false;
        type Payload = super::ServerProviderUpdateInput;
        type Success = super::ServerProviderUpdatedPayload;
        type Error = ServerUpdateProviderError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ServerUpdateProviderError {
        ServerProviderUpdateError(super::ServerProviderUpdateError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ServerUpdateServer;

    impl RpcMethod for ServerUpdateServer {
        const TAG: &'static str = "server.updateServer";
        const STREAM: bool = false;
        type Payload = super::ServerSelfUpdateInput;
        type Success = super::ServerSelfUpdateResult;
        type Error = ServerUpdateServerError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ServerUpdateServerError {
        ServerSelfUpdateError(super::ServerSelfUpdateError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ServerUpdateSettings;

    impl RpcMethod for ServerUpdateSettings {
        const TAG: &'static str = "server.updateSettings";
        const STREAM: bool = false;
        type Payload = super::ServerUpdateSettingsPayload;
        type Success = super::ServerSettings;
        type Error = ServerUpdateSettingsError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ServerUpdateSettingsError {
        ServerSettingsError(super::ServerSettingsError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ServerUpsertKeybinding;

    impl RpcMethod for ServerUpsertKeybinding {
        const TAG: &'static str = "server.upsertKeybinding";
        const STREAM: bool = false;
        type Payload = super::ServerUpsertKeybindingInput;
        type Success = super::ServerRemoveKeybindingResult;
        type Error = ServerUpsertKeybindingError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ServerUpsertKeybindingError {
        KeybindingsConfigParseError(super::KeybindingsConfigParseError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct ShellOpenInEditor;

    impl RpcMethod for ShellOpenInEditor {
        const TAG: &'static str = "shell.openInEditor";
        const STREAM: bool = false;
        type Payload = super::LaunchEditorInput;
        type Success = super::ShellOpenInEditorSuccess;
        type Error = ShellOpenInEditorError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum ShellOpenInEditorError {
        ExternalLauncherError(super::ExternalLauncherError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct SourceControlCloneRepository;

    impl RpcMethod for SourceControlCloneRepository {
        const TAG: &'static str = "sourceControl.cloneRepository";
        const STREAM: bool = false;
        type Payload = super::SourceControlCloneRepositoryInput;
        type Success = super::SourceControlCloneRepositoryResult;
        type Error = SourceControlCloneRepositoryError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum SourceControlCloneRepositoryError {
        SourceControlRepositoryError(super::SourceControlRepositoryError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct SourceControlLookupRepository;

    impl RpcMethod for SourceControlLookupRepository {
        const TAG: &'static str = "sourceControl.lookupRepository";
        const STREAM: bool = false;
        type Payload = super::SourceControlRepositoryLookupInput;
        type Success = super::SourceControlRepositoryInfo;
        type Error = SourceControlLookupRepositoryError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum SourceControlLookupRepositoryError {
        SourceControlRepositoryError(super::SourceControlRepositoryError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct SourceControlPublishRepository;

    impl RpcMethod for SourceControlPublishRepository {
        const TAG: &'static str = "sourceControl.publishRepository";
        const STREAM: bool = false;
        type Payload = super::SourceControlPublishRepositoryInput;
        type Success = super::SourceControlPublishRepositoryResult;
        type Error = SourceControlPublishRepositoryError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum SourceControlPublishRepositoryError {
        SourceControlRepositoryError(super::SourceControlRepositoryError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct SubscribeAuthAccess;

    impl RpcMethod for SubscribeAuthAccess {
        const TAG: &'static str = "subscribeAuthAccess";
        const STREAM: bool = true;
        type Payload = super::SubscribeAuthAccessPayload;
        type Success = super::AuthAccessStreamEvent;
        type Error = SubscribeAuthAccessError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum SubscribeAuthAccessError {
        AuthAccessStreamError(super::AuthAccessStreamError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct SubscribeDiscoveredLocalServers;

    impl RpcMethod for SubscribeDiscoveredLocalServers {
        const TAG: &'static str = "subscribeDiscoveredLocalServers";
        const STREAM: bool = true;
        type Payload = super::SubscribeDiscoveredLocalServersPayload;
        type Success = super::DiscoveredLocalServerList;
        type Error = SubscribeDiscoveredLocalServersError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum SubscribeDiscoveredLocalServersError {
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct SubscribePreviewEvents;

    impl RpcMethod for SubscribePreviewEvents {
        const TAG: &'static str = "subscribePreviewEvents";
        const STREAM: bool = true;
        type Payload = super::SubscribePreviewEventsPayload;
        type Success = super::PreviewEvent;
        type Error = SubscribePreviewEventsError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum SubscribePreviewEventsError {
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct SubscribeServerConfig;

    impl RpcMethod for SubscribeServerConfig {
        const TAG: &'static str = "subscribeServerConfig";
        const STREAM: bool = true;
        type Payload = super::SubscribeServerConfigPayload;
        type Success = super::ServerConfigStreamEvent;
        type Error = SubscribeServerConfigError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum SubscribeServerConfigError {
        KeybindingsConfigParseError(super::KeybindingsConfigParseError),
        ServerSettingsError(super::ServerSettingsError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct SubscribeServerLifecycle;

    impl RpcMethod for SubscribeServerLifecycle {
        const TAG: &'static str = "subscribeServerLifecycle";
        const STREAM: bool = true;
        type Payload = super::SubscribeServerLifecyclePayload;
        type Success = super::ServerLifecycleStreamEvent;
        type Error = SubscribeServerLifecycleError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum SubscribeServerLifecycleError {
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct SubscribeTerminalEvents;

    impl RpcMethod for SubscribeTerminalEvents {
        const TAG: &'static str = "subscribeTerminalEvents";
        const STREAM: bool = true;
        type Payload = super::SubscribeTerminalEventsPayload;
        type Success = super::TerminalEvent;
        type Error = SubscribeTerminalEventsError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum SubscribeTerminalEventsError {
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct SubscribeTerminalMetadata;

    impl RpcMethod for SubscribeTerminalMetadata {
        const TAG: &'static str = "subscribeTerminalMetadata";
        const STREAM: bool = true;
        type Payload = super::SubscribeTerminalMetadataPayload;
        type Success = super::TerminalMetadataStreamEvent;
        type Error = SubscribeTerminalMetadataError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum SubscribeTerminalMetadataError {
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct SubscribeVcsStatus;

    impl RpcMethod for SubscribeVcsStatus {
        const TAG: &'static str = "subscribeVcsStatus";
        const STREAM: bool = true;
        type Payload = super::VcsStatusInput;
        type Success = super::VcsStatusStreamEvent;
        type Error = SubscribeVcsStatusError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum SubscribeVcsStatusError {
        GitManagerServiceError(super::GitManagerServiceError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct TerminalAttach;

    impl RpcMethod for TerminalAttach {
        const TAG: &'static str = "terminal.attach";
        const STREAM: bool = true;
        type Payload = super::TerminalAttachInput;
        type Success = super::TerminalAttachStreamEvent;
        type Error = TerminalAttachError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum TerminalAttachError {
        TerminalError(super::TerminalError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct TerminalClear;

    impl RpcMethod for TerminalClear {
        const TAG: &'static str = "terminal.clear";
        const STREAM: bool = false;
        type Payload = super::TerminalClearInput;
        type Success = super::TerminalClearSuccess;
        type Error = TerminalClearError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum TerminalClearError {
        TerminalError(super::TerminalError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct TerminalClose;

    impl RpcMethod for TerminalClose {
        const TAG: &'static str = "terminal.close";
        const STREAM: bool = false;
        type Payload = super::TerminalCloseInput;
        type Success = super::TerminalCloseSuccess;
        type Error = TerminalCloseError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum TerminalCloseError {
        TerminalError(super::TerminalError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct TerminalOpen;

    impl RpcMethod for TerminalOpen {
        const TAG: &'static str = "terminal.open";
        const STREAM: bool = false;
        type Payload = super::TerminalOpenInput;
        type Success = super::TerminalSessionSnapshot;
        type Error = TerminalOpenError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum TerminalOpenError {
        TerminalError(super::TerminalError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct TerminalResize;

    impl RpcMethod for TerminalResize {
        const TAG: &'static str = "terminal.resize";
        const STREAM: bool = false;
        type Payload = super::TerminalResizeInput;
        type Success = super::TerminalResizeSuccess;
        type Error = TerminalResizeErrorX;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum TerminalResizeErrorX {
        TerminalError(super::TerminalError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct TerminalRestart;

    impl RpcMethod for TerminalRestart {
        const TAG: &'static str = "terminal.restart";
        const STREAM: bool = false;
        type Payload = super::TerminalRestartInput;
        type Success = super::TerminalSessionSnapshot;
        type Error = TerminalRestartError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum TerminalRestartError {
        TerminalError(super::TerminalError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct TerminalWrite;

    impl RpcMethod for TerminalWrite {
        const TAG: &'static str = "terminal.write";
        const STREAM: bool = false;
        type Payload = super::TerminalWriteInput;
        type Success = super::TerminalWriteSuccess;
        type Error = TerminalWriteErrorX;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum TerminalWriteErrorX {
        TerminalError(super::TerminalError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct VcsCreateRef;

    impl RpcMethod for VcsCreateRef {
        const TAG: &'static str = "vcs.createRef";
        const STREAM: bool = false;
        type Payload = super::VcsCreateRefInput;
        type Success = super::VcsCreateRefResult;
        type Error = VcsCreateRefError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum VcsCreateRefError {
        GitCommandError(super::GitCommandError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct VcsCreateWorktree;

    impl RpcMethod for VcsCreateWorktree {
        const TAG: &'static str = "vcs.createWorktree";
        const STREAM: bool = false;
        type Payload = super::VcsCreateWorktreeInput;
        type Success = super::VcsCreateWorktreeResult;
        type Error = VcsCreateWorktreeError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum VcsCreateWorktreeError {
        GitCommandError(super::GitCommandError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct VcsGetFileBaseline;

    impl RpcMethod for VcsGetFileBaseline {
        const TAG: &'static str = "vcs.getFileBaseline";
        const STREAM: bool = false;
        type Payload = super::VcsFileBaselineInput;
        type Success = super::VcsFileBaselineResult;
        type Error = VcsGetFileBaselineError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum VcsGetFileBaselineError {
        GitCommandError(super::GitCommandError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct VcsGetFileStatuses;

    impl RpcMethod for VcsGetFileStatuses {
        const TAG: &'static str = "vcs.getFileStatuses";
        const STREAM: bool = false;
        type Payload = super::VcsFileStatusesInput;
        type Success = super::VcsFileStatusesResult;
        type Error = VcsGetFileStatusesError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum VcsGetFileStatusesError {
        GitCommandError(super::GitCommandError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct VcsInit;

    impl RpcMethod for VcsInit {
        const TAG: &'static str = "vcs.init";
        const STREAM: bool = false;
        type Payload = super::VcsInitInput;
        type Success = super::VcsInitSuccess;
        type Error = VcsInitError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum VcsInitError {
        VcsError(super::VcsError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct VcsListRefs;

    impl RpcMethod for VcsListRefs {
        const TAG: &'static str = "vcs.listRefs";
        const STREAM: bool = false;
        type Payload = super::VcsListRefsInput;
        type Success = super::VcsListRefsResult;
        type Error = VcsListRefsError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum VcsListRefsError {
        GitCommandError(super::GitCommandError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct VcsPull;

    impl RpcMethod for VcsPull {
        const TAG: &'static str = "vcs.pull";
        const STREAM: bool = false;
        type Payload = super::VcsPullInput;
        type Success = super::VcsPullResult;
        type Error = VcsPullError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum VcsPullError {
        GitCommandError(super::GitCommandError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct VcsRefreshStatus;

    impl RpcMethod for VcsRefreshStatus {
        const TAG: &'static str = "vcs.refreshStatus";
        const STREAM: bool = false;
        type Payload = super::VcsStatusInput;
        type Success = super::VcsStatusResult;
        type Error = VcsRefreshStatusError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum VcsRefreshStatusError {
        GitManagerServiceError(super::GitManagerServiceError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct VcsRemoveWorktree;

    impl RpcMethod for VcsRemoveWorktree {
        const TAG: &'static str = "vcs.removeWorktree";
        const STREAM: bool = false;
        type Payload = super::VcsRemoveWorktreeInput;
        type Success = super::VcsRemoveWorktreeSuccess;
        type Error = VcsRemoveWorktreeError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum VcsRemoveWorktreeError {
        GitCommandError(super::GitCommandError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }

    pub struct VcsSwitchRef;

    impl RpcMethod for VcsSwitchRef {
        const TAG: &'static str = "vcs.switchRef";
        const STREAM: bool = false;
        type Payload = super::VcsSwitchRefInput;
        type Success = super::VcsSwitchRefResult;
        type Error = VcsSwitchRefError;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum VcsSwitchRefError {
        GitCommandError(super::GitCommandError),
        EnvironmentAuthorizationError(super::EnvironmentAuthorizationError),
        Unknown(serde_json::Value),
    }
}

#[cfg(test)]
mod fixture_tests {
    use crate::support::assert_fixture_roundtrip;

    #[test]
    fn environment_authorization_error() {
        assert_fixture_roundtrip::<super::EnvironmentAuthorizationError>(include_str!(
            "../fixtures/EnvironmentAuthorizationError.json"
        ));
    }

    #[test]
    fn keybinding_when_node() {
        assert_fixture_roundtrip::<super::KeybindingWhenNode>(include_str!(
            "../fixtures/KeybindingWhenNode.json"
        ));
    }

    #[test]
    fn orchestration_shell_stream_event() {
        assert_fixture_roundtrip::<super::OrchestrationShellStreamEvent>(include_str!(
            "../fixtures/OrchestrationShellStreamEvent.json"
        ));
    }

    #[test]
    fn orchestration_subscribe_shell_input() {
        assert_fixture_roundtrip::<super::OrchestrationSubscribeShellInput>(include_str!(
            "../fixtures/OrchestrationSubscribeShellInput.json"
        ));
    }

    #[test]
    fn orchestration_thread_stream_item() {
        assert_fixture_roundtrip::<super::OrchestrationThreadStreamItem>(include_str!(
            "../fixtures/OrchestrationThreadStreamItem.json"
        ));
    }

    #[test]
    fn orchestration_turn_diff_range() {
        assert_fixture_roundtrip::<super::OrchestrationTurnDiffRange>(include_str!(
            "../fixtures/OrchestrationTurnDiffRange.json"
        ));
    }

    #[test]
    fn resolved_workspace_root() {
        assert_fixture_roundtrip::<super::ResolvedWorkspaceRoot>(include_str!(
            "../fixtures/ResolvedWorkspaceRoot.json"
        ));
    }

    #[test]
    fn server_settings_patch() {
        assert_fixture_roundtrip::<super::ServerSettingsPatch>(include_str!(
            "../fixtures/ServerSettingsPatch.json"
        ));
    }

    #[test]
    fn server_signal_process_result() {
        assert_fixture_roundtrip::<super::ServerSignalProcessResult>(include_str!(
            "../fixtures/ServerSignalProcessResult.json"
        ));
    }

    #[test]
    fn thread_id() {
        assert_fixture_roundtrip::<super::ThreadId>(include_str!("../fixtures/ThreadId.json"));
    }
}
