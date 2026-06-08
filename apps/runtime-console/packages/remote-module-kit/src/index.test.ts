import { describe, expect, test } from "vitest";

import {
  booleanField,
  defineRemoteModule,
  defineSchemaEntity,
  getRoute,
  integerField,
  jsonField,
  postRoute,
  runtimeFunction,
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

  test("defines HTTP route declarations", () => {
    expect(
      defineRemoteModule({
        httpRoutes: [
          getRoute("/contacts/{id}", {
            capability: "crm.contacts.read",
            displayName: "Fetch Contact",
            storyTitle: "Fetch Contact",
          }),
        ],
        name: "crm",
      })
    ).toMatchObject({
      http_routes: [
        {
          capability: "crm.contacts.read",
          display_name: "Fetch Contact",
          method: "GET",
          path: "/contacts/{id}",
          story_title: "Fetch Contact",
        },
      ],
    });
  });

  test("defines runtime function declarations", () => {
    expect(
      defineRemoteModule({
        name: "crm",
        runtimeFunctions: [
          runtimeFunction("crm.contacts.enrich.v1", {
            inputSchema: "crm.contacts.enrich.v1",
            queue: "crm",
            retryPolicy: {
              initial_delay_ms: 1000,
              max_attempts: 3,
            },
            version: 1,
          }),
        ],
      })
    ).toMatchObject({
      runtime: {
        functions: [
          {
            input_schema: "crm.contacts.enrich.v1",
            name: "crm.contacts.enrich.v1",
            queue: "crm",
            retry_policy: {
              initial_delay_ms: 1000,
              max_attempts: 3,
            },
            version: 1,
          },
        ],
      },
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

  test("serves declared HTTP routes with params and request body", async () => {
    const manifest = defineRemoteModule({
      httpRoutes: [
        getRoute("/contacts/{id}", { capability: "crm.contacts.read" }),
        postRoute("/contacts", { capability: "crm.contacts.write" }),
      ],
      name: "crm",
    });
    const served = await serveRemoteModule(manifest, {
      http: {
        "GET /contacts/{id}": ({ params }) => ({
          email: "ada@example.com",
          id: params.id,
        }),
        "POST /contacts": ({ body }) => ({
          body: { contact: body },
          statusCode: 201,
        }),
      },
      port: 0,
    });
    try {
      await expect(
        fetch(`${served.baseUrl}/contacts/contact_1`).then((response) =>
          response.json()
        )
      ).resolves.toEqual({
        email: "ada@example.com",
        id: "contact_1",
      });
      const createResponse = await fetch(`${served.baseUrl}/contacts`, {
        body: JSON.stringify({ email: "grace@example.com" }),
        headers: { "content-type": "application/json" },
        method: "POST",
      });
      expect(createResponse.status).toBe(201);
      await expect(createResponse.json()).resolves.toEqual({
        contact: { email: "grace@example.com" },
      });
    } finally {
      await served.close();
    }
  });

  test("serves runtime function invocations", async () => {
    const manifest = defineRemoteModule({
      name: "crm",
      runtimeFunctions: [runtimeFunction("crm.contacts.enrich.v1")],
    });
    const served = await serveRemoteModule(manifest, {
      port: 0,
      runtime: {
        "crm.contacts.enrich.v1": ({ input, invocation }) => ({
          enriched: true,
          function_run_id: invocation.function_run_id,
          input,
        }),
      },
    });
    try {
      await expect(
        fetch(
          `${served.baseUrl}/runtime/functions/crm.contacts.enrich.v1/invoke`,
          {
            body: JSON.stringify({
              actor: { id: "worker", kind: "service", scopes: [] },
              attempt: 1,
              correlation_id: "corr_1",
              function_name: "crm.contacts.enrich.v1",
              function_run_id: "fnrun_1",
              input: { contact_id: "contact_1" },
              request_id: "req_1",
              trace: { span_id: "span_1", trace_id: "trace_1" },
            }),
            headers: { "content-type": "application/json" },
            method: "POST",
          }
        ).then((response) => response.json())
      ).resolves.toEqual({
        output: {
          enriched: true,
          function_run_id: "fnrun_1",
          input: { contact_id: "contact_1" },
        },
      });
    } finally {
      await served.close();
    }
  });
});
