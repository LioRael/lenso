import { SQL } from "bun";
import { beforeAll, describe, expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import { Pool } from "pg";
import { drizzle as bunDrizzle } from "drizzle-orm/bun-sql";
import { drizzle as pgDrizzle } from "drizzle-orm/node-postgres";
import { createAuditService } from "@lenso/audit";
import { createPostgresAuditRepository } from "@lenso/audit/postgres";
import { createLimits } from "@lenso/limits";
import { createPostgresLimitStore } from "@lenso/limits/postgres";
import { createTaskQueue, type TaskQueue } from "@lenso/tasks";
import { createPostgresTaskProvider, migratePostgresTaskQueue } from "@lenso/tasks/postgres";
import {
  createWebhooks, defineWebhookTask, verifyWebhook, webhookConfig, WebhookError,
  type EventEnvelope, type WebhookConfig, type WebhookContext, type WebhookScope,
  type Webhooks,
} from "../src/index";
import { createPostgresWebhookRepository } from "../src/postgres";

const connectionString = process.env.WEBHOOK_TEST_DATABASE_URL;
if (!connectionString) {
  console.warn("Skipping real Webhooks PostgreSQL integration tests: WEBHOOK_TEST_DATABASE_URL is absent (requires a disposable database).");
}
const integration = connectionString ? describe : describe.skip;
const key = { id: "integration-key", secret: new TextEncoder().encode("test-only-signing-key-not-a-production-credential") };
const endpointHost = "webhooks.test";
const secretRef = "test-only-secret-reference";
const uuid = () => crypto.randomUUID();
const freshScope = (): WebhookScope => ({ tenantId: uuid(), scopeId: uuid() });
const context = (scope: WebhookScope): WebhookContext<WebhookScope> => ({ scope, principal: scope });
const owns = (principal: WebhookScope, scope: { tenantId: string | null; scopeId: string }) =>
  principal.tenantId === scope.tenantId && principal.scopeId === scope.scopeId;

async function waitFor<T>(read: () => Promise<T>, accept: (value: T) => boolean): Promise<T> {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    const value = await read();
    if (accept(value)) return value;
    await Bun.sleep(20);
  }
  throw new Error("Timed out waiting for webhook integration state");
}

// These migrations are host-owned, not implicit adapter setup. Older optional
// packages ship non-idempotent DDL; adding IF NOT EXISTS makes repeat runs safe.
async function migrate(pool: Pool) {
  for (const path of [
    "../migrations/0001_webhooks.sql",
    "../../audit/migrations/pg/0000_audit.sql",
    "../../limits/migrations/0001_postgres.sql",
  ]) {
    const sql = await readFile(new URL(path, import.meta.url), "utf8");
    await pool.query(sql.replaceAll("__LENSO_WEBHOOK_SCHEMA__", '"public"')
      .replace(/CREATE TABLE (?!IF NOT EXISTS)/g, "CREATE TABLE IF NOT EXISTS ")
      .replace(/CREATE UNIQUE INDEX (?!IF NOT EXISTS)/g, "CREATE UNIQUE INDEX IF NOT EXISTS ")
      .replace(/CREATE INDEX (?!IF NOT EXISTS)/g, "CREATE INDEX IF NOT EXISTS "));
  }
}

type Received = { path: string; body: Uint8Array; event: EventEnvelope; validSignature: boolean };
type Reply = { status: number; retryAfter?: string; delayMs?: number };

