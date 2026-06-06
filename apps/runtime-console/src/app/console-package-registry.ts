import { storyConsoleModule } from "../modules/story-console";
import type { ConsoleModule } from "./console-module-api";

export type ConsolePackageRegistrySource = "first_party" | "installed";

export type InstalledConsolePackage = {
  packageName: string;
  exportName: string;
  module: ConsoleModule;
  source: ConsolePackageRegistrySource;
  version?: string;
};

export function consolePackageKey({
  exportName,
  packageName,
}: {
  packageName: string;
  exportName: string;
}): string {
  return `${packageName}#${exportName}`;
}

export const installedConsolePackages = [
  {
    exportName: "storyConsoleModule",
    module: storyConsoleModule,
    packageName: "@lenso/story-console",
    source: "first_party",
    version: "workspace",
  },
] satisfies InstalledConsolePackage[];

export function consolePackageRegistryByKey(
  packages: readonly InstalledConsolePackage[] = installedConsolePackages
): Record<string, InstalledConsolePackage> {
  return Object.fromEntries(
    packages.map((item) => [consolePackageKey(item), item])
  );
}
