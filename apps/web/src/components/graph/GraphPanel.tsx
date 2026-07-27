/**
 * GraphPanel - the knowledge-graph right-panel surface.
 *
 * A state machine over one poll of `graph.status`: runtime-missing →
 * not-built → building → ready → failed. Each state is a terminal screen with
 * exactly one obvious action, because every one of them is a dead end the user
 * has to be told how to leave.
 *
 * The canvas lives behind `React.lazy` and is the only thing that pulls sigma
 * and graphology into the app. This module deliberately imports nothing from
 * either — reaching for a colour helper is enough to defeat the split, which
 * is why those live in `graphColors.ts`.
 *
 * @module GraphPanel
 */
import type { EnvironmentId, GraphBuildMode, GraphStatus } from "@t3tools/contracts";
import { GRAPH_SUBGRAPH_MAX_NODES } from "@t3tools/contracts";
import { Link } from "@tanstack/react-router";
import {
  AlertTriangleIcon,
  HammerIcon,
  RefreshCwIcon,
  SparklesIcon,
  WaypointsIcon,
} from "lucide-react";
import { Suspense, lazy, useCallback, useMemo, useState } from "react";

import {
  AlertDialog,
  AlertDialogClose,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogPopup,
  AlertDialogTitle,
} from "~/components/ui/alert-dialog";
import { Badge } from "~/components/ui/badge";
import { Button, buttonVariants } from "~/components/ui/button";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "~/components/ui/empty";
import { Spinner } from "~/components/ui/spinner";
import { graphEnvironment } from "~/state/graph";
import { useEnvironmentQuery } from "~/state/query";
import { useAtomCommand } from "~/state/use-atom-command";

import { type GraphFocus, GraphSidebar, defaultFocus } from "./GraphSidebar";

const GraphCanvas = lazy(() => import("./GraphCanvas"));

/**
 * Neighbourhood depth for a node focus.
 *
 * Two hops: one hop is a star and tells you nothing about structure, three
 * hops on a hub reaches most of the graph and hits the node cap immediately.
 */
const NODE_FOCUS_DEPTH = 2;

interface GraphPanelProps {
  readonly environmentId: EnvironmentId;
  /** Workspace root of the thread — the server resolves it to a store key. */
  readonly cwd: string;
  readonly onOpenFile: (relativePath: string, line?: number) => void;
}

function PanelMessage(props: {
  readonly title: string;
  readonly description: React.ReactNode;
  readonly icon: React.ReactNode;
  readonly action?: React.ReactNode;
}) {
  return (
    <Empty>
      <EmptyHeader>
        <EmptyMedia variant="icon">{props.icon}</EmptyMedia>
        <EmptyTitle>{props.title}</EmptyTitle>
        <EmptyDescription>{props.description}</EmptyDescription>
      </EmptyHeader>
      {props.action === undefined ? null : <EmptyContent>{props.action}</EmptyContent>}
    </Empty>
  );
}

function BuildBanner({ status }: { status: GraphStatus["build"] }) {
  if (status.state !== "queued" && status.state !== "running") return null;
  return (
    <div className="flex items-center gap-2 border-b bg-muted/40 px-3 py-1.5 text-[11px] text-muted-foreground">
      <Spinner className="size-3" />
      <span>
        {status.state === "queued"
          ? "Build queued…"
          : `Building${status.message === null ? "" : ` — ${status.message}`}…`}
      </span>
      {status.mode === "semantic" ? <Badge size="sm">Semantic</Badge> : null}
    </div>
  );
}

/**
 * The one place in the feature that spends money, so it asks first.
 *
 * Everything else here runs graphify's local AST pass. A deep build additionally
 * shells out to the `claude` CLI, which is why the copy names both costs the
 * user cannot see from the button: tokens, and the CLI having to be installed at
 * all. If it is not, graphify exits with "Claude Code CLI not found on $PATH"
 * and that text lands verbatim in the failure panel.
 */
function DeepBuildDialog(props: {
  readonly open: boolean;
  readonly onOpenChange: (open: boolean) => void;
  readonly onConfirm: () => void;
}) {
  return (
    <AlertDialog onOpenChange={props.onOpenChange} open={props.open}>
      <AlertDialogPopup>
        <AlertDialogHeader>
          <AlertDialogTitle>Run a deep build?</AlertDialogTitle>
          <AlertDialogDescription>
            A deep build re-extracts the whole workspace and adds an LLM pass on top of the
            structural one, inferring relationships the AST cannot see. It runs through the{" "}
            <code>claude</code> CLI on your PATH and spends tokens on your existing Claude
            subscription — on a large repository, a lot of them, over several minutes. The
            structural build is free and needs no API key.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogClose render={<Button variant="outline" />}>Cancel</AlertDialogClose>
          <Button onClick={props.onConfirm}>
            <SparklesIcon aria-hidden className="size-3.5" />
            Run deep build
          </Button>
        </AlertDialogFooter>
      </AlertDialogPopup>
    </AlertDialog>
  );
}

