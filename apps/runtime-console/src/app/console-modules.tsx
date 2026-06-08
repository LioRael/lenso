import { identityConsoleManifest } from "@lenso/identity-console";
import { remoteCrmConsoleManifest } from "@lenso/remote-crm-console";
import {
  consoleSurfaceFromPackageManifest,
  type ConsolePackageManifest,
} from "@lenso/runtime-console-api";
import { storyConsoleManifest } from "@lenso/story-console";

import type {
  ConsoleModule,
  ConsoleNavigationItem,
  ConsoleRouteContribution,
  ConsoleSurfaceArea,
} from "./console-module-api";
import {
  type ConsoleModuleMetadata,
  resolveConsoleModules,
  selectConsoleModulePackageReferences,
} from "./console-module-resolver";
import { SYSTEM_WORKSPACE } from "./console-workspace-navigation";

export { defineConsoleModule } from "./console-module-api";
export type {
  ConsoleModule,
  ConsoleNavigationItem,
  ConsoleNavigationMetadata,
  ConsoleNavigationGroup,
  ConsoleRouteContribution,
  ConsoleSurfaceArea,
  ConsoleSurfaceIcon,
  ConsoleWorkspaceRef,
  ConsoleModuleSurface,
} from "./console-module-api";

export function buildConsoleRoutes(
  modules: ConsoleModule[]
): ConsoleRouteContribution[] {
  const seenPaths = new Set<string>();
  const routes: ConsoleRouteContribution[] = [];

  for (const module of modules) {
    for (const surface of module.surfaces) {
      if (isReservedHostConsoleRoute(surface.path)) {
        throw new Error(`Reserved host console route: ${surface.path}`);
      }
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
    item.navigation =
      route.navigation ?? defaultSystemNavigationForArea(route.area);
    return item;
  });
}

export function selectDefaultConsoleRoute(
  routes: ConsoleRouteContribution[]
): ConsoleRouteContribution {
  const [route] = routes;
  if (!route) {
    throw new Error("No console module routes are registered");
  }
  return route;
}

const RESERVED_HOST_CONSOLE_ROUTE_PATHS = new Set([
  "/overview",
  "/operations",
  "/operations/queues",
  "/operations/dead-letters",
  "/operations/functions",
  "/operations/remote-calls",
  "/operations/admin-actions",
  "/modules",
  "/config",
  "/data",
]);

const DEFAULT_SYSTEM_NAVIGATION_ORDER = {
  runtime: -10,
  operations: 80,
  data: 100,
  configuration: 120,
} satisfies Record<ConsoleSurfaceArea, number>;

function isReservedHostConsoleRoute(path: string): boolean {
  return RESERVED_HOST_CONSOLE_ROUTE_PATHS.has(path);
}

function defaultSystemNavigationForArea(area: ConsoleSurfaceArea) {
  return {
    order: defaultSystemNavigationOrder(area),
    workspace: SYSTEM_WORKSPACE,
  };
}

function defaultSystemNavigationOrder(area: ConsoleSurfaceArea): number {
  return DEFAULT_SYSTEM_NAVIGATION_ORDER[area];
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
  identityConsoleManifest,
  remoteCrmConsoleManifest,
].map(consoleModuleMetadataFromManifest);

export const consoleModulePackageReferences =
  selectConsoleModulePackageReferences(buildTimeConsoleModuleMetadata);

export const consoleModules = resolveConsoleModules(
  consoleModulePackageReferences
);

export const consoleRoutes = buildConsoleRoutes(consoleModules);
export const consoleNavigation = buildConsoleNavigation(consoleModules);
