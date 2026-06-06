import type {
  ConsoleSurfaceArea,
  ConsoleSurfaceIcon,
} from "../../../src/app/console-module-api";
import type { ConsolePackageRegistrySource } from "../../../src/app/console-package-registry";

export {
  defineConsoleModule,
  type ConsoleModule,
  type ConsoleModuleSurface,
  type ConsoleNavigationItem,
  type ConsoleRouteContribution,
  type ConsoleSurfaceArea,
  type ConsoleSurfaceIcon,
} from "../../../src/app/console-module-api";
export {
  runtimeConsoleHostApi,
  type RuntimeConsoleHostApi,
  type ExecutionInspectorTab,
  type ExecutionNode,
  type RuntimeStory,
  type StoryViewMode,
} from "../../../src/app/console-host-api";

export interface ConsolePackageManifest {
  id: string;
  packageName: string;
  exportName: string;
  surfaceName: string;
  label: string;
  area: ConsoleSurfaceArea;
  route: string;
  requiredCapabilities: readonly string[];
  source: ConsolePackageRegistrySource;
  version?: string;
  icon?: ConsoleSurfaceIcon;
}

export const defineConsolePackageManifest = <
  Manifest extends ConsolePackageManifest,
>(
  manifest: Manifest
): Manifest => manifest;
