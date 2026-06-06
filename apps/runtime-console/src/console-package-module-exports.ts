import {
  exampleConsoleManifest,
  exampleConsoleModule,
} from "@lenso/example-console";
import { storyConsoleManifest, storyConsoleModule } from "@lenso/story-console";

import {
  consolePackageKey,
  type ConsolePackageModuleExportsByKey,
} from "./app/console-package-registry";

export const consolePackageModuleExportsByKey = {
  [consolePackageKey(storyConsoleManifest)]: storyConsoleModule,
  [consolePackageKey(exampleConsoleManifest)]: exampleConsoleModule,
} satisfies ConsolePackageModuleExportsByKey;
