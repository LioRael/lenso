import { describe, expect, test } from "vitest";

import {
  booleanField,
  defineRemoteModule,
  defineSchemaEntity,
  integerField,
  jsonField,
  schemaAdmin,
  serveRemoteModule,
  textField,
  timestampField,
} from ".";

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

  test("defines schema-admin entities and serves list/detail data", async () => {
    const contacts = defineSchemaEntity({
      fields: [
        textField("email"),
        textField("name", { label: "Full name" }),
        integerField("score", { nullable: true }),
        booleanField("active"),
        timestampField("created_at"),
        jsonField("metadata"),
      ],
      label: "Contacts",
      name: "contacts",
      readCapability: "crm.contacts.read",
    });
    const manifest = defineRemoteModule({
      admin: schemaAdmin([contacts]),
      capabilities: ["crm.contacts.read"],
      name: "crm",
    });
    expect(manifest.admin).toMatchObject({
      entities: [
        {
          fields: [
            {
              field_type: { kind: "string" },
              label: "Email",
              name: "email",
              nullable: false,
            },
            {
              field_type: { kind: "string" },
              label: "Full name",
              name: "name",
            },
            {
              field_type: { kind: "integer" },
              name: "score",
              nullable: true,
            },
            { field_type: { kind: "boolean" }, name: "active" },
            { field_type: { kind: "timestamp" }, name: "created_at" },
            { field_type: { kind: "json" }, name: "metadata" },
          ],
          name: "contacts",
          read_capability: "crm.contacts.read",
        },
      ],
      kind: "schema",
    });

    const served = await serveRemoteModule(manifest, {
      data: {
        contacts: {
          detail: (id) =>
            id === "contact_1" ? { email: "ada@example.com", id } : null,
          list: ({ limit }) => ({
            next_cursor: null,
            records: [{ email: "ada@example.com", limit }],
          }),
        },
      },
      port: 0,
    });
    try {
      await expect(
        fetch(`${served.baseUrl}/admin/contacts?limit=2`).then((response) =>
          response.json()
        )
      ).resolves.toEqual({
        next_cursor: null,
        records: [{ email: "ada@example.com", limit: 2 }],
      });
      await expect(
        fetch(`${served.baseUrl}/admin/contacts/contact_1`).then((response) =>
          response.json()
        )
      ).resolves.toEqual({
        record: { email: "ada@example.com", id: "contact_1" },
      });
    } finally {
      await served.close();
    }
  });
});
