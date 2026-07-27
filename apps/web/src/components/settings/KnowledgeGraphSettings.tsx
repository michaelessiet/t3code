"use client";

import { CheckIcon, LoaderIcon, NetworkIcon } from "lucide-react";
import { useCallback, useState } from "react";
import {
  isAtomCommandInterrupted,
  squashAtomCommandFailure,
} from "@t3tools/client-runtime/state/runtime";
import type {
  GraphInstallStage,
  GraphRuntimeState,
  GraphRuntimeStatus,
  KnowledgeGraphSettings,
} from "@t3tools/contracts";
import { DEFAULT_SERVER_SETTINGS } from "@t3tools/contracts";

import { usePrimarySettings, useUpdatePrimarySettings } from "../../hooks/useSettings";
import { usePrimaryEnvironmentId } from "../../state/environments";
import { graphEnvironment } from "../../state/graph";
import { useEnvironmentQuery } from "../../state/query";
import { useAtomCommand } from "../../state/use-atom-command";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { DraftInput } from "../ui/draft-input";
import { Switch } from "../ui/switch";
import {
  SettingResetButton,
  SettingsPageContainer,
  SettingsRow,
  SettingsSection,
} from "./settingsLayout";

const DEFAULT_KNOWLEDGE_GRAPH = DEFAULT_SERVER_SETTINGS.knowledgeGraph;

const STATUS_BADGES: Record<
  GraphRuntimeState,
  { label: string; variant: "success" | "info" | "error" | "warning" | "secondary" }
> = {
  disabled: { label: "Disabled", variant: "secondary" },
  missing: { label: "Not installed", variant: "warning" },
  installing: { label: "Installing", variant: "info" },
  ready: { label: "Ready", variant: "success" },
  failed: { label: "Failed", variant: "error" },
};

/**
 * The stages the server can report, in the order they occur.
 *
 * `creating_venv` is skipped when `uv` is available, so a stage index is a
 * lower bound on progress rather than an exact position — the list marks
 * everything before the active stage as done, which reads correctly either
 * way.
 */
const INSTALL_STEPS: ReadonlyArray<{ stage: GraphInstallStage; label: string }> = [
  { stage: "checking", label: "Checking for an existing install" },
  { stage: "waiting_for_lock", label: "Waiting for another installer" },
  { stage: "creating_venv", label: "Creating a Python environment" },
  { stage: "installing", label: "Installing graphify" },
  { stage: "validating", label: "Validating the install" },
];

type InstallView =
  | { readonly status: "idle" }
  | {
      readonly status: "running";
      readonly stage: GraphInstallStage;
      readonly detail: string | null;
    }
  | { readonly status: "failed"; readonly message: string };

function installFailureMessage(error: unknown): string {
  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message;
  }
  return "The install failed. Check the server log for details.";
}

function RuntimeSummary({ runtime }: { runtime: GraphRuntimeStatus }) {
  const facts: Array<string> = [];
  if (runtime.version !== null) facts.push(`graphify ${runtime.version}`);
  if (runtime.source !== null) {
    facts.push(runtime.source === "managed" ? "installed by T3 Code" : "found on this machine");
  }
  if (runtime.interpreterPath !== null) facts.push(runtime.interpreterPath);
  return (
    <div className="space-y-1">
      {facts.length > 0 ? (
        <p className="truncate font-mono text-[11px] text-muted-foreground">{facts.join(" · ")}</p>
      ) : null}
      {runtime.detail !== null ? <p>{runtime.detail}</p> : null}
      {runtime.state === "missing" && !runtime.pythonAvailable ? (
        <p>
          Install Python 3.10 or newer first — T3 Code will not install a language runtime for you.
        </p>
      ) : null}
    </div>
  );
}

