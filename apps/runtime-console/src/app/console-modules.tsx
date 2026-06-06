import { exampleConsoleManifest } from "../modules/example-console-package";
import { storyConsoleManifest } from "../modules/story-console";
import type {
  ConsoleModule,
  ConsoleNavigationItem,
  ConsoleRouteContribution,
} from "./console-module-api";
import {
  resolveConsoleModules,
  selectConsoleModulePackageReferences,
} from "./console-module-resolver";

export { defineConsoleModule } from "./console-module-api";
export type {
  ConsoleModule,
  ConsoleNavigationItem,
  ConsoleRouteContribution,
  ConsoleSurfaceArea,
  ConsoleSurfaceIcon,
  ConsoleModuleSurface,
} from "./console-module-api";

export function buildConsoleRoutes(
  modules: ConsoleModule[]
): ConsoleRouteContribution[] {
  const seenPaths = new Set<string>();
  const routes: ConsoleRouteContribution[] = [];

  for (const module of modules) {
    for (const surface of module.surfaces) {
      if (seenPaths.has(surface.path)) {
        throw new Error(`Duplicate console module route: ${surface.path}`);
      }
      seenPaths.add(surface.path);
      routes.push({
        ...surface,
        moduleId: module.id,
      });
    }
  }

  return routes;
}

export function buildConsoleNavigation(
  modules: ConsoleModule[]
): ConsoleNavigationItem[] {
  return buildConsoleRoutes(modules).map((route) => {
    const item: ConsoleNavigationItem = {
      label: route.label,
      moduleId: route.moduleId,
      path: route.path,
    };
    if (route.icon) {
      item.icon = route.icon;
    }
    return item;
  });
}

export const buildTimeConsoleModuleMetadata = [
  {
    console: [
      {
        label: storyConsoleManifest.label,
        name: storyConsoleManifest.surfaceName,
        package: {
          export: storyConsoleManifest.exportName,
          name: storyConsoleManifest.packageName,
        },
        required_capabilities: storyConsoleManifest.requiredCapabilities,
        route: storyConsoleManifest.route,
      },
    ],
    module_name: storyConsoleManifest.id,
  },
  {
    console: [
      {
        label: exampleConsoleManifest.label,
        name: exampleConsoleManifest.surfaceName,
        package: {
          export: exampleConsoleManifest.exportName,
          name: exampleConsoleManifest.packageName,
        },
        required_capabilities: exampleConsoleManifest.requiredCapabilities,
        route: exampleConsoleManifest.route,
      },
    ],
    module_name: exampleConsoleManifest.id,
  },
];

export const consoleModulePackageReferences =
  selectConsoleModulePackageReferences(buildTimeConsoleModuleMetadata);

export const consoleModules = resolveConsoleModules(
  consoleModulePackageReferences
);

export const consoleRoutes = buildConsoleRoutes(consoleModules);
export const consoleNavigation = buildConsoleNavigation(consoleModules);
export const [runtimeStoriesConsoleRoute] = consoleRoutes;
