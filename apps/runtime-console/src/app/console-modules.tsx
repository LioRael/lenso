import { exampleConsoleManifest } from "@lenso/example-console";
import {
  consoleSurfaceFromPackageManifest,
  type ConsolePackageManifest,
} from "@lenso/runtime-console-api";
import { storyConsoleManifest } from "@lenso/story-console";

import type {
  ConsoleModule,
  ConsoleNavigationItem,
  ConsoleRouteContribution,
} from "./console-module-api";
import {
  type ConsoleModuleMetadata,
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

export function consoleModuleMetadataFromManifest(
  manifest: ConsolePackageManifest
): ConsoleModuleMetadata {
  return {
    console: [consoleSurfaceFromPackageManifest(manifest)],
    module_name: manifest.id,
  };
}

export const buildTimeConsoleModuleMetadata = [
  storyConsoleManifest,
  exampleConsoleManifest,
].map(consoleModuleMetadataFromManifest);

export const consoleModulePackageReferences =
  selectConsoleModulePackageReferences(buildTimeConsoleModuleMetadata);

export const consoleModules = resolveConsoleModules(
  consoleModulePackageReferences
);

export const consoleRoutes = buildConsoleRoutes(consoleModules);
export const consoleNavigation = buildConsoleNavigation(consoleModules);
export const [runtimeStoriesConsoleRoute] = consoleRoutes;
