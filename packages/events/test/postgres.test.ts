import { expect, test } from "bun:test";
import { SQL } from "bun";
import { createPostgresEventStore, initializePostgresEvents } from "../src/postgres";
import { createAsyncEvents } from "../src/async";

// Explicit disposable database opt-in only. Never read DATABASE_URL or production configuration.
const enabled = process.env.LENSO_EVENTS_PG_TEST === "1";
const url = process.env.LENSO_EVENTS_TEST_DATABASE_URL;
const pgTest = test.skipIf(!enabled || !url);

async function fixture(
  run: (f: {
    db: SQL[];
    admin: SQL;
    schema: string;
    clock: { value: number };
    stores: ReturnType<typeof createPostgresEventStore>[];
  }) => Promise<void>,
  migrate = true,
) {
  const schema = `events_test_${crypto.randomUUID().replaceAll("-", "")}`;
  const admin = new SQL(url!, { max: 1 });
  const db = Array.from(
    { length: 3 },
    (_, i) =>
      new SQL(url!, {
        max: 1,
        connection: { search_path: schema, application_name: `${schema}_${i}` },
      }),
  );
  const clock = { value: 1_000 };
  let created = false;
  try {
    await admin.unsafe(`CREATE SCHEMA "${schema}"`);
    created = true;
    if (migrate) await Promise.all(db.slice(0, 2).map(initializePostgresEvents));
    await run({
      db,
      admin,
      schema,
      clock,
      stores: db.map((client) => createPostgresEventStore(client, { now: () => clock.value })),
    });
  } finally {
    await Promise.all(db.map((client) => client.close()));
    try {
      if (created) await admin.unsafe(`DROP SCHEMA "${schema}" CASCADE`);
    } finally {
      await admin.close();
    }
  }
}

const event = (id: string) => ({
  id,
  topic: "order.created",
  payload: { order: 42, nested: { a: true, b: null } },
});

async function waiting(admin: SQL, application: string): Promise<number> {
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    const [row] = await admin`SELECT pid FROM pg_stat_activity
      WHERE application_name = ${application} AND wait_event_type = 'Lock'`;
    if (row) return row.pid;
    await Bun.sleep(5);
  }
  throw new Error("Expected test connection to wait on its database lock");
}

function gate() {
  let release!: () => void;
  const promise = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { promise, release };
}

pgTest(
  "explicit migration, canonical duplicate publication and atomic fanout rollback",
  async () => {
    await fixture(async ({ db, stores: [a, b], clock }) => {
      await expect(a.publish(event("unmigrated"))).rejects.toThrow();
      await Promise.all(db.slice(0, 2).map(initializePostgresEvents));
      await Promise.all([
        a.register("receipt", "order.created"),
        b.register("receipt", "order.created"),
      ]);
      await b.register("audit", "order.created");
      const published = await Promise.all(
        Array.from({ length: 12 }, (_, i) => (i % 2 ? a : b).publish(event("same"))),
      );
      expect(published.filter((result) => result.created)).toHaveLength(1);
      expect(await a.status("same")).toHaveLength(2);
      expect(
        await b.publish({ ...event("same"), payload: { nested: { b: null, a: true }, order: 42 } }),
      ).toEqual({ created: false });
      await expect(b.publish({ ...event("same"), topic: "other" })).rejects.toThrow("conflicts");
      await expect(b.publish({ ...event("same"), payload: { order: 43 } })).rejects.toThrow(
        "conflicts",
      );
      await a.register("late", "order.created");
      await a.publish(event("same"));
      expect(await a.status("same")).toHaveLength(2);
      // A database failure during fanout rolls back the event identity too.
      await db[0]`ALTER TABLE lenso_event_deliveries ADD CONSTRAINT test_reject CHECK (event_id <> 'rollback')`;
      await expect(a.publish(event("rollback"))).rejects.toThrow();
      expect((await db[0]`SELECT id FROM lenso_events WHERE id = 'rollback'`).length).toBe(0);
      await db[0]`ALTER TABLE lenso_event_deliveries DROP CONSTRAINT test_reject`;
      expect(await b.publish(event("rollback"))).toEqual({ created: true });
      for (const payload of [NaN, { a: undefined }, new Date(), Array(2), "x".repeat(1_048_576)]) {
        await expect(
          a.publish({ ...event("invalid"), payload: payload as never }),
        ).rejects.toThrow();
      }
      expect((await db[0]`SELECT id FROM lenso_events WHERE id = 'invalid'`).length).toBe(0);
      clock.value = NaN;
      await expect(a.publish(event("clock"))).rejects.toThrow("clock");
      expect((await db[0]`SELECT id FROM lenso_events WHERE id = 'clock'`).length).toBe(0);
    }, false);
  },
);

