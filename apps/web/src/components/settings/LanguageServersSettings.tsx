"use client";

import { PencilIcon, PlusIcon, Trash2Icon } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { useAtomValue } from "@effect/atom-react";
import {
  BUILTIN_LANGUAGE_SERVER_EXTENSIONS,
  BUILTIN_LANGUAGE_SERVER_IDS,
  type CustomLanguageServer,
  type LspServerStatus,
} from "@t3tools/contracts";
import * as Option from "effect/Option";
import { AsyncResult } from "effect/unstable/reactivity";

import { usePrimarySettings, useUpdatePrimarySettings } from "../../hooks/useSettings";
import { useProjects } from "../../state/entities";
import { lspEnvironment } from "../../state/lsp";
import type { EnvironmentProject } from "@t3tools/client-runtime/state/models";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import {
  Dialog,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogPopup,
  DialogTitle,
} from "../ui/dialog";
import { Input } from "../ui/input";
import { toastManager } from "../ui/toast";
import { Tooltip, TooltipPopup, TooltipTrigger } from "../ui/tooltip";
import { SettingsPageContainer, SettingsRow, SettingsSection } from "./settingsLayout";

/**
 * Static display metadata for the built-in servers. Mirrors the server
 * registry (apps/server/src/lsp/LanguageServers.ts); built-ins are not
 * editable so this is presentation-only.
 */
const BUILTIN_SERVER_ROWS = [
  {
    serverId: "typescript",
    displayName: "TypeScript / JavaScript",
    detail: "vtsls — bundled with the app",
    extensions: ".ts .tsx .js .jsx .mts .cts .mjs .cjs",
  },
  {
    serverId: "rust",
    displayName: "Rust",
    detail: "rust-analyzer — resolved from PATH",
    extensions: ".rs",
  },
  {
    serverId: "python",
    displayName: "Python",
    detail: "pyright-langserver — resolved from PATH",
    extensions: ".py .pyi",
  },
  {
    serverId: "go",
    displayName: "Go",
    detail: "gopls — resolved from PATH",
    extensions: ".go",
  },
] as const;

const SERVER_ID_PATTERN = /^[a-zA-Z][a-zA-Z0-9_-]*$/;
const EXTENSION_PATTERN = /^\.[a-z0-9+_-]+$/;

const STATUS_BADGES: Record<
  LspServerStatus["state"],
  { label: string; variant: "success" | "info" | "error" | "warning" }
> = {
  running: { label: "Running", variant: "success" },
  starting: { label: "Starting", variant: "info" },
  failed: { label: "Failed", variant: "error" },
  not_installed: { label: "Not installed", variant: "warning" },
};

function slugifyDisplayName(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 64);
}

/** Normalize a raw extension token to leading-dot lowercase ("RB" -> ".rb"). */
function normalizeExtension(raw: string): string {
  const lower = raw.trim().toLowerCase();
  if (lower.length === 0) return lower;
  return lower.startsWith(".") ? lower : `.${lower}`;
}

function parseExtensions(raw: string): string[] {
  return raw
    .split(/[\s,]+/)
    .map(normalizeExtension)
    .filter((extension) => extension.length > 0);
}

function parseArgs(raw: string): string[] {
  return raw.split(/\s+/).filter((arg) => arg.length > 0);
}

interface ServerDraft {
  serverId: string;
  displayName: string;
  command: string;
  args: string;
  extensions: string;
  languageId: string;
}

const EMPTY_DRAFT: ServerDraft = {
  serverId: "",
  displayName: "",
  command: "",
  args: "",
  extensions: "",
  languageId: "",
};

function draftFromServer(server: CustomLanguageServer): ServerDraft {
  return {
    serverId: server.serverId,
    displayName: server.displayName,
    command: server.command,
    args: server.args.join(" "),
    extensions: server.extensions.join(" "),
    languageId: server.languageId,
  };
}

/**
 * Client-side mirror of the `languageServers` schema validation in
 * @t3tools/contracts settings, so the dialog can surface actionable errors
 * before the server rejects the patch.
 */
