import { describe, expect, test } from "vitest";

import { defineRemoteModule, serveRemoteModule } from ".";

describe("@lenso/remote-module-kit", () => {
  test("defines a serializable remote module manifest", () => {
    expect(
      defineRemoteModule({
        capabilities: ["billing.read"],
        console: [
          {
            area: "data",
            label: "Billing",
            name: "billing",
            package: {
              export: "billingConsoleModule",
              name: "@vendor/lenso-billing-console",
            },
            required_capabilities: ["billing.read"],
            route: "/data/billing",
          },
        ],
        name: "billing",
      })
    ).toEqual({
      admin: null,
      capabilities: ["billing.read"],
      console: [
        {
          area: "data",
          label: "Billing",
          name: "billing",
          package: {
            export: "billingConsoleModule",
            name: "@vendor/lenso-billing-console",
          },
          required_capabilities: ["billing.read"],
          route: "/data/billing",
        },
      ],
      http_routes: [],
      name: "billing",
      runtime: {
        functions: [],
      },
      source: "remote",
      version: "0.1.0",
    });
  });

  test("serves the manifest through the remote module protocol", async () => {
    const manifest = defineRemoteModule({ name: "billing" });
    const served = await serveRemoteModule(manifest, { port: 0 });
    try {
      await expect(
        fetch(served.manifestUrl).then((response) => response.json())
      ).resolves.toMatchObject({
        name: "billing",
        source: "remote",
      });
      await expect(
        fetch(`${served.baseUrl}/missing`).then((response) => response.json())
      ).resolves.toMatchObject({
        error: {
          code: "not_found",
        },
      });
    } finally {
      await served.close();
    }
  });
});
