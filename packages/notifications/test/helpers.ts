import { Database } from "bun:sqlite";
import { drizzle } from "drizzle-orm/bun-sqlite";
import { z } from "zod";
import { createSqliteNotificationStore } from "../src/sqlite";
import { createResendChannel } from "../src/resend";
import { createNotificationService, type NotificationServiceOptions } from "../src/service";
import type { NotificationInput, NotificationTemplate } from "../src/contracts";

export const template: NotificationTemplate = {
  id: "order-status",
  version: "1",
  category: "orders",
  necessity: "required",
  channels: ["email"],
  from: "sender@example.test",
  variables: z.strictObject({ name: z.string().max(1000) }),
  subject: "Hello {{name}}",
  text: "Order for {{name}}\nThank you",
};

export const input: NotificationInput = {
  tenantId: "tenant-a",
  scope: "order.confirmed",
  idempotencyKey: "order-1",
  businessId: "order-1",
  recipientId: "alice",
  email: "alice@example.test",
  templateId: template.id,
  templateVersion: template.version,
  variables: { name: "Alice" },
};

export async function localFixture(
  handler: (request: Request) => Response | Promise<Response> = () =>
    Response.json({ id: "provider-1" }),
  options: Partial<NotificationServiceOptions> & { timeoutMs?: number } = {},
) {
  const client = new Database(":memory:");
  try {
    client.exec(
      await Bun.file(
        new URL("../migrations/sqlite/0001_notifications.sql", import.meta.url),
      ).text(),
    );
    const store = createSqliteNotificationStore(drizzle(client));
    const server = Bun.serve({ port: 0, hostname: "127.0.0.1", fetch: handler });
    const channel = createResendChannel({
      id: "email",
      apiKey: "local-fixture-key",
      endpoint: `http://127.0.0.1:${server.port}/emails`,
      timeoutMs: options.timeoutMs ?? 1000,
    });
    try {
      const service = createNotificationService({
        store,
        templates: [template],
        channels: [channel],
        ...options,
      });
      return {
        client,
        store,
        channel,
        service,
        close() {
          server.stop(true);
          client.close();
        },
      };
    } catch (error) {
      server.stop(true);
      throw error;
    }
  } catch (error) {
    client.close();
    throw error;
  }
}
