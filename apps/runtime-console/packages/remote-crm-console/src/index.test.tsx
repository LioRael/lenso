import { describe, expect, test } from "vitest";

import {
  RemoteCrmConsolePage,
  remoteCrmConsoleManifest,
  remoteCrmConsoleModule,
} from ".";

describe("remote crm console package", () => {
  test("declares an installable remote crm console package export", () => {
    expect(remoteCrmConsoleManifest).toMatchObject({
      exportName: "remoteCrmConsoleModule",
      navigation: {
        order: 70,
        workspace: {
          icon: "network",
          id: "remote-crm",
          label: "Remote CRM",
        },
      },
      packageName: "@lenso/remote-crm-console",
      requiredCapabilities: ["remote_crm.contacts.read"],
      route: "/data/remote-crm",
      source: "installed",
      surfaceName: "remote-crm",
      version: "workspace",
    });
    expect(remoteCrmConsoleModule).toMatchObject({
      id: "remote-crm",
      surfaces: [
        {
          label: "Remote CRM",
          navigation: remoteCrmConsoleManifest.navigation,
          path: "/data/remote-crm",
        },
      ],
    });
    expect(RemoteCrmConsolePage).toBeTypeOf("function");
  });
});
