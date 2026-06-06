import type { ReactNode } from "react";

export type ConsoleSurfaceArea =
  | "runtime"
  | "operations"
  | "data"
  | "configuration";

export type ConsoleSurfaceIcon =
  | "activity"
  | "boxes"
  | "database"
  | "network"
  | "settings"
  | "workflow";

export type ConsoleModuleSurface = {
  path: string;
  label: string;
  area: ConsoleSurfaceArea;
  component: () => ReactNode;
  icon?: ConsoleSurfaceIcon;
};

export type ConsoleModule = {
  id: string;
  surfaces: ConsoleModuleSurface[];
};

export type ConsoleRouteContribution = ConsoleModuleSurface & {
  moduleId: string;
};

export type ConsoleNavigationItem = {
  path: string;
  label: string;
  moduleId: string;
  icon?: ConsoleSurfaceIcon;
};

export function defineConsoleModule(module: ConsoleModule): ConsoleModule {
  return module;
}