function GraphHeader(props: {
  readonly status: GraphStatus;
  readonly building: boolean;
  readonly onRebuild: () => void;
  readonly onDeepBuild: () => void;
}) {
  const snapshot = props.status.snapshot;
  return (
    <div className="flex items-center gap-2 border-b px-3 py-1.5 text-[11px]">
      <WaypointsIcon aria-hidden className="size-3.5 shrink-0 text-muted-foreground" />
      <span className="min-w-0 truncate text-muted-foreground">
        {props.status.branch ?? "detached HEAD"}
      </span>
      {snapshot === null ? null : (
        <span className="shrink-0 tabular-nums text-muted-foreground">
          {snapshot.nodeCount.toLocaleString()} nodes · {snapshot.edgeCount.toLocaleString()} edges
        </span>
      )}
      {snapshot?.stale === true ? (
        <Badge size="sm" variant="warning">
          Stale
        </Badge>
      ) : null}
      <div className="flex-1" />
      <Button
        disabled={props.building}
        onClick={props.onDeepBuild}
        size="sm"
        variant="ghost"
        title="Re-extract with an LLM pass — spends tokens"
      >
        <SparklesIcon aria-hidden className="size-3" />
        Deep build
      </Button>
      <Button
        disabled={props.building}
        onClick={props.onRebuild}
        size="sm"
        variant="ghost"
        title="Re-extract this branch"
      >
        <RefreshCwIcon aria-hidden className="size-3" />
        Rebuild
      </Button>
    </div>
  );
}

