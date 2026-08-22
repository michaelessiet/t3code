import type { EnvironmentId } from "@t3tools/contracts";
import { createRuntimeCommand } from "@t3tools/client-runtime/state/runtime";

import { ensureClaudeBinaryAvailable } from "./claudeBinaryInstall";
import { connectionAtomRuntime } from "./connection/runtime";

export const installClaudeBinary = createRuntimeCommand(connectionAtomRuntime, {
  label: "web:claude:install-binary",
  concurrency: {
    mode: "serial",
    key: (input: { readonly environmentId: EnvironmentId }) => input.environmentId,
  },
  execute: (input: { readonly environmentId: EnvironmentId }) =>
    ensureClaudeBinaryAvailable(input.environmentId),
});
