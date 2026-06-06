import { resolveInstalledConsolePackages } from "./app/console-package-registry";
import { consolePackageInstallManifests } from "./console-package-install-manifests";
import { consolePackageModuleExportsByKey } from "./console-package-module-exports";

export const installedConsolePackages = resolveInstalledConsolePackages(
  consolePackageInstallManifests,
  consolePackageModuleExportsByKey
);
