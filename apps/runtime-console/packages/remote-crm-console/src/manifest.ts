import { defineConsolePackageManifest } from "@lenso/runtime-console-api";

import consoleSurface from "../console-surface.json";

const consoleSurfaceContract = consoleSurface as unknown as {
  readonly area: "data";
  readonly exportName: "remoteCrmConsoleModule";
  readonly icon: "network";
  readonly id: "remote-crm";
  readonly label: "Remote CRM";
  readonly navigation: {
    readonly order: 70;
    readonly workspace: {
      readonly icon: "network";
      readonly id: "remote-crm";
      readonly label: "Remote CRM";
    };
  };
  readonly packageName: "@lenso/remote-crm-console";
  readonly requiredCapabilities: readonly ["remote_crm.contacts.read"];
  readonly route: "/data/remote-crm";
  readonly source: "installed";
  readonly surfaceName: "remote-crm";
  readonly version: "workspace";
};

export const remoteCrmConsoleManifest = defineConsolePackageManifest(
  consoleSurfaceContract
);
