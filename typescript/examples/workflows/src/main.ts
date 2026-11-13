import { mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { ActionError, Catalog, DurableEvent, Event, Executor, Module, consoleManifest, defineResource, defineTask, jsonCodec, requireScope, withResources } from "@lenso/app";
import type { Action, Context } from "@lenso/app";
import { SqliteQueue } from "@lenso/sqlite";

interface Mail { recipient: string; body: string }
const mailCodec = jsonCodec({
  parse(value: unknown): Mail {
    if (typeof value !== "object" || value === null || !("recipient" in value) || typeof value.recipient !== "string"
      || !("body" in value) || typeof value.body !== "string") throw new ActionError("invalid_input", "Invalid mail payload");
    return { recipient: value.recipient, body: value.body };
  },
});
interface Resources { db: DatabaseSync; queue: SqliteQueue; mail: { deliver(id: string, input: Mail): void } }

const sendMail = defineTask<Resources>()({
  id: "mail.deliver",
  input: mailCodec,
  output: jsonCodec({
    parse(value: unknown) {
      if (typeof value !== "string") throw new ActionError("internal", "Invalid mail result");
      return value;
    },
  }),
  run(context, input) {
    context.resources.mail.deliver(context.jobId, input);
    return "delivered";
  },
});

function transaction<T>(db: DatabaseSync, run: () => T): T {
  db.exec("BEGIN IMMEDIATE");
  try { const result = run(); db.exec("COMMIT"); return result; }
  catch (error) { db.exec("ROLLBACK"); throw error; }
}

interface User { id: string; name: string; email: string }
const userCreated = new DurableEvent<User>("user.created");
const userObserved = new Event<User>("user.created.local");
const register = {
  id: "users.register",
  authorize(context) { requireScope(context.principal, "users:create"); },
  async run(context, input) {
    transaction(context.resources.db, () => {
      context.resources.db.prepare("INSERT INTO users (id, name) VALUES (?, ?) ON CONFLICT(id) DO NOTHING").run(input.id, input.name);
      context.resources.queue.publish(userCreated, input.id, input);
    });
    await userObserved.publish(input);
    return input.id;
  },
} satisfies Action<{ id: string; name: string; email: string }, string, Resources>;

const invite = {
  id: "invitations.create",
  authorize(context) { requireScope(context.principal, "invitations:create"); },
  run(context, input) {
    return transaction(context.resources.db, () => {
      context.resources.db.prepare("INSERT INTO invitations (id, recipient) VALUES (?, ?) ON CONFLICT(id) DO NOTHING").run(input.id, input.recipient);
      return context.resources.queue.enqueue(sendMail, { recipient: input.recipient, body: `Invitation ${input.id}` }, { id: `invitation/${input.id}` }).id;
    });
  },
} satisfies Action<{ id: string; recipient: string }, string, Resources>;

const listMail = {
  id: "console.mailbox",
  authorize(context) { requireScope(context.principal, "console:read"); },
  run(context) { return context.resources.db.prepare("SELECT job_id, recipient, body FROM mailbox ORDER BY job_id").all(); },
} satisfies Action<void, unknown, Resources>;

const filename = process.env.LENSO_DB ?? ".local/workflows.sqlite";
mkdirSync(dirname(filename), { recursive: true });
const database = defineResource({
  id: "sqlite",
  config: { parse(value: unknown) { if (typeof value !== "string" || !value) throw new TypeError("Database path required"); return value; } },
  open: (filename) => new DatabaseSync(filename), close: (db) => db.close(),
});
await withResources({ db: database.configure(filename) }, async ({ db }) => {
  db.exec(`CREATE TABLE IF NOT EXISTS users (id TEXT PRIMARY KEY, name TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS invitations (id TEXT PRIMARY KEY, recipient TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS mailbox (job_id TEXT PRIMARY KEY, recipient TEXT NOT NULL, body TEXT NOT NULL);`);
  const resources: Resources = { db, queue: new SqliteQueue(db), mail: {
    deliver(id, input) {
      db.prepare("INSERT INTO mailbox (job_id, recipient, body) VALUES (?, ?, ?) ON CONFLICT(job_id) DO NOTHING")
        .run(id, input.recipient, input.body);
    },
  } };
  const context: Context<Resources> = {
    callId: "workflow-demo", resources,
    principal: { subject: "local-operator", scopes: ["users:create", "invitations:create", "console:read"] },
    signal: new AbortController().signal,
  };
  const records = [];
  const executor = new Executor((record) => { records.push(record); });
  const catalog = new Catalog();
  catalog.register(new Module("notifications", { actions: { register, invite, listMail }, tasks: { sendMail }, events: { userCreated, userObserved }, resources: [database.id, "mailbox"] }));
  catalog.requireResources([database.id, "mailbox"]);
  let localNotifications = 0;
  userCreated.subscribe("welcome", sendMail, (user) => ({ recipient: user.email, body: `Welcome, ${user.name}` }));
  userObserved.subscribe(() => { localNotifications++; });
  await executor.call(register, context, { id: "alice", name: "Alice", email: "alice@example.test" });
  await executor.call(invite, context, { id: "invite-1", recipient: "invitee@example.test" });
  let completed = 0;
  while (await resources.queue.runOne(sendMail, resources)) completed++;
  const mailbox = await executor.call(listMail, context, undefined);
  const manifest = consoleManifest(catalog, [{ id: "mailbox", title: "Local mailbox", list: listMail.id }]);
  console.log(JSON.stringify({ mailbox, manifest, localNotifications, calls: records.length, completed }));
});
