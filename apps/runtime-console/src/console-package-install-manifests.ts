import { exampleConsoleManifest } from "@lenso/example-console";
import { storyConsoleManifest } from "@lenso/story-console";

import type { ConsolePackageInstallDeclaration } from "./app/console-package-registry";

export const consolePackageInstallManifests = [
  {
    manifest: storyConsoleManifest,
    source: "first_party",
    version: "workspace",
  },
  {
    manifest: exampleConsoleManifest,
    source: "installed",
    version: "workspace",
  },
] satisfies ConsolePackageInstallDeclaration[];
