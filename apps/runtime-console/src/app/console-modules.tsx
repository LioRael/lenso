import { storyConsoleModule } from "../modules/story-console";
import type {
  ConsoleModule,
  ConsoleNavigationItem,
  ConsoleRouteContribution,
} from "./console-module-api";

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

export const consoleModules = [storyConsoleModule];

export const consoleRoutes = buildConsoleRoutes(consoleModules);
export const consoleNavigation = buildConsoleNavigation(consoleModules);
export const [runtimeStoriesConsoleRoute] = consoleRoutes;
