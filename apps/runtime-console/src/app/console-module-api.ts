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

export type ConsoleNavigationMetadata = {
  workspace: ConsoleWorkspaceRef;
  group?: ConsoleNavigationGroup;
  order?: number;
};

export type ConsoleWorkspaceRef = {
  id: string;
  label: string;
  icon?: string;
};

export type ConsoleNavigationGroup = {
  id: string;
  label: string;
  icon?: string;
  order?: number;
};

export type ConsoleModuleSurface = {
  path: string;
  label: string;
  area: ConsoleSurfaceArea;
  component: () => ReactNode;
  icon?: ConsoleSurfaceIcon;
  navigation?: ConsoleNavigationMetadata;
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
  navigation?: ConsoleNavigationMetadata;
};

export function defineConsoleModule(module: ConsoleModule): ConsoleModule {
  return module;
}
