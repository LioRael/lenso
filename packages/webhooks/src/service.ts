import { z } from "zod";
import {
  WebhookError,
  type Attempt,
  type AttemptCode,
  type Delivery,
  type Endpoint,
  type PageInput,
  type Page,
  type WebhookAction,
  type WebhookContext,
  type WebhookOptions,
} from "./contracts";
import { webhookConfig } from "./config";
import { OutboundError, validateEndpointUrl } from "./network";
import { signWebhook } from "./signing";

const uuid = z.uuid();
const scopeSchema = z.object({ tenantId: z.string().min(1).max(256), scopeId: z.string().min(1).max(256) }).strict();
const pageSchema = z.object({
  limit: z.number().int().min(1).max(100).default(50),
  cursor: z.object({ createdAt: z.number().int().nonnegative(), id: uuid }).strict().optional(),
}).strict();
export const endpointInput = z.object({
  id: uuid,
  url: z.string().min(1).max(2048),
  secretRef: z.string().min(1).max(256),
  enabled: z.boolean(),
}).strict();
export const subscriptionInput = z.object({
  id: uuid,
  endpointId: uuid,
  eventType: z.string().min(1).max(128),
  enabled: z.boolean(),
}).strict();
export const publishInput = z.object({
  type: z.string().min(1).max(128),
  data: z.unknown(),
}).strict();

function parse<S extends z.ZodType>(schema: S, value: unknown): z.output<S> {
  const result = schema.safeParse(value);
  if (!result.success) throw new WebhookError("invalid-input");
  return result.data;
}
function endpointView(endpoint: Endpoint) {
  return {
    id: endpoint.id, scope: endpoint.scope, enabled: endpoint.enabled,
    revision: endpoint.revision, createdAt: endpoint.createdAt,
  };
}
function page<T>(rows: readonly T[], limit: number, cursor: (row: T) => NonNullable<PageInput["cursor"]>): Page<T> {
  const items = rows.slice(0, limit);
  return { items, nextCursor: rows.length > limit && items.length ? cursor(items.at(-1)!) : null };
}
/** Retry-After is advisory, finite, and never makes a retry exceed the configured cap. */
export function retryDelay(attempt: number, baseMs: number, capMs: number, retryAfter: string | null, now: number): number {
  const ceiling = Math.min(capMs, baseMs * 2 ** Math.min(attempt - 1, 30));
  const jittered = Math.floor(ceiling / 2 + Math.random() * ceiling / 2);
  let advisory = 0;
  if (retryAfter && retryAfter.length <= 128) {
    if (/^\d{1,8}$/.test(retryAfter)) advisory = Number(retryAfter) * 1000;
    else {
      const date = Date.parse(retryAfter);
      if (Number.isFinite(date)) advisory = Math.max(0, date - now);
    }
  }
  return Math.min(capMs, Math.max(jittered, advisory));
}

function copyPayload(value: unknown): import("./contracts").JsonValue {
  let nodes = 0;
  function visit(input: unknown, depth: number): import("./contracts").JsonValue {
    if (++nodes > 50_000 || depth > 32) throw new WebhookError("invalid-input");
    if (input === null || typeof input === "boolean" || typeof input === "string") return input;
    if (typeof input === "number" && Number.isFinite(input)) return input;
    if (Array.isArray(input)) return input.map(item => visit(item, depth + 1));
    if (typeof input === "object" && input !== null && Object.getPrototypeOf(input) === Object.prototype) {
      return Object.fromEntries(Object.entries(input).map(([key, item]) => [key, visit(item, depth + 1)]));
    }
    throw new WebhookError("invalid-input");
  }
  try { return visit(value, 0); } catch { throw new WebhookError("invalid-input"); }
}

