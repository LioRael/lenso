import { describe, expect, test } from "vitest";

import {
  IdentityConsolePage,
  identityConsoleManifest,
  identityConsoleModule,
} from ".";

describe("identity console package", () => {
  test("declares an installable identity console package export", () => {
    expect(identityConsoleManifest).toMatchObject({
      exportName: "identityConsoleModule",
      navigation: {
        order: 60,
        workspace: {
          icon: "database",
          id: "identity",
          label: "Identity",
        },
      },
      packageName: "@lenso/identity-console",
      requiredCapabilities: ["identity.users.read"],
      route: "/data/identity",
      source: "installed",
      surfaceName: "identity",
      version: "workspace",
    });
    expect(identityConsoleModule).toMatchObject({
      id: "identity",
      surfaces: [
        {
          label: "Identity",
          navigation: identityConsoleManifest.navigation,
          path: "/data/identity",
        },
      ],
    });
    expect(IdentityConsolePage).toBeTypeOf("function");
  });
});