pgTest(
  "concurrent claims, expired leases, fenced settlement and per-subscription retries",
  async () => {
    await fixture(async ({ stores: [a, b], clock, db }) => {
      await a.register("receipt", "order.created");
      await a.register("audit", "order.created");
      await a.publish(event("one"));
      const claims = await Promise.all(
        Array.from({ length: 12 }, (_, i) => (i % 2 ? a : b).claim(["receipt"], 100)),
      );
      const winners = claims.filter((claim) => claim !== null);
      expect(winners).toHaveLength(1);
      const first = winners[0]!;
      const audit = (await b.claim(["audit"], 100))!;
      expect(await b.acknowledge(audit)).toBe(true);
      clock.value = 1_100;
      expect(await a.acknowledge(first)).toBe(false);
      expect(await a.retry(first, 10)).toBe(false);
      const second = (await b.claim(["receipt"], 100))!;
      expect(second.attempt).toBe(2);
      expect(second.idempotencyKey).toBe(first.idempotencyKey);
      expect(second.token).not.toBe(first.token);
      expect(await a.acknowledge(first)).toBe(false);
      expect(await a.retry(first, 10)).toBe(false);
      expect(await b.retry(second, 50)).toBe(true);
      expect(await a.claim(["receipt"], 100)).toBeNull();
      clock.value = 1_150;
      const third = (await a.claim(["receipt"], 100))!;
      expect(third.attempt).toBe(3);
      expect(await a.acknowledge(third)).toBe(true);
      expect((await b.status("one")).map((row) => [row.state, row.attempts])).toEqual([
        ["delivered", 1],
        ["delivered", 3],
      ]);
      expect(await b.claim(["receipt", "audit"], 100)).toBeNull();
      expect(await a.claim([], 100)).toBeNull();
      await expect(a.claim([], 0)).rejects.toThrow();
      // SKIP LOCKED must make progress on another delivery, not await the first row.
      await a.publish(event("locked"));
      await a.publish(event("unlocked"));
      await db[2].begin(async (tx) => {
        await tx`SELECT 1 FROM lenso_event_deliveries WHERE event_id = 'locked' FOR UPDATE`;
        const available = (await b.claim(["receipt"], 100))!;
        expect(available.event.id).toBe("unlocked");
      });
    });
  },
);

pgTest(
  "permanent cancellation fences processing, retains completions and survives reconnect",
  async () => {
    await fixture(async ({ stores: [a, b], db, clock }) => {
      await a.cancel("missing");
      await a.register("receipt", "order.created");
      await a.publish(event("completed"));
      const completed = (await a.claim(["receipt"], 100))!;
      await a.acknowledge(completed);
      await a.publish(event("processing"));
      const processing = (await a.claim(["receipt"], 100))!;
      await a.publish(event("pending"));
      await b.cancel("receipt");
      expect(await a.acknowledge(processing)).toBe(false);
      expect(await a.retry(processing, 10)).toBe(false);
      expect((await a.status("completed"))[0].state).toBe("delivered");
      for (const id of ["processing", "pending"])
        expect((await b.status(id))[0].state).toBe("cancelled");
      await expect(b.register("receipt", "order.created")).rejects.toThrow("cannot be reused");
      await a.publish(event("after"));
      expect(await a.status("after")).toEqual([]);
      await a.register("recover", "order.created");
      await a.publish(event("recover"));
      const lost = (await a.claim(["recover"], 100))!;
      await db[0].close();
      await expect(a.acknowledge(lost)).rejects.toThrow();
      await b.register("recover", "order.created");
      expect(await b.claim(["recover"], 100)).toBeNull();
      clock.value += 100;
      const recovered = (await b.claim(["recover"], 100))!;
      expect(recovered.attempt).toBe(2);
      expect(recovered.idempotencyKey).toBe(lost.idempotencyKey);
      expect(await b.acknowledge(recovered)).toBe(true);
    });
  },
);

pgTest("cancellation waits for in-flight publication and cancels its fanout", async () => {
  await fixture(async ({ db, stores: [a, b], admin, schema }) => {
    await a.register("receipt", "order.created");
    await db[0]
      .unsafe(`CREATE FUNCTION test_pause_fanout() RETURNS trigger LANGUAGE plpgsql AS $$
      BEGIN PERFORM pg_advisory_xact_lock(hashtext(TG_TABLE_SCHEMA), 28); RETURN NEW; END $$;
      CREATE TRIGGER test_pause BEFORE INSERT ON lenso_event_deliveries
      FOR EACH ROW EXECUTE FUNCTION test_pause_fanout();`)
      .simple();
    const release = gate();
    const locked = gate();
    const blocker = db[2].begin(async (tx) => {
      await tx`SELECT pg_advisory_xact_lock(hashtext(current_schema()), 28)`;
      locked.release();
      await release.promise;
    });
    await locked.promise;
    // Pause inside fanout after the publisher has locked the subscription FOR SHARE.
    const publication = a.publish(event("racing"));
    let cancellation: Promise<void> | undefined;
    try {
      await waiting(admin, `${schema}_0`);
      cancellation = b.cancel("receipt");
      await waiting(admin, `${schema}_1`);
    } finally {
      release.release();
    }
    await Promise.all([blocker, publication, cancellation]);
    expect((await a.status("racing")).map((row) => row.state)).toEqual(["cancelled"]);
    expect(await a.claim(["receipt"], 100)).toBeNull();
  });
});

