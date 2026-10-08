import { expect, test } from "bun:test";
import { SQL } from "bun";
import { drizzle } from "drizzle-orm/bun-sql";
import { createPostgresNotificationStore } from "../src/postgres";
import { createNotificationService } from "../src/service";
import { input, template } from "./helpers";

// Never fall back to DATABASE_URL. Both values explicitly designate a disposable test database.
const enabled = process.env.LENSO_NOTIFICATIONS_PG_TEST === "1";
const url = process.env.LENSO_NOTIFICATIONS_TEST_DATABASE_URL;

test.skipIf(!enabled || !url)(
  "PostgreSQL migration, competing connections, CAS and private JSON snapshot",
  async () => {
    const schema = `notifications_test_${crypto.randomUUID().replaceAll("-", "")}`;
    const first = new SQL(url!, { max: 1 });
    const second = new SQL(url!, { max: 1 });
    let created = false;
    try {
      await first.unsafe(`CREATE SCHEMA "${schema}"`);
      created = true;
      await first.unsafe(`SET search_path TO "${schema}"`);
      await second.unsafe(`SET search_path TO "${schema}"`);
      const migration = await Bun.file(
        new URL("../migrations/pg/0001_notifications.sql", import.meta.url),
      ).text();
      await first.unsafe(migration).simple();
      const stores = [
        createPostgresNotificationStore(drizzle(first)),
        createPostgresNotificationStore(drizzle(second)),
      ];
      const services = stores.map((store) =>
        createNotificationService({
          store,
          templates: [template],
          channels: [
            {
              id: "email",
              kind: "email",
              idempotencyWindowMs: 86_400_000,
              // No hosted provider is called by this persistence test.
              send: async () => ({ state: "accepted", providerMessageId: "local-test" }),
            },
          ],
        }),
      );
      const winners = await Promise.all(
        Array.from({ length: 10 }, (_, index) => services[index % 2].create(input)),
      );
      expect(new Set(winners.map((record) => record.id)).size).toBe(1);
      const id = winners[0].id;
      expect((await stores[0].get(id))?.message.text).toContain("Alice");
      await expect(
        services[1].create({ ...input, email: "other@example.test" }),
      ).rejects.toMatchObject({ code: "idempotency-conflict" });
      const original = (await stores[0].get(id))!;
      const next = {
        ...original,
        revision: 1,
        attemptCount: 1,
        state: "sending" as const,
        firstRequestAt: Date.now(),
      };
      const saves = await Promise.all(
        stores.map((store) =>
          store.save(next, 0, {
            id: crypto.randomUUID(),
            notificationId: id,
            number: 1,
            state: "sending",
            startedAt: Date.now(),
            finishedAt: null,
            providerMessageId: null,
            error: null,
          }),
        ),
      );
      expect(saves.filter(Boolean)).toHaveLength(1);
      expect(await stores[0].attempts(id)).toHaveLength(1);
      expect(await stores[0].markEnqueued(id, "test-job")).toBe(true);
      expect(await stores[1].markEnqueued(id, "other-job")).toBe(false);
      expect((await stores[1].get(id))?.revision).toBe(1);
      expect(await stores[0].recoverable(Date.now(), 100)).toEqual([]);
      const currentAttempt = (await stores[0].attempts(id))[0];
      expect(
        await stores[1].save({ ...next, revision: 2, state: "accepted" }, 1, {
          ...currentAttempt,
          state: "accepted",
          providerMessageId: "local-test",
          finishedAt: Date.now(),
        }),
      ).toBe(true);
      await expect(
        stores[0].save({ ...next, revision: 3 }, 2, {
          ...currentAttempt,
          id: crypto.randomUUID(),
        }),
      ).rejects.toThrow();
      expect((await stores[0].get(id))?.revision).toBe(2);
      expect((await stores[0].get(id))?.taskJobId).toBe("test-job");
    } finally {
      try {
        if (created) await first.unsafe(`DROP SCHEMA "${schema}" CASCADE`);
      } finally {
        await Promise.all([first.close(), second.close()]);
      }
    }
  },
);
