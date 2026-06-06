import { describe, expect, test } from "vitest";

import {
  consoleSurfaceFromPackageManifest,
  defineConsolePackageManifest,
} from ".";

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

  test("maps package manifests to Rust console surface metadata", () => {
    const manifest = defineConsolePackageManifest({
      area: "data",
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
    } as const);

    expect(consoleSurfaceFromPackageManifest(manifest)).toEqual({
      area: "data",
      icon: "database",
      label: "Billing",
      name: "billing",
      package: {
        export: "billingConsoleModule",
        name: "@lenso/billing-console",
      },
      required_capabilities: ["billing.read"],
      route: "/data/billing",
    });
  });

  test("maps package manifest navigation to Rust console surface metadata", () => {
    const manifest = defineConsolePackageManifest({
      area: "data",
      exportName: "crmConsoleModule",
      icon: "database",
      id: "crm",
      label: "Contacts",
      navigation: {
        group: {
          id: "customers",
          label: "Customers",
          order: 20,
        },
        order: 10,
        workspace: {
          icon: "briefcase",
          id: "crm",
          label: "CRM",
        },
      },
      packageName: "@lenso/crm-console",
      requiredCapabilities: ["crm.contacts.read"],
      route: "/crm/contacts",
      source: "installed",
      surfaceName: "contacts",
      version: "workspace",
    } as const);

    expect(consoleSurfaceFromPackageManifest(manifest)).toEqual({
      area: "data",
      icon: "database",
      label: "Contacts",
      name: "contacts",
      navigation: {
        group: {
          id: "customers",
          label: "Customers",
          order: 20,
        },
        order: 10,
        workspace: {
          icon: "briefcase",
          id: "crm",
          label: "CRM",
        },
      },
      package: {
        export: "crmConsoleModule",
        name: "@lenso/crm-console",
      },
      required_capabilities: ["crm.contacts.read"],
      route: "/crm/contacts",
    });
  });

  test("omits install-only manifest fields from console surface metadata", () => {
    const manifest = defineConsolePackageManifest({
      area: "runtime",
      exportName: "storyConsoleModule",
      id: "platform-story",
      label: "Stories",
      packageName: "@lenso/story-console",
      requiredCapabilities: [],
      route: "/runtime/stories",
      source: "first_party",
      surfaceName: "stories",
      version: "workspace",
    } as const);

    expect(Object.keys(consoleSurfaceFromPackageManifest(manifest))).toEqual([
      "area",
      "label",
      "name",
      "package",
      "required_capabilities",
      "route",
    ]);
  });
});