function InstallProgress({ view }: { view: InstallView }) {
  if (view.status === "failed") {
    return <p className="text-[11px] text-destructive-foreground">{view.message}</p>;
  }
  if (view.status !== "running") return null;
  const activeIndex = INSTALL_STEPS.findIndex((step) => step.stage === view.stage);
  return (
    <div className="space-y-1.5 pt-1">
      <ul className="space-y-1">
        {INSTALL_STEPS.map((step, index) => {
          const isActive = index === activeIndex;
          const isDone = activeIndex >= 0 && index < activeIndex;
          return (
            <li
              key={step.stage}
              className={
                isActive
                  ? "flex items-center gap-1.5 text-[11px] text-foreground"
                  : "flex items-center gap-1.5 text-[11px] text-muted-foreground/70"
              }
            >
              {isActive ? (
                <LoaderIcon className="size-3 animate-spin" aria-hidden />
              ) : isDone ? (
                <CheckIcon className="size-3" aria-hidden />
              ) : (
                <span className="size-3" aria-hidden />
              )}
              {step.label}
            </li>
          );
        })}
      </ul>
      {view.detail !== null ? (
        <pre className="max-h-24 overflow-y-auto whitespace-pre-wrap rounded-md bg-muted/50 p-2 font-mono text-[10px] leading-snug text-muted-foreground">
          {view.detail}
        </pre>
      ) : null}
    </div>
  );
}

