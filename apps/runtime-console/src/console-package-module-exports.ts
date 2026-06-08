import {
  identityConsoleManifest,
  identityConsoleModule,
} from "@lenso/identity-console";
import {
  remoteCrmConsoleManifest,
  remoteCrmConsoleModule,
} from "@lenso/remote-crm-console";
import { storyConsoleManifest, storyConsoleModule } from "@lenso/story-console";

import {
  consolePackageKey,
  type ConsolePackageModuleExportsByKey,
} from "./app/console-package-registry";

export const consolePackageModuleExportsByKey = {
  [consolePackageKey(identityConsoleManifest)]: identityConsoleModule,
  [consolePackageKey(remoteCrmConsoleManifest)]: remoteCrmConsoleModule,
  [consolePackageKey(storyConsoleManifest)]: storyConsoleModule,
} satisfies ConsolePackageModuleExportsByKey;