async function fixture() {
  const cleanup: Array<() => Promise<unknown>> = [];
  const close = async () => {
    const failures: unknown[] = [];
    for (const release of cleanup.reverse()) {
      try { await release(); } catch (error) { failures.push(error); }
    }
    if (failures.length) throw new AggregateError(failures, "Webhook fixture cleanup failed");
  };
  try {
    const pool = new Pool({ connectionString });
    cleanup.push(() => pool.end());
    const auditSql = new SQL(connectionString!);
    cleanup.push(() => auditSql.close());
    const scope = freshScope();
    const ctx = context(scope);
    const received: Received[] = [];
    const replies = new Map<string, Reply[]>();
    const deniedActions = new Set<string>();
    const server = Bun.serve({
      hostname: "127.0.0.1", port: 0,
      async fetch(request) {
        const body = new Uint8Array(await request.arrayBuffer());
        const path = new URL(request.url).pathname;
        received.push({
          path, body, event: JSON.parse(new TextDecoder().decode(body)),
          validSignature: verifyWebhook({
            body,
            eventId: request.headers.get("x-lenso-event-id") ?? "",
            timestamp: request.headers.get("x-lenso-timestamp") ?? "",
            signature: request.headers.get("x-lenso-signature") ?? "",
            keys: [key], now: Math.floor(Date.now() / 1000),
          }),
        });
        const script = replies.get(path);
        const reply = script?.length ? script.shift()! : { status: 204 };
        if (reply.delayMs) await Bun.sleep(reply.delayMs);
        return new Response(null, {
          status: reply.status,
          headers: reply.retryAfter ? { "retry-after": reply.retryAfter } : {},
        });
      },
    });
    cleanup.push(async () => { await server.stop(true); });
    const audit = createAuditService({
      repository: createPostgresAuditRepository(bunDrizzle(auditSql), { durableIntents: true }),
      authority: {
        async resolve(principal: WebhookScope, requested) {
          if (!owns(principal, requested)) throw new Error("test principal scope denied");
          return { kind: "system" as const, systemId: "webhook-integration-host" };
        },
      },
    });
    const limits = createLimits({
      store: createPostgresLimitStore(pgDrizzle(pool)),
      config: { failurePolicy: "throw" },
    });
    cleanup.push(() => limits.close());
    let service: Webhooks<WebhookScope>;
    const task = defineWebhookTask(`webhooks_${uuid().replaceAll("-", "")}`,
      (input, signal) => service.execute(input, signal));
    const providerOptions = {
      pool, queueName: `webhooks_${uuid().replaceAll("-", "")}`,
      pollIntervalMs: 20, heartbeatSeconds: 10, expireInSeconds: 120,
    };
    await migratePostgresTaskQueue(providerOptions);
    const queue = createTaskQueue({
      provider: await createPostgresTaskProvider(providerOptions), tasks: [task],
    });
    cleanup.push(() => queue.close());
    const config = webhookConfig({
      enabled: true, instanceId: uuid(), source: "webhook-integration",
      eventTypes: ["order.created"], maxAttempts: 3, concurrency: 8,
      baseDelayMs: 100, maxDelayMs: 200, retentionMs: 86_400_000,
      outbound: {
        allowedHosts: [endpointHost], timeoutMs: 250, dnsTimeoutMs: 50,
        connectTimeoutMs: 50, maxRequestBytes: 64_000, maxResponseBytes: 1_024,
      },
    });
    function recreate(
      borrowedQueue: TaskQueue = queue,
      overrides: Partial<WebhookConfig> = {},
      repository = createPostgresWebhookRepository({ pool }),
    ) {
      service = createWebhooks({
        repository, audit, limits, task,
        queue: borrowedQueue, config: { ...config, ...overrides },
        authority: {
          async authorize(principal: WebhookScope, requested, action) {
            if (!owns(principal, requested) || deniedActions.has(action)) throw new Error("secret authority diagnostic");
          },
        },
        keys: { async active(ref) {
          if (ref !== secretRef) throw new Error("unknown test key");
          return key;
        } },
        // Test-only transport DI: fetch never sees a configured destination.
        // This owned listener exercises HTTP bytes, not production SSRF security.
        transport: { async send(input) {
          const target = new URL(input.url);
          if (target.protocol !== "https:" || target.hostname !== endpointHost) {
            throw new Error("Fixture refuses unowned destinations");
          }
          const response = await fetch(`http://127.0.0.1:${server.port}${target.pathname}`, {
            method: "POST", body: new Uint8Array(input.body), headers: input.headers,
            signal: input.signal, redirect: "error",
          });
          await response.arrayBuffer();
          return { status: response.status, retryAfter: response.headers.get("retry-after") };
        } },
      });
      return service;
    }
    recreate();
    async function endpoint(path: string, target = ctx) {
      const id = uuid();
      await service.putEndpoint({ id, url: `https://${endpointHost}${path}`, secretRef, enabled: true }, target);
      const subscriptionId = uuid();
      await service.putSubscription({
        id: subscriptionId, endpointId: id, eventType: "order.created", enabled: true,
      }, target);
      return { id, subscriptionId };
    }
    async function publish(data: EventEnvelope["data"] = { orderId: "example", text: "raw bytes: café ☕" }) {
      return service.publish({ type: "order.created", data }, ctx);
    }
    async function batch() { await queue.runBatch({ concurrency: 8, maxJobs: 100, timeoutMs: 5_000 }); }
    async function settle(id: string, state: "succeeded" | "failed") {
      return waitFor(async () => {
        await batch();
        return service.getDelivery({ id }, ctx);
      }, delivery => delivery.state === state);
    }
    return {
      pool, scope, ctx, received, replies, deniedActions, audit, queue, task, config,
      get service() { return service; },
      recreate, endpoint, publish, batch, settle, close,
      repository: () => createPostgresWebhookRepository({ pool }),
    };
  } catch (error) {
    try { await close(); } catch (cleanupError) {
      throw new AggregateError([error, cleanupError], "Webhook fixture setup failed", { cause: cleanupError });
    }
    throw error;
  }
}

