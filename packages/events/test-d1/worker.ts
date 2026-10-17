import { createD1EventStore, initializeD1Events, type D1EventDatabase } from "../src/d1";
import type { Delivery, Json } from "../src/contracts";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}
async function rejects(action: () => Promise<unknown>, message: string) {
  let failed = false;
  try {
    await action();
  } catch {
    failed = true;
  }
  assert(failed, message);
}
const event = (id: string, topic = "topic", payload: Json = { z: 1, a: 2 }) => ({
  id,
  topic,
  payload,
});

export default {
  async fetch(request: Request, env: { DB: D1EventDatabase }) {
    try {
      const action = new URL(request.url).pathname;
      const db = env.DB;
      let time = 1000;
      const store = createD1EventStore(db, { now: () => time });
      const peer = createD1EventStore(db, { now: () => time });
      if (action === "/concurrent-seed") {
        await store.register("requests", "requests");
        await store.publish(event("requests", "requests"));
        return Response.json({ seeded: true });
      }
      if (action === "/concurrent-claim") {
        return Response.json(await store.claim(["requests"], 20));
      }
      if (action === "/seed") {
        await store.register("restart", "restart");
        await store.publish(event("restart-event", "restart"));
        const delivery = await store.claim(["restart"], 20);
        assert(delivery, "restart seed claimed");
        // Fake idempotent destination effect happens before the Worker crashes.
        await db
          .prepare("INSERT INTO test_effects(id) VALUES (?)")
          .bind(delivery.idempotencyKey)
          .run();
        return Response.json(delivery);
      }
      if (action === "/recover") {
        const old = (await request.json()) as Delivery;
        time = 1020;
        const recovered = await store.claim(["restart"], 20);
        assert(recovered && recovered.attempt === 2, "restart recovers expired claim");
        assert(recovered.idempotencyKey === old.idempotencyKey, "restart identity stable");
        assert(!(await store.acknowledge(old)), "old process cannot acknowledge");
        assert(!(await store.retry(old, 5)), "old process cannot retry");
        await db
          .prepare("INSERT INTO test_effects(id) VALUES (?) ON CONFLICT(id) DO NOTHING")
          .bind(recovered.idempotencyKey)
          .run();
        const effects = await db
          .prepare("SELECT count(*) AS count FROM test_effects")
          .all<{ count: number }>();
        assert(
          effects.results[0].count === 1,
          "idempotent destination preserves one effect after crash",
        );
        assert(await store.acknowledge(recovered), "recovered delivery acknowledged");
        return Response.json({
          restart: "passed",
          attempts: recovered.attempt,
          effects: effects.results[0].count,
        });
      }
      const checks: string[] = [];
      await initializeD1Events(db);
      await initializeD1Events(db);
      await db.prepare("CREATE TABLE test_effects(id TEXT PRIMARY KEY)").run();
      await Promise.all([store.register("a", "topic"), peer.register("a", "topic")]);
      await store.register("b", "topic");
      await rejects(() => peer.register("a", "different"), "subscription cannot change topics");
      // Fail the second fanout row: both the first row and event INSERT must roll back.
      await db
        .prepare(`CREATE TRIGGER fail_fanout BEFORE INSERT ON lenso_event_deliveries
        WHEN NEW.subscription_id = 'b' BEGIN SELECT RAISE(ABORT, 'injected'); END`)
        .run();
      await rejects(() => store.publish(event("atomic")), "batch must reject failed fanout");
      const count = await db
        .prepare("SELECT count(*) AS count FROM lenso_events")
        .all<{ count: number }>();
      assert(count.results[0].count === 0, "event insert rolled back");
      assert((await store.status("atomic")).length === 0, "all deliveries rolled back");
      await db.prepare("DROP TRIGGER fail_fanout").run();
      checks.push("fanout failure rolls back entire D1 batch");

      const publications = await Promise.all([
        store.publish(event("atomic")),
        peer.publish(event("atomic")),
      ]);
      assert(publications.filter((p) => p.created).length === 1, "one publication wins");
      await store.register("late", "topic");
      assert(
        !(await store.publish(event("atomic", "topic", { a: 2, z: 1 }))).created,
        "canonical duplicate",
      );
      await rejects(
        () => store.publish(event("atomic", "topic", { a: 9 })),
        "conflicting identity rejected",
      );
      assert((await store.status("atomic")).length === 2, "no historical fanout on duplicate");
      checks.push("concurrent deduplication and canonical identity preserve subscriber snapshot");

      const claims = await Promise.all(
        Array.from({ length: 12 }, (_, i) => (i % 2 ? store : peer).claim(["a"], 20)),
      );
      const first = claims.find((claim): claim is Delivery => claim !== null);
      assert(first && claims.filter(Boolean).length === 1, "one concurrent claimant wins");
      assert(
        first.event.topic === "topic" && JSON.stringify(first.event.payload) === '{"a":2,"z":1}',
        "claim returns event",
      );
      time = 1020;
      assert(!(await store.acknowledge(first)), "expiry boundary is fenced before replacement");
      const next = await peer.claim(["a"], 20);
      assert(next && next.attempt === 2 && next.token !== first.token, "expired claim replaced");
      assert(next.idempotencyKey === first.idempotencyKey, "idempotency key stable");
      assert(
        !(await store.acknowledge(first)) && !(await store.retry(first, 5)),
        "stale mutations fenced",
      );
      assert(await peer.retry(next, 5), "current failure rescheduled");
      assert((await store.claim(["a"], 20)) === null, "retry delay enforced");
      time = 1025;
      const retried = await store.claim(["a"], 20);
      assert(
        retried && retried.attempt === 3 && (await store.acknowledge(retried)),
        "retry recovers",
      );
      assert(!(await store.acknowledge(retried)), "duplicate ack is false");
      checks.push("concurrent claims, expiration fencing, delayed retry and acknowledgement");

      const b = await store.claim(["b"], 20);
      assert(b, "independent subscriber remains claimable");
      await db
        .prepare(`CREATE TRIGGER fail_cancel BEFORE UPDATE ON lenso_event_deliveries
        WHEN NEW.state = 'cancelled' BEGIN SELECT RAISE(ABORT, 'injected'); END`)
        .run();
      await rejects(() => store.cancel("b"), "cancel failure rejects");
      await store.register("b", "topic"); // proves active flag rolled back too
      assert(
        (await store.status("atomic")).find((s) => s.subscriptionId === "b")?.state ===
          "processing",
        "cancel rolled back delivery",
      );
      await db.prepare("DROP TRIGGER fail_cancel").run();
      await store.cancel("b");
      assert(
        !(await store.acknowledge(b)) && !(await store.retry(b, 1)),
        "cancel fences current token",
      );
      await rejects(() => store.register("b", "topic"), "cancelled ID cannot be reused");
      assert((await store.claim(["b"], 20)) === null, "cancelled subscriber never claimed");
      const states = await store.status("atomic");
      assert(
        states[0].state === "delivered" && states[1].state === "cancelled",
        "subscriber outcomes isolated",
      );
      checks.push("atomic cancellation, permanent subscription identity and subscriber isolation");

      await store.register("lost", "lost");
      await store.publish(event("lost-response", "lost"));
      const lost = await store.claim(["lost"], 20);
      assert(lost, "claim for failed ack");
      await db
        .prepare(`CREATE TRIGGER fail_ack BEFORE UPDATE ON lenso_event_deliveries
        WHEN NEW.state = 'delivered' BEGIN SELECT RAISE(ABORT, 'injected'); END`)
        .run();
      await rejects(() => store.acknowledge(lost), "storage error must propagate");
      await db.prepare("DROP TRIGGER fail_ack").run();
      time += 20;
      const recovered = await store.claim(["lost"], 20);
      assert(
        recovered && recovered.attempt === 2 && (await store.acknowledge(recovered)),
        "failed ack recovers",
      );
      // Simulate an acknowledged publication whose caller lost its response.
      await store.register("lost-publish", "lost-publish");
      await store.publish(event("lost-publish", "lost-publish"));
      assert(
        !(await peer.publish(event("lost-publish", "lost-publish"))).created,
        "lost publish response safely retried",
      );
      checks.push("storage rejection and uncertain publication recovery");

      for (const payload of [
        NaN,
        undefined,
        new Date(),
        Array(1),
        { invalid: undefined },
        "x".repeat(1_048_576),
      ]) {
        await rejects(
          () => store.publish({ id: "invalid", topic: "topic", payload: payload as Json }),
          "invalid JSON/oversize rejected",
        );
      }
      await rejects(() => store.claim(["a"], 0), "invalid duration rejected");
      await rejects(
        () => createD1EventStore(db, { now: () => -1 }).publish(event("bad-clock")),
        "invalid clock rejected",
      );
      assert((await store.claim([], 1)) === null, "empty subscription set");
      checks.push("JSON, byte size, clock and duration validation");
      const production = createD1EventStore(db);
      await production.register("clock", "clock");
      await production.publish(event("clock", "clock"));
      const clockStatus = await production.status("clock");
      assert(
        Math.abs(clockStatus[0].availableAt - Date.now()) < 10000,
        "database clock uses milliseconds",
      );
      const clockClaim = await production.claim(["clock"], 1);
      assert(clockClaim, "production clock claims delivery");
      await new Promise((resolve) => setTimeout(resolve, 30));
      assert(
        !(await production.acknowledge(clockClaim)),
        "production clock fences expired delivery",
      );
      const clockRecovered = await production.claim(["clock"], 10000);
      assert(
        clockRecovered && (await production.acknowledge(clockRecovered)),
        "production clock recovers",
      );
      checks.push("database-side production clock and expiration");
      return Response.json({ checks });
    } catch (error) {
      return Response.json(
        { error: String(error), stack: error instanceof Error ? error.stack : undefined },
        { status: 500 },
      );
    }
  },
};
