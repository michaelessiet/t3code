import { createFileRoute } from "@tanstack/react-router";

import { KnowledgeGraphPanel } from "../components/settings/KnowledgeGraphSettings";

function SettingsKnowledgeGraphRoute() {
  return <KnowledgeGraphPanel />;
}

export const Route = createFileRoute("/settings/knowledge-graph")({
  component: SettingsKnowledgeGraphRoute,
});
