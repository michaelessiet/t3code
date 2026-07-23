import type { EnvironmentId } from "@t3tools/contracts";
import { describe, expect, it } from "vite-plus/test";

import { planCopy } from "./copyEntryAcrossThreads";

const envA = "env-a" as EnvironmentId;
const envB = "env-b" as EnvironmentId;

describe("planCopy", () => {
  it("copies within the same environment server-side", () => {
    expect(planCopy({ environmentId: envA, kind: "file" }, envA)).toEqual({
      strategy: "same-environment",
    });
    expect(planCopy({ environmentId: envA, kind: "directory" }, envA)).toEqual({
      strategy: "same-environment",
    });
  });

  it("orchestrates single files across environments", () => {
    expect(planCopy({ environmentId: envA, kind: "file" }, envB)).toEqual({
      strategy: "cross-environment-file",
    });
  });

  it("refuses directories across environments", () => {
    const plan = planCopy({ environmentId: envA, kind: "directory" }, envB);
    expect(plan.strategy).toBe("unsupported");
    if (plan.strategy === "unsupported") {
      expect(plan.reason).toMatch(/folders/i);
    }
  });
});