pgTest(
  "terminated backend rolls publication back and producer retry restores exactly one fanout",
  async () => {
    await fixture(async ({ db, admin, schema, stores: [a, b] }) => {
      await a.register("receipt", "order.created");
      const release = gate();
      const locked = gate();
      const blocker = db[2].begin(async (tx) => {
        await tx`LOCK TABLE lenso_event_deliveries IN SHARE MODE`;
        locked.release();
        await release.promise;
      });
      await locked.promise;
      const publication = a.publish(event("disconnected"));
      const rejected = publication.then(
        () => false,
        () => true,
      );
      try {
        const pid = await waiting(admin, `${schema}_0`);
        expect((await admin`SELECT pg_terminate_backend(${pid}) AS killed`)[0].killed).toBe(true);
        expect(await rejected).toBe(true);
      } finally {
        release.release();
        await blocker;
      }
      expect(await b.status("disconnected")).toEqual([]);
      expect(await b.publish(event("disconnected"))).toEqual({ created: true });
      expect(await b.publish(event("disconnected"))).toEqual({ created: false });
      expect(await b.status("disconnected")).toHaveLength(1);
    });
  },
);

pgTest("settlement samples clock after lock wait; default lease uses database time", async () => {
  await fixture(async ({ db, stores: [a, b], clock, admin, schema }) => {
    await a.register("receipt", "order.created");
    await a.publish(event("late-ack"));
    const delivery = (await a.claim(["receipt"], 100))!;
    const release = gate();
    const locked = gate();
    const blocker = db[2].begin(async (tx) => {
      await tx`SELECT 1 FROM lenso_event_deliveries WHERE event_id = 'late-ack' FOR UPDATE`;
      locked.release();
      await release.promise;
    });
    await locked.promise;
    const acknowledgement = b.acknowledge(delivery);
    try {
      await waiting(admin, `${schema}_1`);
      clock.value += 100;
    } finally {
      release.release();
    }
    await blocker;
    expect(await acknowledgement).toBe(false);
    const serverClockStore = createPostgresEventStore(db[0]);
    const before = Number(
      (await db[0]`SELECT floor(extract(epoch FROM clock_timestamp()) * 1000) AS ms`)[0].ms,
    );
    expect((await serverClockStore.claim(["receipt"], 30_000))!.attempt).toBe(2);
    const lease = (await serverClockStore.status("late-ack"))[0].leaseUntil!;
    const after = Number(
      (await db[0]`SELECT floor(extract(epoch FROM clock_timestamp()) * 1000) AS ms`)[0].ms,
    );
    expect(lease).toBeGreaterThanOrEqual(before + 30_000);
    expect(lease).toBeLessThanOrEqual(after + 30_000);
  });
});

pgTest(
  "async service retries one subscriber, drains on close and reattaches persistent backlog",
  async () => {
    await fixture(async ({ stores: [a, b], clock }) => {
      const first = createAsyncEvents({ store: a, leaseMs: 100, retryDelayMs: 20 });
      const second = createAsyncEvents({ store: b, leaseMs: 100, retryDelayMs: 20 });
      const effects = new Set<string>();
      try {
        await first.subscribe("receipt", "order.created", (_, context) => {
          effects.add(context.idempotencyKey);
          throw new Error("failure after external effect");
        });
        await first.subscribe("audit", "order.created", () => {});
        await first.publish(event("service"));
        expect(await first.dispatch()).toEqual({ delivered: 1, retried: 1, lost: 0 });
        await first.close();
        await second.subscribe("receipt", "order.created", (_, context) => {
          effects.add(context.idempotencyKey);
        });
        clock.value += 20;
        expect(await second.dispatch()).toEqual({ delivered: 1, retried: 0, lost: 0 });
        expect(effects.size).toBe(1);
        expect((await second.status("service")).map((row) => row.attempts)).toEqual([1, 2]);
      } finally {
        await Promise.all([first.close(), second.close()]);
      }
    });
  },
);
