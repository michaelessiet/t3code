import { createLspEnvironmentAtoms } from "@t3tools/client-runtime/state/lsp";

import { connectionAtomRuntime } from "../connection/runtime";

export const lspEnvironment = createLspEnvironmentAtoms(connectionAtomRuntime);
