import { defineConsolePackageManifest } from "@lenso/runtime-console-api";

import consoleSurface from "../console-surface.json";

const consoleSurfaceContract = consoleSurface as unknown as {
  readonly area: "data";
  readonly exportName: "identityConsoleModule";
  readonly icon: "database";
  readonly id: "identity";
  readonly label: "Identity";
  readonly navigation: {
    readonly order: 60;
    readonly workspace: {
      readonly icon: "database";
      readonly id: "identity";
      readonly label: "Identity";
    };
  };
  readonly packageName: "@lenso/identity-console";
  readonly requiredCapabilities: readonly ["identity.users.read"];
  readonly route: "/data/identity";
  readonly source: "installed";
  readonly surfaceName: "identity";
  readonly version: "workspace";
};

export const identityConsoleManifest = defineConsolePackageManifest(
  consoleSurfaceContract
);