function validateDraft(
  draft: ServerDraft,
  others: ReadonlyArray<CustomLanguageServer>,
): string | null {
  if (draft.serverId.length === 0) return "Server ID is required.";
  if (draft.serverId.length > 64) return "Server ID must be 64 characters or fewer.";
  if (!SERVER_ID_PATTERN.test(draft.serverId)) {
    return "Server ID must start with a letter and use only letters, digits, '-', or '_'.";
  }
  if (BUILTIN_LANGUAGE_SERVER_IDS.includes(draft.serverId)) {
    return `'${draft.serverId}' is reserved for a built-in server.`;
  }
  if (others.some((server) => server.serverId === draft.serverId)) {
    return `A server named '${draft.serverId}' already exists.`;
  }
  if (draft.displayName.trim().length === 0) return "Display name is required.";
  if (draft.command.trim().length === 0) return "Command is required.";
  if (draft.languageId.trim().length === 0) return "Language ID is required.";
  const extensions = parseExtensions(draft.extensions);
  if (extensions.length === 0) return "At least one file extension is required.";
  const seen = new Set<string>();
  for (const extension of extensions) {
    if (!EXTENSION_PATTERN.test(extension)) {
      return `'${extension}' is not a valid extension (letters, digits, '-', '_', or '+' after the dot).`;
    }
    if (BUILTIN_LANGUAGE_SERVER_EXTENSIONS.includes(extension)) {
      return `'${extension}' is handled by a built-in server.`;
    }
    if (others.some((server) => server.extensions.includes(extension))) {
      return `'${extension}' is already claimed by another server.`;
    }
    if (seen.has(extension)) return `'${extension}' is listed more than once.`;
    seen.add(extension);
  }
  return null;
}

function draftToServer(draft: ServerDraft): CustomLanguageServer {
  return {
    serverId: draft.serverId,
    displayName: draft.displayName.trim(),
    command: draft.command.trim(),
    args: parseArgs(draft.args),
    extensions: parseExtensions(draft.extensions),
    languageId: draft.languageId.trim(),
  };
}

interface LanguageServerDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Server being edited, or null when adding a new one. */
  editing: CustomLanguageServer | null;
  servers: ReadonlyArray<CustomLanguageServer>;
  onSave: (next: ReadonlyArray<CustomLanguageServer>) => void;
}

