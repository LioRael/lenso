import type { ConsoleModule } from "./console-module-api";

export type ConsolePackageRegistrySource = "first_party" | "installed";

export type ConsolePackageInstallManifest = {
  packageName: string;
  exportName: string;
};

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

export function defineInstalledConsolePackage({
  manifest,
  module,
  source,
  version,
}: {
  manifest: ConsolePackageInstallManifest;
  module: ConsoleModule;
  source: ConsolePackageRegistrySource;
  version?: string;
}): InstalledConsolePackage {
  const installedPackage: InstalledConsolePackage = {
    exportName: manifest.exportName,
    module,
    packageName: manifest.packageName,
    source,
  };
  if (version) {
    installedPackage.version = version;
  }
  return installedPackage;
}

export function consolePackageRegistryByKey(
  packages: readonly InstalledConsolePackage[]
): Record<string, InstalledConsolePackage> {
  return Object.fromEntries(
    packages.map((item) => [consolePackageKey(item), item])
  );
}
