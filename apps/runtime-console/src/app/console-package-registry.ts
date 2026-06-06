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

export type ConsolePackageInstallDeclaration = {
  manifest: ConsolePackageInstallManifest;
  source: ConsolePackageRegistrySource;
  version?: string;
};

export type ConsolePackageModuleExportsByKey = Record<string, ConsoleModule>;

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
}: ConsolePackageInstallDeclaration & {
  module: ConsoleModule;
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

export function resolveInstalledConsolePackages(
  declarations: readonly ConsolePackageInstallDeclaration[],
  moduleExportsByKey: ConsolePackageModuleExportsByKey
): InstalledConsolePackage[] {
  return declarations.map((declaration) => {
    const key = consolePackageKey(declaration.manifest);
    const module = moduleExportsByKey[key];
    if (!module) {
      throw new Error(`Console package module export is not installed: ${key}`);
    }
    return defineInstalledConsolePackage({
      ...declaration,
      module,
    });
  });
}

export function consolePackageRegistryByKey(
  packages: readonly InstalledConsolePackage[]
): Record<string, InstalledConsolePackage> {
  return Object.fromEntries(
    packages.map((item) => [consolePackageKey(item), item])
  );
}
