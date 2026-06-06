import { defineConsoleModule } from "@lenso/runtime-console/console-package-api";

import { exampleConsoleManifest } from "./manifest";
import { ExampleConsolePage } from "./page";

export const exampleConsoleModule = defineConsoleModule({
  id: exampleConsoleManifest.id,
  surfaces: [
    {
      area: exampleConsoleManifest.area,
      component: ExampleConsolePage,
      icon: exampleConsoleManifest.icon,
      label: exampleConsoleManifest.label,
      path: exampleConsoleManifest.route,
    },
  ],
});

export { exampleConsoleManifest } from "./manifest";
export { ExampleConsolePage } from "./page";
