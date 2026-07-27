/**
 * GraphSidebar - the list you navigate the graph from.
 *
 * The canvas can only ever show a bounded slice, so the sidebar is what makes
 * the rest reachable: communities are the coarse map, god nodes are the
 * structural hubs worth looking at first. Both are already in the snapshot, so
 * this renders without a further round trip.
 *
 * @module GraphSidebar
 */
import type { GraphSnapshot } from "@t3tools/contracts";

import { ScrollArea } from "~/components/ui/scroll-area";
import { cn } from "~/lib/utils";

import { communityColor } from "./graphColors";

/** What the canvas is currently showing. */
export type GraphFocus =
  | { readonly kind: "community"; readonly id: number }
  | { readonly kind: "node"; readonly id: string };

export function focusEquals(a: GraphFocus | null, b: GraphFocus): boolean {
  return a !== null && a.kind === b.kind && a.id === b.id;
}

/** Largest community, which is the most useful thing to open on. */
export function defaultFocus(snapshot: GraphSnapshot): GraphFocus | null {
  const largest = snapshot.communities.reduce<GraphSnapshot["communities"][number] | null>(
    (best, community) => (best === null || community.nodeCount > best.nodeCount ? community : best),
    null,
  );
  if (largest !== null) return { kind: "community", id: largest.id };
  const hub = snapshot.godNodes[0];
  return hub === undefined ? null : { kind: "node", id: hub.id };
}

function SectionHeading({ children }: { children: React.ReactNode }) {
  return (
    <h3 className="px-2 pt-3 pb-1 font-medium text-[10px] text-muted-foreground uppercase tracking-wide">
      {children}
    </h3>
  );
}

function FocusRow(props: {
  readonly active: boolean;
  readonly color: string;
  readonly label: string;
  readonly detail: string;
  readonly onClick: () => void;
}) {
  return (
    <button
      className={cn(
        "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs transition-colors",
        props.active ? "bg-accent text-accent-foreground" : "hover:bg-accent/50",
      )}
      onClick={props.onClick}
      type="button"
    >
      <span
        aria-hidden
        className="size-2 shrink-0 rounded-full"
        style={{ backgroundColor: props.color }}
      />
      <span className="min-w-0 flex-1 truncate">{props.label}</span>
      <span className="shrink-0 tabular-nums text-[10px] text-muted-foreground">
        {props.detail}
      </span>
    </button>
  );
}

export function GraphSidebar(props: {
  readonly snapshot: GraphSnapshot;
  readonly focus: GraphFocus | null;
  readonly onFocus: (focus: GraphFocus) => void;
}) {
  return (
    <ScrollArea className="h-full w-56 shrink-0 border-r">
      <div className="p-1 pb-4">
        <SectionHeading>Communities</SectionHeading>
        {props.snapshot.communities.length === 0 ? (
          <p className="px-2 py-1 text-[11px] text-muted-foreground">
            graphify found no clusters in this graph.
          </p>
        ) : (
          props.snapshot.communities.map((community) => (
            <FocusRow
              active={focusEquals(props.focus, { kind: "community", id: community.id })}
              color={communityColor(community.id)}
              detail={community.nodeCount.toLocaleString()}
              key={community.id}
              label={community.label}
              onClick={() => props.onFocus({ kind: "community", id: community.id })}
            />
          ))
        )}

        <SectionHeading>Hubs</SectionHeading>
        {props.snapshot.godNodes.length === 0 ? (
          <p className="px-2 py-1 text-[11px] text-muted-foreground">No standout hubs.</p>
        ) : (
          props.snapshot.godNodes.map((node) => (
            <FocusRow
              active={focusEquals(props.focus, { kind: "node", id: node.id })}
              color={communityColor(node.communityId)}
              // Degree, not size: the number is the fact, `nodeSize` is only
              // how the canvas draws it.
              detail={`${node.degree.toLocaleString()}`}
              key={node.id}
              label={node.label}
              onClick={() => props.onFocus({ kind: "node", id: node.id })}
            />
          ))
        )}
      </div>
    </ScrollArea>
  );
}
