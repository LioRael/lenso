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
        path: "/runtime/stories",
      },
    ]);
    expect(buildConsoleRoutes([module])).toHaveLength(1);
    expect(buildConsoleRoutes([module])[0]?.path).toBe("/runtime/stories");
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
        exportName: "storyConsoleModule",
        packageName: "@lenso/story-console",
      },
      {
        exportName: "identityConsoleModule",
        packageName: "@lenso/identity-console",
      },
    ]);
    expect(consoleModules.map((module) => module.id)).toContain(
      "platform-story"
    );
    expect(consoleModules.map((module) => module.id)).toContain("identity");
    expect(
      buildConsoleRoutes(consoleModules).map((route) => route.path)
    ).toEqual(["/runtime/stories", "/data/identity"]);
    expect(defaultConsoleRoute).toMatchObject({
      moduleId: "platform-story",
      path: "/runtime/stories",
    });
  });

  test("derives fallback metadata from a package manifest", () => {
    expect(
      consoleModuleMetadataFromManifest({
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
      })
    ).toEqual({
      console: [
        {
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
        },
      ],
      module_name: "billing",
    });
  });
});
