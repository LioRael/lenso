import { defineConsoleModule } from "@lenso/runtime-console-api";

import { remoteCrmConsoleManifest } from "./manifest";
import { RemoteCrmConsolePage } from "./page";

export const remoteCrmConsoleModule = defineConsoleModule({
  id: remoteCrmConsoleManifest.id,
  surfaces: [
    {
      area: remoteCrmConsoleManifest.area,
      component: RemoteCrmConsolePage,
      icon: remoteCrmConsoleManifest.icon,
      label: remoteCrmConsoleManifest.label,
      navigation: remoteCrmConsoleManifest.navigation,
      path: remoteCrmConsoleManifest.route,
    },
  ],
});

export { remoteCrmConsoleManifest } from "./manifest";
export { RemoteCrmConsolePage } from "./page";
