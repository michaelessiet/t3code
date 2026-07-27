import { createGraphEnvironmentAtoms } from "@t3tools/client-runtime/state/graph";

import { connectionAtomRuntime } from "../connection/runtime";

export const graphEnvironment = createGraphEnvironmentAtoms(connectionAtomRuntime);