export function KnowledgeGraphPanel() {
  const settings = usePrimarySettings((current) => current.knowledgeGraph);
  const updateSettings = useUpdatePrimarySettings();
  const environmentId = usePrimaryEnvironmentId();
  const [install, setInstall] = useState<InstallView>({ status: "idle" });

  // `updateSettings` takes a top-level `Partial<UnifiedSettings>`, so a nested
  // struct is replaced wholesale rather than merged — the same contract as
  // `languageServers` and `providerInstances`. Spread the current value so a
  // single-field edit does not blank the rest.
  const patchGraphSettings = useCallback(
    (patch: Partial<KnowledgeGraphSettings>) =>
      updateSettings({ knowledgeGraph: { ...settings, ...patch } }),
    [settings, updateSettings],
  );

  // Probing costs a subprocess spawn, so only ask once the feature is on.
  // The server answers `disabled` without probing anyway; this keeps the
  // request itself off the wire.
  const runtimeQuery = useEnvironmentQuery(
    environmentId !== null && settings.enabled
      ? graphEnvironment.runtimeStatus({ environmentId, input: {} })
      : null,
  );
  const runtime = runtimeQuery.data;
  const runInstall = useAtomCommand(graphEnvironment.installRuntime, { reportFailure: false });

  const handleInstall = useCallback(async () => {
    if (environmentId === null) return;
    setInstall({ status: "running", stage: "checking", detail: null });
    const result = await runInstall({
      environmentId,
      input: {
        interpreterPath: settings.graphifyPath.trim() === "" ? null : settings.graphifyPath.trim(),
        onEvent: (event) => {
          if (event.type === "progress") {
            setInstall({ status: "running", stage: event.stage, detail: event.detail });
          }
        },
      },
    });
    if (result._tag === "Failure") {
      setInstall(
        isAtomCommandInterrupted(result)
          ? { status: "idle" }
          : { status: "failed", message: installFailureMessage(squashAtomCommandFailure(result)) },
      );
      return;
    }
    setInstall({ status: "idle" });
    // The command returns the fresh status, but the query atom holds the
    // stale one; refresh so the badge and the summary agree.
    runtimeQuery.refresh();
  }, [environmentId, runInstall, runtimeQuery, settings.graphifyPath]);

  const isInstalling = install.status === "running";
  const badge = STATUS_BADGES[runtime?.state ?? "disabled"];

  return (
    <SettingsPageContainer>
      <SettingsSection title="Knowledge graph" icon={<NetworkIcon className="size-3.5" />}>
        <SettingsRow
          title="Enable knowledge graph"
          description="Build a queryable map of how code in a project relates, and expose it to agents. Off by default: it needs graphify, a Python tool T3 Code does not ship."
          resetAction={
            settings.enabled !== DEFAULT_KNOWLEDGE_GRAPH.enabled ? (
              <SettingResetButton
                label="knowledge graph"
                onClick={() => patchGraphSettings({ enabled: DEFAULT_KNOWLEDGE_GRAPH.enabled })}
              />
            ) : null
          }
          control={
            <Switch
              checked={settings.enabled}
              onCheckedChange={(checked) => patchGraphSettings({ enabled: Boolean(checked) })}
              aria-label="Enable knowledge graph"
            />
          }
        />

        <SettingsRow
          title="graphify runtime"
          description="Where T3 Code found graphify. Nothing is probed while the feature is off."
          status={
            settings.enabled ? (
              runtimeQuery.error !== null ? (
                <p className="text-destructive-foreground">{runtimeQuery.error}</p>
              ) : runtime !== null ? (
                <RuntimeSummary runtime={runtime} />
              ) : null
            ) : null
          }
          control={
            <div className="flex items-center gap-2">
              <Badge variant={badge.variant} size="sm">
                {isInstalling ? "Installing" : badge.label}
              </Badge>
              {settings.enabled && runtime !== null && runtime.state !== "ready" ? (
                <Button
                  size="sm"
                  variant="outline"
                  disabled={isInstalling || environmentId === null}
                  onClick={() => void handleInstall()}
                >
                  {isInstalling ? "Installing…" : "Install graphify"}
                </Button>
              ) : null}
            </div>
          }
        >
          <div className="pb-3.5">
            <InstallProgress view={install} />
          </div>
        </SettingsRow>

        <SettingsRow
          title="graphify path"
          description="Absolute path to a graphify executable or a Python interpreter that has it installed. Leave empty to detect one automatically."
          resetAction={
            settings.graphifyPath !== DEFAULT_KNOWLEDGE_GRAPH.graphifyPath ? (
              <SettingResetButton
                label="graphify path"
                onClick={() =>
                  patchGraphSettings({ graphifyPath: DEFAULT_KNOWLEDGE_GRAPH.graphifyPath })
                }
              />
            ) : null
          }
          control={
            <DraftInput
              className="w-full sm:w-72"
              value={settings.graphifyPath}
              onCommit={(next) => patchGraphSettings({ graphifyPath: next })}
              placeholder="Auto-detect"
              spellCheck={false}
              aria-label="graphify path"
            />
          }
        />
      </SettingsSection>

      <SettingsSection title="Storage">
        <SettingsRow
          title="Where graphs are stored"
          description="Graphs are written into T3 Code's own data directory, keyed by project and branch. Your repository is never modified and no graphify-out folder appears in your file tree."
        />

        <SettingsRow
          title="Keep unused graphs for"
          description="Days since a graph was last opened before it is deleted. 0 keeps graphs forever."
          resetAction={
            settings.retentionDays !== DEFAULT_KNOWLEDGE_GRAPH.retentionDays ? (
              <SettingResetButton
                label="graph retention"
                onClick={() =>
                  patchGraphSettings({ retentionDays: DEFAULT_KNOWLEDGE_GRAPH.retentionDays })
                }
              />
            ) : null
          }
          control={
            <DraftInput
              className="w-full sm:w-28"
              value={String(settings.retentionDays)}
              onCommit={(next) => {
                const parsed = Number.parseInt(next, 10);
                if (!Number.isSafeInteger(parsed) || parsed < 0) return;
                patchGraphSettings({ retentionDays: parsed });
              }}
              inputMode="numeric"
              aria-label="Days to keep unused graphs"
            />
          }
        />

        <SettingsRow
          title="Store size budget"
          description="Megabytes of graph data to keep. The least recently opened graphs are dropped first. 0 disables the budget."
          resetAction={
            settings.maxStoreMegabytes !== DEFAULT_KNOWLEDGE_GRAPH.maxStoreMegabytes ? (
              <SettingResetButton
                label="graph store size budget"
                onClick={() =>
                  patchGraphSettings({
                    maxStoreMegabytes: DEFAULT_KNOWLEDGE_GRAPH.maxStoreMegabytes,
                  })
                }
              />
            ) : null
          }
          control={
            <DraftInput
              className="w-full sm:w-28"
              value={String(settings.maxStoreMegabytes)}
              onCommit={(next) => {
                const parsed = Number.parseInt(next, 10);
                if (!Number.isSafeInteger(parsed) || parsed < 0) return;
                patchGraphSettings({ maxStoreMegabytes: parsed });
              }}
              inputMode="numeric"
              aria-label="Graph store size budget in megabytes"
            />
          }
        />
      </SettingsSection>
    </SettingsPageContainer>
  );
}
