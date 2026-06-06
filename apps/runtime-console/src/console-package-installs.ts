import {
  exampleConsoleManifest,
  exampleConsoleModule,
} from "@lenso/example-console";
import { storyConsoleManifest, storyConsoleModule } from "@lenso/story-console";

import { defineInstalledConsolePackage } from "./app/console-package-registry";

export const installedConsolePackages = [
  defineInstalledConsolePackage({
    manifest: storyConsoleManifest,
    module: storyConsoleModule,
    source: "first_party",
    version: "workspace",
  }),
  defineInstalledConsolePackage({
    manifest: exampleConsoleManifest,
    module: exampleConsoleModule,
    source: "installed",
    version: "workspace",
  }),
];