function LanguageServerDialog({
  open,
  onOpenChange,
  editing,
  servers,
  onSave,
}: LanguageServerDialogProps) {
  const [draft, setDraft] = useState<ServerDraft>(() =>
    editing ? draftFromServer(editing) : EMPTY_DRAFT,
  );
  const [serverIdTouched, setServerIdTouched] = useState(editing !== null);
  const [hasAttemptedSubmit, setHasAttemptedSubmit] = useState(false);

  const others = useMemo(
    () => servers.filter((server) => server.serverId !== editing?.serverId),
    [editing, servers],
  );
  const validationError = validateDraft(draft, others);
  const showError = hasAttemptedSubmit && validationError !== null;

  const setField = useCallback((field: keyof ServerDraft, value: string) => {
    setDraft((current) => {
      const next = { ...current, [field]: value };
      return next;
    });
  }, []);

  const handleDisplayNameChange = useCallback(
    (value: string) => {
      setDraft((current) => ({
        ...current,
        displayName: value,
        ...(serverIdTouched ? {} : { serverId: slugifyDisplayName(value) }),
      }));
    },
    [serverIdTouched],
  );

  const handleSave = useCallback(() => {
    setHasAttemptedSubmit(true);
    if (validationError !== null) return;
    const nextServer = draftToServer(draft);
    const next = editing
      ? servers.map((server) => (server.serverId === editing.serverId ? nextServer : server))
      : [...servers, nextServer];
    onSave(next);
    onOpenChange(false);
  }, [draft, editing, onOpenChange, onSave, servers, validationError]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogPopup className="max-w-lg overflow-hidden">
        <div className="flex min-h-0 flex-col overflow-hidden border-foreground/10 bg-background shadow-2xl">
          <DialogHeader className="border-b border-border/70 bg-background">
            <DialogTitle>{editing ? "Edit language server" : "Add language server"}</DialogTitle>
            <DialogDescription>
              Connect any language server that speaks LSP over stdio. The binary is resolved from
              PATH (or an absolute path) and starts when a matching file is opened.
            </DialogDescription>
          </DialogHeader>

          <div
            data-slot="dialog-panel"
            className="space-y-4 border-b border-border/70 bg-muted/20 px-6 py-5"
          >
            <label className="grid gap-2">
              <span className="text-xs font-medium text-foreground">Display name</span>
              <Input
                className="bg-background"
                placeholder="e.g. Ruby (solargraph)"
                value={draft.displayName}
                onChange={(event) => handleDisplayNameChange(event.target.value)}
              />
            </label>

            <label className="grid gap-2">
              <span className="text-xs font-medium text-foreground">Server ID</span>
              <Input
                className="bg-background"
                placeholder="ruby"
                value={draft.serverId}
                disabled={editing !== null}
                onChange={(event) => {
                  setServerIdTouched(true);
                  setField("serverId", event.target.value);
                }}
              />
              <span className="text-[11px] text-muted-foreground">
                Stable registry key. Letters, digits, '-', or '_'.
              </span>
            </label>

            <label className="grid gap-2">
              <span className="text-xs font-medium text-foreground">Command</span>
              <Input
                className="bg-background"
                placeholder="solargraph"
                value={draft.command}
                onChange={(event) => setField("command", event.target.value)}
              />
              <span className="text-[11px] text-muted-foreground">
                Executable resolved from PATH, or an absolute path.
              </span>
            </label>

            <label className="grid gap-2">
              <span className="text-xs font-medium text-foreground">Arguments</span>
              <Input
                className="bg-background"
                placeholder="stdio"
                value={draft.args}
                onChange={(event) => setField("args", event.target.value)}
              />
              <span className="text-[11px] text-muted-foreground">
                Space-separated. Most servers need a flag like 'stdio' or '--stdio'.
              </span>
            </label>

            <label className="grid gap-2">
              <span className="text-xs font-medium text-foreground">File extensions</span>
              <Input
                className="bg-background"
                placeholder=".rb .rake"
                value={draft.extensions}
                onChange={(event) => setField("extensions", event.target.value)}
              />
              <span className="text-[11px] text-muted-foreground">
                Space or comma separated, e.g. '.rb, .rake'.
              </span>
            </label>

            <label className="grid gap-2">
              <span className="text-xs font-medium text-foreground">Language ID</span>
              <Input
                className="bg-background"
                placeholder="ruby"
                value={draft.languageId}
                onChange={(event) => setField("languageId", event.target.value)}
              />
              <span className="text-[11px] text-muted-foreground">
                LSP languageId sent when a document opens, e.g. 'ruby', 'cpp', 'zig'.
              </span>
            </label>

            {showError ? <p className="text-[11px] text-destructive">{validationError}</p> : null}
          </div>

          <DialogFooter className="border-t bg-background">
            <Button variant="outline" size="sm" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button size="sm" onClick={handleSave}>
              {editing ? "Save changes" : "Add server"}
            </Button>
          </DialogFooter>
        </div>
      </DialogPopup>
    </Dialog>
  );
}

type ServerStatusMap = ReadonlyMap<string, LspServerStatus["state"]>;

const EMPTY_STATUS_MAP: ServerStatusMap = new Map();

function StatusBadge({ state }: { state: LspServerStatus["state"] | undefined }) {
  if (state === undefined) {
    return (
      <Badge variant="outline" size="sm">
        Idle
      </Badge>
    );
  }
  const badge = STATUS_BADGES[state];
  return (
    <Badge variant={badge.variant} size="sm">
      {badge.label}
    </Badge>
  );
}

