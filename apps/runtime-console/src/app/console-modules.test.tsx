import { describe, expect, test } from "vitest";

import {
  buildConsoleNavigation,
  buildConsoleRoutes,
  consoleModulePackageReferences,
  consoleModules,
  defineConsoleModule,
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

  test("loads build-time module metadata through installed package registry", () => {
    expect(consoleModulePackageReferences).toEqual([
      {
        exportName: "storyConsoleModule",
        packageName: "@lenso/story-console",
      },
      {
        exportName: "exampleConsoleModule",
        packageName: "@lenso/example-console",
      },
    ]);
    expect(consoleModules.map((module) => module.id)).toContain(
      "platform-story"
    );
    expect(consoleModules.map((module) => module.id)).toContain(
      "example-console"
    );
    expect(
      buildConsoleRoutes(consoleModules).map((route) => route.path)
    ).toEqual(["/runtime/stories", "/runtime/example-console"]);
  });
});
