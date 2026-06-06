import { describe, expect, test } from "vitest";

import { identityConsoleManifest, identityConsoleModule } from ".";

describe("identity console package", () => {
  test("declares an installable identity console package export", () => {
    expect(identityConsoleManifest).toMatchObject({
      exportName: "identityConsoleModule",
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
          path: "/data/identity",
        },
      ],
    });
  });
});