export function createWebhooks<P>(options: WebhookOptions<P>) {
  const config = webhookConfig(options.config);
  const { repository, queue, task } = options;
  async function authorize(context: WebhookContext<P>, action: WebhookAction) {
    const scope = parse(scopeSchema, context?.scope);
    try { await options.authority.authorize(context.principal, scope, action); }
    catch { throw new WebhookError("unauthorized"); }
    return scope;
  }
  async function storage<T>(work: () => Promise<T>): Promise<T> {
    try { return await work(); }
    catch (error) {
      if (error instanceof WebhookError) throw error;
      throw new WebhookError("storage-failed");
    }
  }
  async function schedule(delivery: Delivery) {
    try {
      await queue.enqueue(task, { deliveryId: delivery.id, generation: delivery.generation }, {
        runAt: new Date(delivery.dueAt),
        deduplicationKey: `webhooks:${delivery.id}:${delivery.generation}`,
      });
      return true;
    } catch { return false; }
  }
  const createdCursor = (row: { createdAt: number; id: string }) => ({ createdAt: row.createdAt, id: row.id });
  return {
    async putEndpoint(input: z.input<typeof endpointInput>, context: WebhookContext<P>) {
      const scope = await authorize(context, "configure");
      const data = parse(endpointInput, input);
      let url: string;
      try { url = validateEndpointUrl(data.url, config.outbound); }
      catch { throw new WebhookError("invalid-input"); }
      return endpointView(await storage(() => repository.putEndpoint({ ...data, url, scope }, Date.now())));
    },
    async getEndpoint(input: { id: string }, context: WebhookContext<P>) {
      const scope = await authorize(context, "read");
      const id = parse(z.object({ id: uuid }).strict(), input).id;
      const endpoint = await storage(() => repository.getEndpoint(scope, id));
      if (!endpoint) throw new WebhookError("not-found");
      return endpointView(endpoint);
    },
    async listEndpoints(input: PageInput, context: WebhookContext<P>) {
      const scope = await authorize(context, "read");
      const query = parse(pageSchema, input);
      const rows = await storage(() => repository.listEndpoints(scope, { ...query, limit: query.limit + 1 }));
      return page(rows.map(endpointView), query.limit, createdCursor);
    },
    async putSubscription(input: z.input<typeof subscriptionInput>, context: WebhookContext<P>) {
      const scope = await authorize(context, "configure");
      const data = parse(subscriptionInput, input);
      if (!config.eventTypes.includes(data.eventType)) throw new WebhookError("invalid-input");
      return storage(() => repository.putSubscription({ ...data, scope }, Date.now()));
    },
    async listSubscriptions(input: PageInput, context: WebhookContext<P>) {
      const scope = await authorize(context, "read");
      const query = parse(pageSchema, input);
      return page(await storage(() => repository.listSubscriptions(scope, { ...query, limit: query.limit + 1 })), query.limit, createdCursor);
    },
    async publish(input: z.input<typeof publishInput>, context: WebhookContext<P>) {
      const scope = await authorize(context, "publish");
      const data = parse(publishInput, input);
      if (!config.eventTypes.includes(data.type)) throw new WebhookError("invalid-input");
      const now = Date.now();
      const event = {
        version: 1 as const, id: crypto.randomUUID(), type: data.type,
        occurredAt: new Date(now).toISOString(), source: config.source, data: copyPayload(data.data),
      };
      const body = JSON.stringify(event);
      if (new TextEncoder().encode(body).byteLength > config.outbound.maxRequestBytes) throw new WebhookError("invalid-input");
      const deliveries = await storage(() => repository.publish(scope, event, body, config.maxAttempts, now));
      const accepted = config.enabled ? await Promise.all(deliveries.map(schedule)) : [];
      return {
        eventId: event.id,
        deliveryIds: deliveries.map(delivery => delivery.id),
        dispatch: !config.enabled ? "disabled" as const : accepted.every(Boolean) ? "queued" as const : "recovery-required" as const,
      };
    },
    async getDelivery(input: { id: string }, context: WebhookContext<P>) {
      const scope = await authorize(context, "read");
      const id = parse(z.object({ id: uuid }).strict(), input).id;
      const delivery = await storage(() => repository.getDelivery(scope, id));
      if (!delivery) throw new WebhookError("not-found");
      return delivery;
    },
    async listDeliveries(input: PageInput, context: WebhookContext<P>) {
      const scope = await authorize(context, "read");
      const query = parse(pageSchema, input);
      return page(await storage(() => repository.listDeliveries(scope, { ...query, limit: query.limit + 1 })), query.limit, createdCursor);
    },
    async listAttempts(input: PageInput & { deliveryId: string }, context: WebhookContext<P>) {
      const scope = await authorize(context, "read");
      const query = parse(pageSchema.extend({ deliveryId: uuid }), input);
      if (!await storage(() => repository.getDelivery(scope, query.deliveryId))) throw new WebhookError("not-found");
      return page(await storage(() => repository.listAttempts(scope, query.deliveryId, { ...query, limit: query.limit + 1 })), query.limit,
        (row: Attempt) => ({ createdAt: row.startedAt, id: row.id }));
    },
    async replay(input: { id: string }, context: WebhookContext<P>) {
      const scope = await authorize(context, "replay");
      const id = parse(z.object({ id: uuid }).strict(), input).id;
      if (!config.enabled) throw new WebhookError("disabled");
      const original = await storage(() => repository.getDelivery(scope, id));
      if (!original) throw new WebhookError("not-found");
      if (!["succeeded", "failed"].includes(original.state)) throw new WebhookError("conflict");
      const newId = crypto.randomUUID();
      let prepared;
      try {
        prepared = await options.audit.prepare({
          id: crypto.randomUUID(), occurredAt: Date.now(), scope,
          action: "webhooks.replay", target: { type: "webhook-delivery", id: newId },
          result: "intent", reasonCode: "manual-replay", correlationId: id,
        }, context.principal);
      } catch { throw new WebhookError("audit-failed"); }
      if (prepared.status !== "ready") throw new WebhookError("audit-failed");
      let delivery: Delivery;
      try {
        delivery = await storage(() => repository.replay(scope, id, newId, prepared.receipt.intentId, Date.now()));
      } catch (error) {
        // A failed commit may still have committed. No compensating delete or blind replay.
        const rejected = error instanceof WebhookError && ["conflict", "not-found", "disabled"].includes(error.code);
        try { await options.audit.complete(prepared.receipt, {
          id: crypto.randomUUID(), occurredAt: Date.now(),
          result: rejected ? "failure" : "unknown",
          reasonCode: rejected ? "replay-rejected" : "reconcile-delivery",
        }); } catch { throw new WebhookError("replay-outcome-unknown", newId); }
        if (rejected) throw error;
        throw new WebhookError("replay-outcome-unknown", newId);
      }
      try {
        await options.audit.complete(prepared.receipt, {
          id: crypto.randomUUID(), occurredAt: Date.now(), result: "success", reasonCode: "delivery-created",
        });
      } catch { throw new WebhookError("replay-outcome-unknown", newId); }
      return { delivery, dispatch: await schedule(delivery) ? "queued" as const : "recovery-required" as const };
    },
    /** Worker-only trusted entry, not an operation exposed to business callers. */
    async execute(input: { deliveryId: string; generation: number }, signal: AbortSignal) {
      if (!config.enabled || signal.aborted) return;
      const leaseMs = config.outbound.timeoutMs + 10_000;
      // Fail-open Limits cannot grant permission to dispatch without a real lease.
      const acquisition = await options.limits.acquire({
        scope: { instance: config.instanceId, tenant: "_webhooks", key: task.name },
        capacity: config.concurrency, quantity: 1, ttlMs: leaseMs,
      });
      if (!acquisition.allowed || !acquisition.lease) return;
      try {
        const claim = await storage(() => repository.claim(input.deliveryId, input.generation, crypto.randomUUID(), Date.now(), leaseMs));
        if (!claim) return;
        const delivery = claim.delivery;
        // Claim may have waited on database locks longer than our admission lease.
        const renewed = await options.limits.renew(acquisition.lease, leaseMs);
        if (!renewed || signal.aborted) return;
        // A lock wait must not give HTTP a new deadline beyond the persisted delivery lease.
        const remaining = Math.min(delivery.leaseUntil!, renewed.expiresAt) - Date.now() - 1000;
        if (remaining <= 0) return;
        const deadline = AbortSignal.any([signal, AbortSignal.timeout(Math.min(config.outbound.timeoutMs, remaining))]);
        let code: AttemptCode = "key-unavailable";
        let status: number | null = null;
        let keyId: string | null = null;
        let retryAfter: string | null = null;
        try {
          const key = await abortable(options.keys.active(delivery.secretRef, delivery.scope), deadline);
          const headers = signWebhook(new TextEncoder().encode(delivery.body), delivery.eventId, Math.floor(Date.now() / 1000), key);
          keyId = key.id;
          const result = await abortable(options.transport.send({
            url: delivery.url, body: new TextEncoder().encode(delivery.body),
            headers: { ...headers, "content-type": "application/json" }, signal: deadline,
          }), deadline);
          status = result.status;
          retryAfter = result.retryAfter;
          code = status >= 200 && status < 300 ? "success" : status === 429 ? "rate-limited" : status >= 500 && status <= 599 ? "server-error" : "permanent-http";
        } catch (error) {
          code = deadline.aborted ? "timeout" : error instanceof OutboundError ? error.code :
            keyId === null ? "key-unavailable" : "connection-failed";
        }
        const retryable = ["timeout", "connection-failed", "rate-limited", "server-error", "key-unavailable"].includes(code);
        const now = Date.now();
        const finished = await storage(() => repository.finish(delivery.id, delivery.leaseToken!, {
          code, status, keyId, retryAt: retryable ? now + retryDelay(delivery.attemptCount, config.baseDelayMs, config.maxDelayMs, retryAfter, now) : null,
        }, now));
        if (finished?.state === "retry") await schedule(finished);
      } finally {
        await options.limits.release(acquisition.lease);
      }
    },
    /** Host-owned startup/periodic maintenance, bounded and without its own scheduler. */
    async recover(input: { limit?: number } = {}) {
      if (!config.enabled) return { scheduled: 0, recoveryRequired: 0 };
      const limit = parse(z.object({ limit: z.number().int().min(1).max(500).default(100) }).strict(), input).limit;
      const rows = await storage(() => repository.recover(Date.now(), limit));
      let scheduled = 0;
      let recoveryRequired = 0;
      for (const row of rows) {
        try {
          const accepted = await queue.lookupDeduplicationKey(`webhooks:${row.id}:${row.generation}`);
          if (accepted?.status && ["pending", "running"].includes(accepted.status.state)) continue;
          const candidate = accepted ? await storage(() => repository.advanceSchedule(row.id, row.generation, Date.now())) : row;
          if (!candidate) continue;
          if (await schedule(candidate)) scheduled++;
          else recoveryRequired++;
        } catch { recoveryRequired++; }
      }
      return { scheduled, recoveryRequired };
    },
    async prune(input: { limit?: number } = {}) {
      const limit = parse(z.object({ limit: z.number().int().min(1).max(500).default(100) }).strict(), input).limit;
      return storage(() => repository.prune(Date.now() - config.retentionMs, limit));
    },
  };
}

function abortable<T>(promise: Promise<T>, signal: AbortSignal): Promise<T> {
  return new Promise((resolve, reject) => {
    const abort = () => reject(new OutboundError("timeout"));
    if (signal.aborted) abort();
    else signal.addEventListener("abort", abort, { once: true });
    // Observe late rejection even when cancellation won before the caller's promise settled.
    void promise.then(resolve, reject).finally(() => signal.removeEventListener("abort", abort));
  });
}

export type Webhooks<P> = ReturnType<typeof createWebhooks<P>>;
