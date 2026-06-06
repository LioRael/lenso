import type { ConsolePackageInstallDeclaration } from "./app/console-package-registry";
import { consolePackageManifests } from "./console-package-manifest-exports";

export const consolePackageInstallManifests = consolePackageManifests.map(
  (manifest) => ({
    manifest,
    source: manifest.source,
    version: manifest.version,
  })
) satisfies ConsolePackageInstallDeclaration[];
