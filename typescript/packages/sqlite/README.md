# @lenso/sqlite

SQLite-backed durable tasks and event delivery for Node.js. The application owns the native database connection and worker loop.

```ts
import { DatabaseSync } from "node:sqlite";
import { SqliteQueue } from "@lenso/sqlite";
import { z } from "zod";
import { defineTask, jsonCodec } from "@lenso/app";

const task = defineTask<{ send: (id: string, body: string) => Promise<void> }>()({
  id: "mail.send",
  input: jsonCodec(z.string()),
  output: jsonCodec(z.string()),
  async run(context, body) {
    await context.resources.send(context.jobId, body);
    return "sent";
  },
});

const db = new DatabaseSync("jobs.sqlite");
const queue = new SqliteQueue(db);
const receipt = queue.enqueue(task, "Hello", { id: "welcome/alice" });
await queue.runOne(task, { send }, { concurrency: 1 });
const status = queue.get(receipt.id);
const attempts = queue.attempts(receipt.id);
db.close();
```

`send` is an application-supplied resource; the SDK does not supply an email provider. Enqueue inside an authorized operation. Delivery can repeat, so downstream effects must use the job id for idempotency. All workers for one task kind must agree on its concurrency setting.

`publish(event, eventId, value)` accepts a typed `DurableEvent`; subscribers own task mapping and payload encoding. The publication savepoint joins an existing native SQLite transaction or commits on its own when there is no enclosing transaction. It does not make an earlier, separately committed write atomic. Per-attempt history survives later successful retries.

A publication takes one clock snapshot for newly created subscriber jobs. If a running worker loses its lease, it signals cancellation and waits for its handler to settle before returning the lease error. A handler that ignores cancellation can delay that return. Lease fencing still prevents stale writes to queue status.
