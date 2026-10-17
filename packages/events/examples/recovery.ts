import { Database } from "bun:sqlite";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createEvents } from "@lenso/events";
import { createSqliteEventStore, initializeEvents } from "@lenso/events/sqlite";

// Local fake destination: separate database, unique delivery keys, no network/paid services.
const directory = mkdtempSync(join(tmpdir(), "lenso-outbox-recovery-"));
const destination = new Database(join(directory, "fake-destination.sqlite"));
let db = new Database(join(directory, "events.sqlite"));
let time = 1_000;
let attempts = 0;
try {
  destination.run("CREATE TABLE effects(delivery_key TEXT PRIMARY KEY)");
  initializeEvents(db);
  const firstStore = createSqliteEventStore(db, { now: () => time });
  const first = createEvents({
    leaseMs: 100,
    store: {
      ...firstStore,
      acknowledge() {
        throw new Error("Simulated crash after effect, before acknowledgement");
      },
    },
  });
  function fakeDestination(_event: unknown, { idempotencyKey }: { idempotencyKey: string }) {
    attempts++;
    destination.run("INSERT OR IGNORE INTO effects VALUES (?)", [idempotencyKey]);
  }
  first.subscribe("receipt-v1", "order.created", fakeDestination);
  first.publish({ id: "order:42:created", topic: "order.created", payload: { orderId: 42 } });
  try {
    await first.dispatch();
    throw new Error("Expected acknowledgement failure");
  } catch (error) {
    if (!(error instanceof Error) || !error.message.startsWith("Simulated crash")) throw error;
    console.log(error.message);
  }
  await first.close();
  db.close();

  // Restart against persisted data and advance a fake clock; no real-time sleep.
  db = new Database(join(directory, "events.sqlite"));
  time += 100;
  const recovered = createEvents({
    store: createSqliteEventStore(db, { now: () => time }),
    leaseMs: 100,
  });
  recovered.subscribe("receipt-v1", "order.created", fakeDestination);
  const result = await recovered.dispatch();
  const effects = destination
    .query<{ count: number }, []>("SELECT count(*) AS count FROM effects")
    .get()!.count;
  if (result.delivered !== 1 || attempts !== 2 || effects !== 1)
    throw new Error("Recovery invariant failed");
  console.log(
    JSON.stringify({ attempts, effects, result, status: recovered.status("order:42:created") }),
  );
  await recovered.close();
} finally {
  db.close();
  destination.close();
  rmSync(directory, { recursive: true, force: true });
}
