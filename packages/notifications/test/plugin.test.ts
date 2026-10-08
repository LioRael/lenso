import { expect, it } from "bun:test";
import { definePlugin, startApp } from "@lenso/core";
import { createNotificationPlugin } from "../src/plugin";
import { createSqliteNotificationStore } from "../src/sqlite";
import { drizzle } from "drizzle-orm/bun-sqlite";
import { input, localFixture, template } from "./helpers";

it("uses exact resource instances and leaves borrowed resources open at shutdown", async () => {
  const fixture = await localFixture();
  const database = definePlugin({ id: "notification-db", setup: () => drizzle(fixture.client) });
  const channel = definePlugin({ id: "notification-email", setup: () => fixture.channel });
  const notifications = createNotificationPlugin({
    id: "notifications",
    database,
    channels: [channel],
    store: createSqliteNotificationStore,
    templates: [template],
  });
  try {
    expect(notifications.requires).toEqual([database, channel]);
    const app = await startApp({ plugins: [database, channel, notifications] });
    try {
      const service = app.get(notifications);
      const created = await service.create(input);
      expect((await service.deliver(created.id))?.state).toBe("accepted");
    } finally {
      await app.stop();
    }
    expect(fixture.client.query("SELECT count(*) AS count FROM lenso_notifications").get()).toEqual(
      { count: 1 },
    );
    expect(
      (
        await fixture.channel.send(
          {
            from: template.from,
            to: input.email,
            subject: "Still open",
            text: "Text",
            html: "Text",
          },
          { idempotencyKey: "borrowed-resource-test" },
        )
      ).state,
    ).toBe("accepted");
  } finally {
    fixture.close();
  }
});

it("rejects invalid runtime configuration before any resource setup", async () => {
  let setups = 0;
  const database = definePlugin({
    id: "db",
    setup() {
      setups++;
      throw new Error("Should not run");
    },
  });
  const notifications = createNotificationPlugin({
    id: "notifications",
    database,
    channels: [],
    store: () => {
      throw new Error("Should not run");
    },
    templates: [],
    config: { leaseMs: -1 },
  });
  await expect(startApp({ plugins: [database, notifications] })).rejects.toBeDefined();
  expect(setups).toBe(0);
});
