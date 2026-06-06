import { defineConsoleModule } from "@lenso/runtime-console-api";

import { identityConsoleManifest } from "./manifest";
import { IdentityConsolePage } from "./page";

export const identityConsoleModule = defineConsoleModule({
  id: identityConsoleManifest.id,
  surfaces: [
    {
      area: identityConsoleManifest.area,
      component: IdentityConsolePage,
      icon: identityConsoleManifest.icon,
      label: identityConsoleManifest.label,
      path: identityConsoleManifest.route,
    },
  ],
});

export { identityConsoleManifest } from "./manifest";
export { IdentityConsolePage } from "./page";
