/**
 * Live language-server status for the open file, shown in the file surface
 * subheader. jdtls-class servers take 30-120s to boot; without this the
 * window where hover/go-to-def silently no-op is indistinguishable from a
 * broken setup. Quiet when the server is running (or the file has none).
 */
import type { EnvironmentId } from "@t3tools/contracts";
import { useAtomValue } from "@effect/atom-react";
import * as Option from "effect/Option";
import { AsyncResult } from "effect/unstable/reactivity";

import { lspEnvironment } from "~/state/lsp";
import { Badge } from "~/components/ui/badge";
import { Spinner } from "~/components/ui/spinner";

export interface LspFileServerStatusProps {
  readonly environmentId: EnvironmentId;
  readonly cwd: string;
  readonly relativePath: string;
}

export function LspFileServerStatus({
  environmentId,
  cwd,
  relativePath,
}: LspFileServerStatusProps) {
  // Same atom-family key as useLspBridge, so this shares its poller and the
  // post-didOpen refresh rather than issuing extra status RPCs.
  const serverStatusResult = useAtomValue(
    lspEnvironment.serverStatus({ environmentId, input: { cwd } }),
  );
  const status = Option.getOrNull(AsyncResult.value(serverStatusResult));
  if (status === null) return null;

  const dotIndex = relativePath.lastIndexOf(".");
  if (dotIndex === -1) return null;
  const extension = relativePath.slice(dotIndex).toLowerCase();
  // Each extension binds to exactly one server (resolveRegistry semantics);
  // a server only appears here once its first didOpen spawned it, and a
  // crash after running removes it again — both render as quiet.
  const server = status.servers.find((candidate) => candidate.extensions.includes(extension));
  if (server === undefined || server.state === "running") return null;

  if (server.state === "starting") {
    return (
      <div className="flex shrink-0 items-center gap-1.5 text-xs text-muted-foreground">
        <Spinner className="size-3.5" />
        <span>{server.displayName} starting…</span>
      </div>
    );
  }
  return (
    <Badge variant={server.state === "failed" ? "error" : "warning"} size="sm">
      {server.displayName} {server.state === "failed" ? "failed" : "not installed"}
    </Badge>
  );
}
