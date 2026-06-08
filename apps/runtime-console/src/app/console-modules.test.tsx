import { describe, expect, test } from "vitest";

import {
  buildConsoleNavigation,
  buildConsoleRoutes,
  consoleModuleMetadataFromManifest,
  consoleModulePackageReferences,
  consoleModules,
  defaultConsoleRoute,
  defineConsoleModule,
  selectDefaultConsoleRoute,
} from "./console-modules";
import { buildWorkspaceNavigation } from "./console-workspace-navigation";

function TestPage() {
  return <div>Story module</div>;
}

describe("console module registry", () => {
  test("turns build-time module contributions into navigation and routes", () => {
    const module = defineConsoleModule({
      id: "platform-story",
      surfaces: [
        {
          area: "runtime",
          component: TestPage,
          icon: "workflow",
          label: "Stories",
          path: "/runtime/stories",
        },
      ],
    });

    expect(buildConsoleNavigation([module])).toEqual([
      {
        icon: "workflow",
        label: "Stories",
        moduleId: "platform-story",
        navigation: {
          order: -10,
          workspace: {
            icon: "settings",
            id: "system",
            label: "System",
          },
        },
        path: "/runtime/stories",
      },
    ]);
    expect(buildConsoleRoutes([module])).toHaveLength(1);
    expect(buildConsoleRoutes([module])[0]?.path).toBe("/runtime/stories");
  });

  test("accepts optional workspace navigation metadata on module surfaces", () => {
    const navigation = {
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
    } as const;
    const module = defineConsoleModule({
      id: "crm",
      surfaces: [
        {
          area: "data",
          component: TestPage,
          icon: "database",
          label: "Contacts",
          navigation,
          path: "/crm/contacts",
        },
      ],
    });

    expect(module.surfaces[0]?.navigation).toEqual(navigation);
    expect(buildConsoleRoutes([module])[0]?.navigation).toEqual(navigation);
    expect(buildConsoleNavigation([module])[0]?.navigation).toEqual(navigation);
  });

  test("rejects duplicate contribution paths before router creation", () => {
    const storyModule = defineConsoleModule({
      id: "platform-story",
      surfaces: [
        {
          area: "runtime",
          component: TestPage,
          label: "Stories",
          path: "/runtime/stories",
        },
      ],
    });
    const duplicateModule = defineConsoleModule({
      id: "other-story",
      surfaces: [
        {
          area: "runtime",
          component: TestPage,
          label: "Other Stories",
          path: "/runtime/stories",
        },
      ],
    });

    expect(() => buildConsoleRoutes([storyModule, duplicateModule])).toThrow(
      "Duplicate console module route: /runtime/stories"
    );
  });

  test("rejects module routes that collide with host routes", () => {
    const module = defineConsoleModule({
      id: "module-registry",
      surfaces: [
        {
          area: "data",
          component: TestPage,
          label: "Module Registry",
          path: "/modules",
        },
      ],
    });

    expect(() => buildConsoleRoutes([module])).toThrow(
      "Reserved host console route: /modules"
    );
  });

  test("uses the first registered route as the default console route", () => {
    const storyModule = defineConsoleModule({
      id: "platform-story",
      surfaces: [
        {
          area: "runtime",
          component: TestPage,
          label: "Stories",
          path: "/runtime/stories",
        },
      ],
    });
    const identityModule = defineConsoleModule({
      id: "identity",
      surfaces: [
        {
          area: "data",
          component: TestPage,
          label: "Identity",
          path: "/data/identity",
        },
      ],
    });

    expect(
      selectDefaultConsoleRoute(
        buildConsoleRoutes([storyModule, identityModule])
      )
    ).toMatchObject({
      moduleId: "platform-story",
      path: "/runtime/stories",
    });
  });

  test("rejects an empty default route registry", () => {
    expect(() => selectDefaultConsoleRoute([])).toThrow(
      "No console module routes are registered"
    );
  });

  test("loads build-time module metadata through installed package registry", () => {
    expect(consoleModulePackageReferences).toEqual([
      {
        area: "runtime",
        exportName: "storyConsoleModule",
        icon: "workflow",
        label: "Stories",
        moduleName: "platform-story",
        navigation: null,
        packageName: "@lenso/story-console",
        route: "/runtime/stories",
        surfaceName: "stories",
      },
      {
        area: "data",
        exportName: "identityConsoleModule",
        icon: "database",
        label: "Identity",
        moduleName: "identity",
        navigation: {
          order: 60,
          workspace: {
            icon: "database",
            id: "identity",
            label: "Identity",
          },
        },
        packageName: "@lenso/identity-console",
        route: "/data/identity",
        surfaceName: "identity",
      },
      {
        area: "data",
        exportName: "remoteCrmConsoleModule",
        icon: "network",
        label: "Remote CRM",
        moduleName: "remote-crm",
        navigation: {
          order: 70,
          workspace: {
            icon: "network",
            id: "remote-crm",
            label: "Remote CRM",
          },
        },
        packageName: "@lenso/remote-crm-console",
        route: "/data/remote-crm",
        surfaceName: "remote-crm",
      },
    ]);
    expect(consoleModules.map((module) => module.id)).toContain(
      "platform-story"
    );
    expect(consoleModules.map((module) => module.id)).toContain("identity");
    expect(
      buildConsoleRoutes(consoleModules).map((route) => ({
        navigation: route.navigation,
        path: route.path,
      }))
    ).toEqual([
      {
        navigation: undefined,
        path: "/runtime/stories",
      },
      {
        navigation: {
          order: 60,
          workspace: {
            icon: "database",
            id: "identity",
            label: "Identity",
          },
        },
        path: "/data/identity",
      },
      {
        navigation: {
          order: 70,
          workspace: {
            icon: "network",
            id: "remote-crm",
            label: "Remote CRM",
          },
        },
        path: "/data/remote-crm",
      },
    ]);
    expect(
      buildConsoleNavigation(consoleModules).map((item) => ({
        navigation: item.navigation,
        path: item.path,
      }))
    ).toEqual([
      {
        navigation: {
          order: -10,
          workspace: {
            icon: "settings",
            id: "system",
            label: "System",
          },
        },
        path: "/runtime/stories",
      },
      {
        navigation: {
          order: 60,
          workspace: {
            icon: "database",
            id: "identity",
            label: "Identity",
          },
        },
        path: "/data/identity",
      },
      {
        navigation: {
          order: 70,
          workspace: {
            icon: "network",
            id: "remote-crm",
            label: "Remote CRM",
          },
        },
        path: "/data/remote-crm",
      },
    ]);
    expect(defaultConsoleRoute).toMatchObject({
      moduleId: "platform-story",
      path: "/runtime/stories",
    });
  });

  test("build-time module metadata creates switchable workspaces", () => {
    expect(
      buildWorkspaceNavigation(buildConsoleNavigation(consoleModules)).map(
        (workspace) => ({
          id: workspace.id,
          items: workspace.items.map((item) => item.path),
          label: workspace.label,
        })
      )
    ).toEqual([
      {
        id: "system",
        items: ["/runtime/stories"],
        label: "System",
      },
      {
        id: "identity",
        items: ["/data/identity"],
        label: "Identity",
      },
      {
        id: "remote-crm",
        items: ["/data/remote-crm"],
        label: "Remote CRM",
      },
    ]);
  });

  test("derives fallback metadata from a package manifest", () => {
    expect(
      consoleModuleMetadataFromManifest({
        area: "data",
        exportName: "billingConsoleModule",
        icon: "database",
        id: "billing",
        label: "Billing",
        navigation: {
          order: 10,
          workspace: {
            icon: "database",
            id: "billing",
            label: "Billing",
          },
        },
        packageName: "@lenso/billing-console",
        requiredCapabilities: ["billing.read"],
        route: "/data/billing",
        source: "installed",
        surfaceName: "billing",
        version: "workspace",
      })
    ).toEqual({
      console: [
        {
          area: "data",
          icon: "database",
          label: "Billing",
          name: "billing",
          navigation: {
            order: 10,
            workspace: {
              icon: "database",
              id: "billing",
              label: "Billing",
            },
          },
          package: {
            export: "billingConsoleModule",
            name: "@lenso/billing-console",
          },
          required_capabilities: ["billing.read"],
          route: "/data/billing",
        },
      ],
      module_name: "billing",
    });
  });
});
