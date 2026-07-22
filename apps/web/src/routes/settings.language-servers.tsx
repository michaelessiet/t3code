import { createFileRoute } from "@tanstack/react-router";

import { LanguageServersPanel } from "../components/settings/LanguageServersSettings";

function SettingsLanguageServersRoute() {
  return <LanguageServersPanel />;
}

export const Route = createFileRoute("/settings/language-servers")({
  component: SettingsLanguageServersRoute,
});
