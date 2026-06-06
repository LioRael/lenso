import { describe, expect, test } from "vitest";

import { defineConsolePackageManifest } from ".";

describe("runtime console package API", () => {
  test("defines console package manifests for frontend modules", () => {
    const manifest = defineConsolePackageManifest({
      area: "runtime",
      exportName: "billingConsoleModule",
      icon: "database",
      id: "billing",
      label: "Billing",
      packageName: "@lenso/billing-console",
      requiredCapabilities: ["billing.read"],
      route: "/data/billing",
      source: "installed",
      surfaceName: "billing",
      version: "workspace",
    });

    expect(manifest).toEqual({
      area: "runtime",
      exportName: "billingConsoleModule",
      icon: "database",
      id: "billing",
      label: "Billing",
      packageName: "@lenso/billing-console",
      requiredCapabilities: ["billing.read"],
      route: "/data/billing",
      source: "installed",
      surfaceName: "billing",
      version: "workspace",
    });
  });
});