export function GraphPanel(props: GraphPanelProps) {
  const { cwd, environmentId, onOpenFile } = props;
  const [focus, setFocus] = useState<GraphFocus | null>(null);
  const [deepConfirmOpen, setDeepConfirmOpen] = useState(false);

  const statusQuery = useEnvironmentQuery(
    graphEnvironment.status({ environmentId, input: { cwd } }),
  );
  const status = statusQuery.data;
  const runBuild = useAtomCommand(graphEnvironment.build, { reportFailure: true });

  const build = useCallback(
    async (mode: GraphBuildMode, force: boolean) => {
      await runBuild({ environmentId, input: { cwd, mode, force } });
      // The command returns the queued status, but the poll holds the previous
      // one; refresh so the banner appears immediately rather than up to one
      // interval later.
      statusQuery.refresh();
    },
    [cwd, environmentId, runBuild, statusQuery],
  );

  // `force` stays false: a semantic request already bypasses the incremental
  // path server-side, so forcing would only discard graphify's file cache and
  // make the pass that costs tokens re-read everything from scratch.
  const confirmDeepBuild = useCallback(() => {
    setDeepConfirmOpen(false);
    void build("semantic", false);
  }, [build]);

  // Falling back to the largest community rather than storing it on load: the
  // snapshot can change under a rebuild, and a focus pinned to a community
  // that no longer exists would render an empty canvas with no way back.
  const effectiveFocus = useMemo<GraphFocus | null>(() => {
    if (status?.snapshot == null) return null;
    if (focus === null) return defaultFocus(status.snapshot);
    if (focus.kind === "community") {
      return status.snapshot.communities.some((community) => community.id === focus.id)
        ? focus
        : defaultFocus(status.snapshot);
    }
    return focus;
  }, [focus, status?.snapshot]);

  const subgraphQuery = useEnvironmentQuery(
    effectiveFocus === null
      ? null
      : graphEnvironment.subgraph({
          environmentId,
          input: {
            cwd,
            nodeId: effectiveFocus.kind === "node" ? effectiveFocus.id : null,
            communityId: effectiveFocus.kind === "community" ? effectiveFocus.id : null,
            depth: NODE_FOCUS_DEPTH,
            limit: GRAPH_SUBGRAPH_MAX_NODES,
          },
        }),
  );

  const openNode = useCallback(
    (node: { readonly sourceFile: string | null; readonly sourceLine: number | null }) => {
      if (node.sourceFile === null) return;
      onOpenFile(node.sourceFile, node.sourceLine ?? undefined);
    },
    [onOpenFile],
  );

  if (statusQuery.error !== null) {
    return (
      <PanelMessage
        action={
          <Button onClick={statusQuery.refresh} size="sm" variant="outline">
            Try again
          </Button>
        }
        description={statusQuery.error}
        icon={<AlertTriangleIcon />}
        title="The graph could not be read"
      />
    );
  }

  if (status === null) {
    return (
      <div className="flex size-full items-center justify-center">
        <Spinner className="size-4" />
      </div>
    );
  }

  if (!status.enabled || status.runtime.state !== "ready") {
    return (
      <PanelMessage
        action={
          <Link
            className={buttonVariants({ size: "sm", variant: "outline" })}
            to="/settings/knowledge-graph"
          >
            Open graph settings
          </Link>
        }
        description={
          status.runtime.detail ??
          "T3 Code could not find graphify. Install it from settings to build a graph."
        }
        icon={<WaypointsIcon />}
        title={status.enabled ? "graphify is not ready" : "Knowledge graph is off"}
      />
    );
  }

  const building = status.build.state === "queued" || status.build.state === "running";

  if (status.build.state === "failed" && status.snapshot === null) {
    return (
      <PanelMessage
        action={
          <Button onClick={() => void build("structural", true)} size="sm" variant="outline">
            Try again
          </Button>
        }
        description={
          <span className="block max-h-40 overflow-y-auto whitespace-pre-wrap text-left font-mono text-[10px] leading-snug">
            {status.build.detail ?? "graphify failed without reporting a reason."}
          </span>
        }
        icon={<AlertTriangleIcon />}
        title="The build failed"
      />
    );
  }

  if (status.snapshot === null) {
    return (
      <>
        <PanelMessage
          action={
            building ? (
              <span className="flex items-center gap-2 text-xs text-muted-foreground">
                <Spinner className="size-3" />
                {status.build.message ?? "Extracting…"}
              </span>
            ) : (
              // Deep build is offered here as well as in the header so that
              // wanting one does not cost a throwaway structural pass first —
              // a semantic build re-extracts from scratch either way.
              <div className="flex items-center gap-2">
                <Button onClick={() => void build("structural", false)} size="sm">
                  <HammerIcon aria-hidden className="size-3.5" />
                  Build graph
                </Button>
                <Button onClick={() => setDeepConfirmOpen(true)} size="sm" variant="ghost">
                  <SparklesIcon aria-hidden className="size-3.5" />
                  Deep build
                </Button>
              </div>
            )
          }
          description={`Extract a structural map of ${status.branch === null ? "this workspace" : `\`${status.branch}\``}. This runs locally and needs no API key.`}
          icon={<WaypointsIcon />}
          title="No graph yet"
        />
        <DeepBuildDialog
          onConfirm={confirmDeepBuild}
          onOpenChange={setDeepConfirmOpen}
          open={deepConfirmOpen}
        />
      </>
    );
  }

  return (
    <div className="flex size-full min-h-0 flex-col">
      <GraphHeader
        building={building}
        onDeepBuild={() => setDeepConfirmOpen(true)}
        onRebuild={() => void build("structural", true)}
        status={status}
      />
      <DeepBuildDialog
        onConfirm={confirmDeepBuild}
        onOpenChange={setDeepConfirmOpen}
        open={deepConfirmOpen}
      />
      <BuildBanner status={status.build} />
      <div className="flex min-h-0 flex-1">
        <GraphSidebar focus={effectiveFocus} onFocus={setFocus} snapshot={status.snapshot} />
        <div className="relative min-w-0 flex-1">
          {subgraphQuery.data === null ? (
            <div className="flex size-full items-center justify-center">
              <Spinner className="size-4" />
            </div>
          ) : (
            <Suspense
              fallback={
                <div className="flex size-full items-center justify-center">
                  <Spinner className="size-4" />
                </div>
              }
            >
              <GraphCanvas
                edges={subgraphQuery.data.edges}
                nodes={subgraphQuery.data.nodes}
                onOpenNode={openNode}
                onSelectNode={(node) => setFocus({ kind: "node", id: node.id })}
                selectedNodeId={effectiveFocus?.kind === "node" ? effectiveFocus.id : null}
              />
            </Suspense>
          )}
          {subgraphQuery.data?.truncated === true ? (
            <div className="pointer-events-none absolute right-2 bottom-2 rounded-md bg-background/90 px-2 py-1 text-[10px] text-muted-foreground shadow-sm">
              Showing the first {GRAPH_SUBGRAPH_MAX_NODES.toLocaleString()} nodes
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}

export default GraphPanel;
