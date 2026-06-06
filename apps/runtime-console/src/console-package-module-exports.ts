import {
  identityConsoleManifest,
  identityConsoleModule,
} from "@lenso/identity-console";
import { storyConsoleManifest, storyConsoleModule } from "@lenso/story-console";

import {
  consolePackageKey,
  type ConsolePackageModuleExportsByKey,
} from "./app/console-package-registry";

export const consolePackageModuleExportsByKey = {
  [consolePackageKey(identityConsoleManifest)]: identityConsoleModule,
  [consolePackageKey(storyConsoleManifest)]: storyConsoleModule,
} satisfies ConsolePackageModuleExportsByKey;