function LanguageServersPanelBody({
  statuses,
  statusProbe,
}: {
  statuses: ServerStatusMap;
  statusProbe: EnvironmentProject | null;
}) {
  const languageServers = usePrimarySettings((settings) => settings.languageServers);
  const updateSettings = useUpdatePrimarySettings();

  const [dialogState, setDialogState] = useState<{
    open: boolean;
    editing: CustomLanguageServer | null;
  }>({ open: false, editing: null });

  const handleSave = useCallback(
    (next: ReadonlyArray<CustomLanguageServer>) => {
      updateSettings({ languageServers: next });
      toastManager.add({
        type: "success",
        title: "Language servers updated",
        description: "Changes apply to matching files the next time they are opened.",
      });
    },
    [updateSettings],
  );

  const handleRemove = useCallback(
    (serverId: string) => {
      updateSettings({
        languageServers: languageServers.filter((server) => server.serverId !== serverId),
      });
      toastManager.add({
        type: "success",
        title: "Language server removed",
        description: `'${serverId}' was removed. Its running instances shut down.`,
      });
    },
    [languageServers, updateSettings],
  );

  return (
    <SettingsPageContainer>
      <SettingsSection
        title="Language servers"
        headerAction={
          <Tooltip>
            <TooltipTrigger
              render={
                <Button
                  size="icon-xs"
                  variant="ghost"
                  className="size-5 rounded-sm p-0 text-muted-foreground hover:text-foreground"
                  onClick={() => setDialogState({ open: true, editing: null })}
                  aria-label="Add language server"
                >
                  <PlusIcon className="size-3" />
                </Button>
              }
            />
            <TooltipPopup side="top">Add language server</TooltipPopup>
          </Tooltip>
        }
      >
        {languageServers.length === 0 ? (
          <SettingsRow
            title="No custom servers"
            description="Add a language server to get completion, hover, and diagnostics for languages beyond the built-ins."
            control={
              <Button
                size="sm"
                variant="outline"
                onClick={() => setDialogState({ open: true, editing: null })}
              >
                <PlusIcon className="size-3.5" /> Add server
              </Button>
            }
          />
        ) : (
          languageServers.map((server) => (
            <SettingsRow
              key={server.serverId}
              title={
                <span className="flex items-center gap-2">
                  {server.displayName}
                  <StatusBadge state={statuses.get(server.serverId)} />
                </span>
              }
              description={`${[server.command, ...server.args].join(" ")} · ${server.extensions.join(" ")} · languageId '${server.languageId}'`}
              control={
                <div className="flex items-center gap-1">
                  <Button
                    size="icon-xs"
                    variant="ghost"
                    aria-label={`Edit ${server.displayName}`}
                    onClick={() => setDialogState({ open: true, editing: server })}
                  >
                    <PencilIcon className="size-3.5" />
                  </Button>
                  <Button
                    size="icon-xs"
                    variant="ghost"
                    className="text-muted-foreground hover:text-destructive"
                    aria-label={`Remove ${server.displayName}`}
                    onClick={() => handleRemove(server.serverId)}
                  >
                    <Trash2Icon className="size-3.5" />
                  </Button>
                </div>
              }
            />
          ))
        )}
      </SettingsSection>

      <SettingsSection title="Built-in servers">
        {BUILTIN_SERVER_ROWS.map((server) => (
          <SettingsRow
            key={server.serverId}
            title={
              <span className="flex items-center gap-2">
                {server.displayName}
                <StatusBadge state={statuses.get(server.serverId)} />
              </span>
            }
            description={`${server.detail} · ${server.extensions}`}
            control={
              <Badge variant="secondary" size="sm">
                Built-in
              </Badge>
            }
          />
        ))}
        {statusProbe ? (
          <SettingsRow
            title="Status source"
            description={`Live server state is shown for '${statusProbe.title}'. Servers start on demand when a matching file is opened, so 'Idle' just means nothing has needed them there yet.`}
          />
        ) : null}
      </SettingsSection>

      {dialogState.open ? (
        <LanguageServerDialog
          key={dialogState.editing?.serverId ?? "add"}
          open={dialogState.open}
          onOpenChange={(open) => setDialogState((current) => ({ ...current, open }))}
          editing={dialogState.editing}
          servers={languageServers}
          onSave={handleSave}
        />
      ) : null}
    </SettingsPageContainer>
  );
}

function LanguageServersPanelWithStatus({ probe }: { probe: EnvironmentProject }) {
  const statusResult = useAtomValue(
    lspEnvironment.serverStatus({
      environmentId: probe.environmentId,
      input: { cwd: probe.workspaceRoot },
    }),
  );
  const statuses = useMemo<ServerStatusMap>(() => {
    const status = Option.getOrNull(AsyncResult.value(statusResult));
    if (!status) return EMPTY_STATUS_MAP;
    return new Map(status.servers.map((server) => [server.serverId, server.state]));
  }, [statusResult]);
  return <LanguageServersPanelBody statuses={statuses} statusProbe={probe} />;
}

export function LanguageServersPanel() {
  const projects = useProjects();
  // Live server state is per workspace; probe the most recently active
  // project so the panel reflects the workspace the user is working in.
  const probe = useMemo(() => {
    let latest: EnvironmentProject | null = null;
    for (const project of projects) {
      if (latest === null || project.updatedAt > latest.updatedAt) latest = project;
    }
    return latest;
  }, [projects]);

  if (probe === null) {
    return <LanguageServersPanelBody statuses={EMPTY_STATUS_MAP} statusProbe={null} />;
  }
  return <LanguageServersPanelWithStatus probe={probe} />;
}