integration("real PostgreSQL Webhooks, Tasks worker, HTTP and strict durable replay audit", () => {
  beforeAll(async () => {
    const pool = new Pool({ connectionString });
    try { await migrate(pool); } finally { await pool.end(); }
  });

  test("persists raw signed events, retries capped 429/5xx/timeout, and durably audits manual replay", async () => {
    const f = await fixture();
    try {
      await f.endpoint("/retry");
      f.replies.set("/retry", [{ status: 429, retryAfter: "99999999" }, { status: 503 }, { status: 204 }]);
      const published = await f.publish();
      expect(published.dispatch).toBe("queued");
      const id = published.deliveryIds[0]!;
      await f.queue.runBatch({ concurrency: 1, maxJobs: 1, timeoutMs: 5_000 });
      const retry = await f.service.getDelivery({ id }, f.ctx);
      expect(retry.state).toBe("retry");
      expect(retry.dueAt - retry.updatedAt).toBeGreaterThanOrEqual(0);
      expect(retry.dueAt - retry.updatedAt).toBeLessThanOrEqual(f.config.maxDelayMs);
      const succeeded = await f.settle(id, "succeeded");
      expect(succeeded.attemptCount).toBe(3);
      const attempts = await f.service.listAttempts({ deliveryId: id }, f.ctx);
      expect(attempts.items.map(attempt => attempt.code).sort()).toEqual(["rate-limited", "server-error", "success"]);
      expect(attempts.items.every(attempt => attempt.keyId === key.id)).toBe(true);
      expect(f.received).toHaveLength(3);
      expect(f.received.every(request => request.validSignature)).toBe(true);
      expect(f.received.every(request => request.event.id === published.eventId)).toBe(true);
      expect(f.received.every(request => Buffer.from(request.body).equals(Buffer.from(f.received[0]!.body)))).toBe(true);

      const replay = await f.service.replay({ id }, f.ctx);
      expect(replay.delivery.id).not.toBe(id);
      expect(replay.delivery.replayOf).toBe(id);
      expect(replay.delivery.eventId).toBe(published.eventId);
      expect(replay.delivery.auditIntentId).not.toBeNull();
      await f.settle(replay.delivery.id, "succeeded");
      expect((await f.service.getDelivery({ id }, f.ctx)).attemptCount).toBe(3);
      const history = await f.audit.query({
        scope: f.scope, action: "webhooks.replay", correlationId: id,
        target: { type: "webhook-delivery", id: replay.delivery.id },
      }, f.scope);
      expect(history.events).toHaveLength(2);
      const intent = history.events.find(event => event.result === "intent")!;
      const outcome = history.events.find(event => event.result === "success")!;
      expect(intent.id).toBe(replay.delivery.auditIntentId!);
      expect(intent.reasonCode).toBe("manual-replay");
      expect(outcome.reasonCode).toBe("delivery-created");
      expect(outcome.relation).toEqual({ kind: "outcome", eventId: intent.id });
      await expect(f.audit.query({ scope: freshScope() }, f.scope)).rejects.toMatchObject({ code: "unauthorized" });
      expect(f.received[3]!.event.id).toBe(published.eventId);

      f.replies.set("/retry", Array.from({ length: 3 }, () => ({ status: 204, delayMs: 750 })));
      const timeout = await f.publish();
      const failed = await f.settle(timeout.deliveryIds[0]!, "failed");
      expect(failed.attemptCount).toBe(3);
      expect((await f.service.listAttempts({ deliveryId: failed.id }, f.ctx)).items.every(attempt => attempt.code === "timeout")).toBe(true);
      const count = f.received.length;
      await f.queue.enqueue(f.task, { deliveryId: failed.id, generation: failed.generation });
      await f.batch();
      expect(f.received).toHaveLength(count);
      expect(f.received.filter(request => request.event.id === timeout.eventId)).toHaveLength(3);
    } finally { await f.close(); }
  }, 30_000);

  test("isolates tenants and scopes, snapshots endpoint revisions, and suppresses disabled or cancelled subscriptions", async () => {
    const f = await fixture();
    try {
      const endpoint = await f.endpoint("/original");
      const published = await f.publish();
      const id = published.deliveryIds[0]!;
      for (const scope of [
        { ...f.scope, tenantId: uuid() }, { ...f.scope, scopeId: uuid() },
      ]) {
        const other = context(scope);
        expect((await f.service.listEndpoints({}, other)).items).toEqual([]);
        expect((await f.service.listSubscriptions({}, other)).items).toEqual([]);
        expect((await f.service.listDeliveries({}, other)).items).toEqual([]);
        await expect(f.service.getEndpoint({ id: endpoint.id }, other)).rejects.toMatchObject({ code: "not-found" });
        await expect(f.service.getDelivery({ id }, other)).rejects.toMatchObject({ code: "not-found" });
        await expect(f.service.listAttempts({ deliveryId: id }, other)).rejects.toMatchObject({ code: "not-found" });
        await expect(f.service.putEndpoint({
          id: endpoint.id, url: `https://${endpointHost}/stolen`, secretRef, enabled: true,
        }, other)).rejects.toBeInstanceOf(WebhookError);
        await expect(f.service.putSubscription({
          id: endpoint.subscriptionId, endpointId: endpoint.id, eventType: "order.created", enabled: false,
        }, other)).rejects.toBeInstanceOf(WebhookError);
        await expect(f.service.replay({ id }, other)).rejects.toMatchObject({ code: "not-found" });
      }
      const forged = { scope: f.scope, principal: freshScope() };
      await expect(f.service.listEndpoints({}, forged)).rejects.toMatchObject({ code: "unauthorized", message: "Webhook unauthorized" });
      await f.service.putEndpoint({
        id: endpoint.id, url: `https://${endpointHost}/updated`, secretRef, enabled: true,
      }, f.ctx);
      const snapshot = await f.settle(id, "succeeded");
      expect(snapshot.endpointRevision).toBe(1);
      expect(f.received.map(request => request.path)).toEqual(["/original"]);

      f.replies.set("/updated", [{ status: 400 }]);
      const permanent = await f.publish();
      expect((await f.settle(permanent.deliveryIds[0]!, "failed")).attemptCount).toBe(1);
      expect((await f.service.listAttempts({ deliveryId: permanent.deliveryIds[0]! }, f.ctx)).items[0]).toMatchObject({ code: "permanent-http", status: 400 });
      const queued = await f.publish();
      await f.service.putEndpoint({
        id: endpoint.id, url: `https://${endpointHost}/updated`, secretRef, enabled: false,
      }, f.ctx);
      const count = f.received.length;
      expect((await f.settle(queued.deliveryIds[0]!, "failed")).lastCode).toBe("endpoint-disabled");
      expect((await f.service.listAttempts({ deliveryId: queued.deliveryIds[0]! }, f.ctx)).items).toEqual([]);
      expect(f.received).toHaveLength(count);
      await f.service.putEndpoint({
        id: endpoint.id, url: `https://${endpointHost}/updated`, secretRef, enabled: true,
      }, f.ctx);
      const cancelled = await f.publish();
      await f.service.putSubscription({
        id: endpoint.subscriptionId, endpointId: endpoint.id, eventType: "order.created", enabled: false,
      }, f.ctx);
      expect((await f.settle(cancelled.deliveryIds[0]!, "failed")).lastCode).toBe("unsubscribed");
      expect((await f.service.listAttempts({ deliveryId: cancelled.deliveryIds[0]! }, f.ctx)).items).toEqual([]);
      expect(f.received).toHaveLength(count);
      await f.service.putSubscription({
        id: endpoint.subscriptionId, endpointId: endpoint.id, eventType: "order.created", enabled: true,
      }, f.ctx);
      const beforeDisable = await f.publish();
      const disabled = f.recreate(f.queue, { enabled: false });
      await f.batch();
      const held = await disabled.getDelivery({ id: beforeDisable.deliveryIds[0]! }, f.ctx);
      expect(held.state).toBe("pending");
      expect(held.attemptCount).toBe(0);
      expect(f.received).toHaveLength(count);
      const disabledPublish = await disabled.publish({ type: "order.created", data: {} }, f.ctx);
      expect(disabledPublish.dispatch).toBe("disabled");
      f.recreate();
      await f.service.recover();
      await f.settle(held.id, "succeeded");
      await f.settle(disabledPublish.deliveryIds[0]!, "succeeded");
    } finally { await f.close(); }
  }, 30_000);

  test("recovers persisted enqueue failures and expired claims, fencing stale finishes and duplicate execution", async () => {
    const f = await fixture();
    try {
      await f.endpoint("/recovery");
      const brokenQueue: TaskQueue = {
        ...f.queue, async enqueue() { throw new Error("postgres://private:credential@queue.invalid/secret"); },
      };
      f.recreate(brokenQueue);
      const published = await f.publish();
      expect(published.dispatch).toBe("recovery-required");
      expect(JSON.stringify(published)).not.toContain("credential");
      expect(JSON.stringify(published)).not.toContain("queue.invalid");
      const id = published.deliveryIds[0]!;
      const recreated = f.recreate();
      expect((await recreated.getDelivery({ id }, f.ctx)).eventId).toBe(published.eventId);
      expect((await recreated.recover()).recoveryRequired).toBe(0);
      const delivery = await recreated.getDelivery({ id }, f.ctx);
      const healthyKey = `webhooks:${id}:${delivery.generation}`;
      const accepted = await f.queue.lookupDeduplicationKey(healthyKey);
      for (let index = 0; index < 3; index++) {
        expect(await recreated.recover()).toEqual({ scheduled: 0, recoveryRequired: 0 });
        expect((await recreated.getDelivery({ id }, f.ctx)).generation).toBe(delivery.generation);
        expect((await f.queue.lookupDeduplicationKey(healthyKey))?.jobId).toBe(accepted?.jobId);
      }
      await f.queue.enqueue(f.task, { deliveryId: id, generation: delivery.generation });
      await f.queue.enqueue(f.task, { deliveryId: id, generation: delivery.generation });
      await Promise.all([
        recreated.execute({ deliveryId: id, generation: delivery.generation }, new AbortController().signal),
        recreated.execute({ deliveryId: id, generation: delivery.generation }, new AbortController().signal),
        f.batch(),
      ]);
      expect((await f.settle(id, "succeeded")).attemptCount).toBe(1);
      expect(f.received.filter(request => request.event.id === published.eventId)).toHaveLength(1);

      f.recreate(brokenQueue);
      const abandoned = await f.publish();
      const abandonedId = abandoned.deliveryIds[0]!;
      const repository = f.repository();
      const pending = await repository.getDelivery(f.scope, abandonedId);
      const staleToken = uuid();
      const claim = await repository.claim(abandonedId, pending!.generation, staleToken, Date.now(), 1);
      expect(claim).not.toBeNull();
      await Bun.sleep(5);
      f.recreate();
      await f.service.recover();
      const recovered = await f.service.getDelivery({ id: abandonedId }, f.ctx);
      expect(recovered.generation).toBeGreaterThan(pending!.generation);
      expect(await repository.finish(abandonedId, staleToken, {
        code: "success", status: 204, keyId: key.id, retryAt: null,
      }, Date.now())).toBeNull();
      expect(await repository.claim(abandonedId, pending!.generation, uuid(), Date.now(), 100)).toBeNull();
      await f.settle(abandonedId, "succeeded");
      const attempts = await f.service.listAttempts({ deliveryId: abandonedId }, f.ctx);
      expect(attempts.items.map(attempt => attempt.code).sort()).toEqual(["lease-expired", "success"]);
      expect(f.received.filter(request => request.event.id === abandoned.eventId)).toHaveLength(1);
      expect(await repository.finish(abandonedId, staleToken, {
        code: "server-error", status: 503, keyId: key.id, retryAt: Date.now(),
      }, Date.now())).toBeNull();
      expect((await f.service.getDelivery({ id: abandonedId }, f.ctx)).state).toBe("succeeded");
    } finally { await f.close(); }
  }, 30_000);

  test("pages safely, redacts stored payloads and validation errors, and prunes terminal history without active records", async () => {
    const f = await fixture();
    try {
      const endpoint = await f.endpoint("/retention");
      const repository = f.repository();
      const old = Date.now() - 2 * 86_400_000;
      const event = (id: string): EventEnvelope => ({
        version: 1, id, type: "order.created", occurredAt: new Date(old).toISOString(),
        source: "retention-fixture", data: { password: "never-expose-payload" },
      });
      const activeEvent = event(uuid());
      const terminalEvent = event(uuid());
      const active = (await repository.publish(f.scope, activeEvent, JSON.stringify(activeEvent), 3, old))[0]!;
      const terminal = (await repository.publish(f.scope, terminalEvent, JSON.stringify(terminalEvent), 3, old))[0]!;
      const token = uuid();
      await repository.claim(terminal.id, terminal.generation, token, old, 100);
      await repository.finish(terminal.id, token, { code: "success", status: 204, keyId: key.id, retryAt: null }, old + 1);
      const seen = new Set<string>();
      let cursor: { createdAt: number; id: string } | undefined;
      do {
        const page = await f.service.listDeliveries({ limit: 1, ...(cursor ? { cursor } : {}) }, f.ctx);
        for (const item of page.items) {
          expect(seen.has(item.id)).toBe(false);
          seen.add(item.id);
          const publicJson = JSON.stringify(item);
          for (const sensitive of ["never-expose-payload", secretRef, endpointHost, "leaseToken", "body"]) {
            expect(publicJson).not.toContain(sensitive);
          }
        }
        cursor = page.nextCursor ?? undefined;
      } while (cursor);
      expect(seen).toEqual(new Set([active.id, terminal.id]));
      const unicode = await f.publish({ escaped: "\u0000\ud800" });
      expect(unicode.dispatch).toBe("queued");
      const publicEndpoint = JSON.stringify(await f.service.getEndpoint({ id: endpoint.id }, f.ctx));
      expect(publicEndpoint).not.toContain(secretRef);
      expect(publicEndpoint).not.toContain(endpointHost);
      await expect(f.service.listDeliveries({ limit: 101 }, f.ctx)).rejects.toMatchObject({ code: "invalid-input", message: "Webhook invalid-input" });
      await expect(f.service.publish({ type: "order.created", data: { secret: Infinity } }, f.ctx)).rejects.toMatchObject({ code: "invalid-input" });
      await f.service.prune({ limit: 500 });
      expect(await repository.getDelivery(f.scope, terminal.id)).toBeNull();
      expect(await repository.getDelivery(f.scope, active.id)).toMatchObject({ state: "pending", eventId: activeEvent.id });
      expect(await repository.listAttempts(f.scope, terminal.id, { limit: 10 })).toEqual([]);
      // A new repository still owns the active event and its snapshot after prune.
      f.recreate();
      await f.queue.enqueue(f.task, { deliveryId: active.id, generation: active.generation });
      await f.settle(active.id, "succeeded");
      expect(f.received.some(request => request.event.id === activeEvent.id)).toBe(true);
    } finally { await f.close(); }
  }, 30_000);

  test("does not send after a slow persisted claim outlives its delivery lease", async () => {
    const f = await fixture();
    try {
      await f.endpoint("/slow-claim");
      const repository = f.repository();
      f.recreate(f.queue, {}, {
        ...repository,
        async claim(...args) {
          const claimed = await repository.claim(...args);
          if (claimed) await Bun.sleep(10_400);
          return claimed;
        },
      });
      const published = await f.publish();
      const id = published.deliveryIds[0]!;
      const pending = await repository.getDelivery(f.scope, id);
      await f.service.execute({ deliveryId: id, generation: pending!.generation }, new AbortController().signal);
      expect(f.received).toHaveLength(0);
      expect((await repository.getDelivery(f.scope, id))?.state).toBe("running");
      f.recreate();
      await f.service.recover();
      await f.settle(id, "succeeded");
      expect((await f.service.listAttempts({ deliveryId: id }, f.ctx)).items.map(attempt => attempt.code))
        .toEqual(["lease-expired", "success"]);
      expect(f.received).toHaveLength(1);
    } finally { await f.close(); }
  }, 30_000);

  test("isolates schema owners and preserves escaped JSON strings", async () => {
    const f = await fixture();
    try {
      const schema = `webhooks_${uuid().replaceAll("-", "")}`;
      await f.pool.query(`CREATE SCHEMA "${schema}"`);
      const migration = await readFile(new URL("../migrations/0001_webhooks.sql", import.meta.url), "utf8");
      await f.pool.query(migration.replaceAll("__LENSO_WEBHOOK_SCHEMA__", `"${schema}"`));
      const isolated = createPostgresWebhookRepository({ pool: f.pool, schema });
      const endpointId = uuid();
      await isolated.putEndpoint({
        id: endpointId, scope: f.scope, url: `https://${endpointHost}/isolated`,
        secretRef, enabled: true,
      }, Date.now());
      await isolated.putSubscription({
        id: uuid(), scope: f.scope, endpointId, eventType: "order.created", enabled: true,
      }, Date.now());
      const event: EventEnvelope = {
        version: 1, id: uuid(), type: "order.created", source: "isolated",
        occurredAt: new Date().toISOString(), data: { text: "\u0000\ud800" },
      };
      const published = await isolated.publish(f.scope, event, JSON.stringify(event), 3, Date.now());
      expect(published).toHaveLength(1);
      expect(await f.repository().getEndpoint(f.scope, endpointId)).toBeNull();
      expect(await f.repository().getDelivery(f.scope, published[0]!.id)).toBeNull();
      expect(await f.repository().recover(Date.now(), 500)).toEqual([]);
      expect((await isolated.recover(Date.now(), 500))[0]?.eventId).toBe(event.id);
      const claimed = await isolated.claim(published[0]!.id, published[0]!.generation, uuid(), Date.now(), 10_000);
      expect(JSON.parse(claimed!.delivery.body).data).toEqual(event.data);
      expect(() => createPostgresWebhookRepository({ pool: f.pool, schema: 'public";DROP SCHEMA public' }))
        .toThrow(WebhookError);
    } finally { await f.close(); }
  }, 30_000);

  test("requires replay permission and durable intent, and preserves uncertain replay references", async () => {
    const f = await fixture();
    try {
      await f.endpoint("/audit-boundary");
      const published = await f.publish();
      const id = published.deliveryIds[0]!;
      await f.settle(id, "succeeded");
      f.deniedActions.add("replay");
      await expect(f.service.replay({ id }, f.ctx)).rejects.toMatchObject({ code: "unauthorized" });
      expect((await f.audit.query({ scope: f.scope }, f.scope)).events).toEqual([]);
      f.deniedActions.delete("replay");
      const prepare = f.audit.prepare;
      f.audit.prepare = async () => { throw new Error("sensitive-audit-driver-error"); };
      await expect(f.service.replay({ id }, f.ctx)).rejects.toMatchObject({ code: "audit-failed" });
      expect((await f.service.listDeliveries({}, f.ctx)).items).toHaveLength(1);
      f.audit.prepare = prepare;

      const complete = f.audit.complete;
      f.audit.complete = async () => { throw new Error("sensitive-audit-driver-error"); };
      let referenceId: string | undefined;
      try {
        await f.service.replay({ id }, f.ctx);
        throw new Error("Expected uncertain replay outcome");
      } catch (error) {
        expect(error).toBeInstanceOf(WebhookError);
        const failure = error as WebhookError;
        expect(failure.code).toBe("replay-outcome-unknown");
        expect(failure.message).not.toContain("sensitive-audit-driver-error");
        referenceId = failure.referenceId;
      }
      f.audit.complete = complete;
      expect(referenceId).toBeDefined();
      const replay = await f.service.getDelivery({ id: referenceId! }, f.ctx);
      expect(replay.replayOf).toBe(id);
      expect(replay.eventId).toBe(published.eventId);
      const history = await f.audit.query({ scope: f.scope }, f.scope);
      expect(history.events).toHaveLength(1);
      expect(history.events[0]?.result).toBe("intent");
      expect(history.events[0]?.id).toBe(replay.auditIntentId!);
      await f.service.recover();
      await f.settle(referenceId!, "succeeded");
    } finally { await f.close(); }
  }, 30_000);
});
