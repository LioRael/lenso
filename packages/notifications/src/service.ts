import type {
  DeliveryAttempt,
  DeliveryResult,
  NotificationChannel,
  NotificationFilter,
  NotificationInput,
  NotificationRecord,
  NotificationStore,
  NotificationTemplate,
  Preference,
} from "./contracts";
import { NotificationError } from "./errors";
import { canonical, digest, email, identifier, renderTemplate } from "./render";

export function notificationSummary(record: NotificationRecord) {
  return {
    id: record.id,
    templateId: record.templateId,
    templateVersion: record.templateVersion,
    channelId: record.channelId,
    state: record.state,
    attemptCount: record.attemptCount,
    providerMessageId: record.providerMessageId,
    error: record.error,
    retryable: record.retryable,
    createdAt: record.createdAt,
    updatedAt: record.updatedAt,
  };
}
export type NotificationSummary = ReturnType<typeof notificationSummary>;

export interface NotificationServiceOptions {
  store: NotificationStore;
  templates: readonly NotificationTemplate[];
  channels: readonly NotificationChannel[];
  /** Optional notifications default to disabled unless business explicitly opts in. */
  optionalDefault?: "enabled" | "disabled";
  leaseMs?: number;
  clock?: () => number;
  onDelivery?: (event: {
    notificationId: string;
    state: string;
    attempt: number;
    error: string | null;
  }) => void;
}

function boundedLimit(limit: number) {
  if (!Number.isInteger(limit) || limit < 1 || limit > 100) {
    throw new NotificationError("invalid-input");
  }
}

export function createNotificationService(options: NotificationServiceOptions) {
  const store = options.store;
  const clock = options.clock ?? Date.now;
  const leaseMs = options.leaseMs ?? 120_000;
  if (
    !Number.isSafeInteger(leaseMs) ||
    leaseMs < 1000 ||
    leaseMs > 3_600_000 ||
    (options.optionalDefault !== undefined &&
      !["enabled", "disabled"].includes(options.optionalDefault))
  ) {
    throw new NotificationError("invalid-input");
  }
  const channels = new Map<string, NotificationChannel>();
  for (const channel of options.channels) {
    identifier(channel.id);
    if (
      channels.has(channel.id) ||
      channel.kind !== "email" ||
      !Number.isSafeInteger(channel.idempotencyWindowMs) ||
      channel.idempotencyWindowMs <= leaseMs + 60_000
    ) {
      throw new NotificationError("channel-unavailable");
    }
    channels.set(channel.id, channel);
  }
  const templates = new Map<string, NotificationTemplate>();
  for (const template of options.templates) {
    for (const value of [template.id, template.version, template.category]) identifier(value);
    const key = canonical([template.id, template.version]);
    if (
      templates.has(key) ||
      !["required", "optional"].includes(template.necessity) ||
      !template.channels.length ||
      template.channels.some((id) => !channels.has(id)) ||
      typeof template.variables?.["~standard"]?.validate !== "function" ||
      typeof template.subject !== "string" ||
      typeof template.text !== "string"
    ) {
      throw new NotificationError("invalid-template");
    }
    email(template.from);
    templates.set(
      key,
      Object.freeze({ ...template, channels: Object.freeze([...template.channels]) }),
    );
  }
  async function enabled(
    record: Pick<Preference, "tenantId" | "recipientId" | "category" | "channelId">,
  ) {
    const preference = await store.getPreference(record);
    return preference?.enabled ?? options.optionalDefault === "enabled";
  }
  async function save(
    record: NotificationRecord,
    patch: Partial<NotificationRecord>,
    attempt?: DeliveryAttempt,
  ) {
    const next = { ...record, ...patch, revision: record.revision + 1, updatedAt: clock() };
    if (!(await store.save(next, record.revision, attempt)))
      throw new NotificationError("delivery-busy");
    return next;
  }
  function report(record: NotificationRecord) {
    try {
      options.onDelivery?.({
        notificationId: record.id,
        state: record.state,
        attempt: record.attemptCount,
        error: record.error,
      });
    } catch {
      // Observability must not turn an accepted external effect into a failed attempt.
    }
    return notificationSummary(record);
  }
  return {
    async create(raw: NotificationInput): Promise<NotificationSummary> {
      if (!raw || typeof raw !== "object" || Array.isArray(raw))
        throw new NotificationError("invalid-input");
      const allowed = new Set([
        "tenantId",
        "scope",
        "idempotencyKey",
        "businessId",
        "recipientId",
        "email",
        "templateId",
        "templateVersion",
        "variables",
        "channels",
      ]);
      if (Object.keys(raw).some((key) => !allowed.has(key)))
        throw new NotificationError("invalid-input");
      for (const key of [
        "tenantId",
        "scope",
        "idempotencyKey",
        "businessId",
        "recipientId",
        "templateId",
        "templateVersion",
      ] as const)
        identifier(raw[key]);
      email(raw.email);
      const serialized = canonical({
        ...raw,
        channels: raw.channels ?? null,
      });
      if (new TextEncoder().encode(serialized).length > 64 * 1024)
        throw new NotificationError("invalid-input");
      // Snapshot before async validation; callers cannot mutate the input while it is in flight.
      const input = JSON.parse(serialized) as NotificationInput;
      const fingerprint = await digest(serialized);
      const existing = await store.findKey(input.tenantId, input.scope, input.idempotencyKey);
      if (existing) {
        if (existing.fingerprint !== fingerprint)
          throw new NotificationError("idempotency-conflict");
        return notificationSummary(existing);
      }
      const template = templates.get(canonical([input.templateId, input.templateVersion]));
      if (!template) throw new NotificationError("invalid-template");
      const candidates = input.channels ?? template.channels;
      if (
        !Array.isArray(candidates) ||
        !candidates.length ||
        candidates.some((id) => !template.channels.includes(id) || !channels.has(id))
      ) {
        throw new NotificationError("channel-unavailable");
      }
      const message = await renderTemplate(template, input.variables, input.email);
      let channelId = candidates[0];
      let permitted = template.necessity === "required";
      for (const candidate of candidates) {
        if (
          permitted ||
          (await enabled({
            tenantId: input.tenantId,
            recipientId: input.recipientId,
            category: template.category,
            channelId: candidate,
          }))
        ) {
          channelId = candidate;
          permitted = true;
          break;
        }
      }
      const now = clock();
      const record = await store.insertOrGet({
        id: crypto.randomUUID(),
        tenantId: input.tenantId,
        scope: input.scope,
        idempotencyKey: input.idempotencyKey,
        fingerprint,
        businessId: input.businessId,
        recipientId: input.recipientId,
        templateId: template.id,
        templateVersion: template.version,
        category: template.category,
        necessity: template.necessity,
        channelId,
        message,
        state: permitted ? "pending" : "suppressed",
        revision: 0,
        attemptCount: 0,
        firstRequestAt: null,
        leaseUntil: null,
        taskJobId: null,
        providerMessageId: null,
        error: null,
        retryable: permitted,
        createdAt: now,
        updatedAt: now,
      });
      if (record.fingerprint !== fingerprint) throw new NotificationError("idempotency-conflict");
      return notificationSummary(record);
    },
    async deliver(
      id: string,
      context: { signal?: AbortSignal } = {},
    ): Promise<NotificationSummary | null> {
      identifier(id);
      context.signal?.throwIfAborted();
      let record = await store.get(id);
      if (!record) return null;
      if (
        ["accepted", "delivered", "suppressed"].includes(record.state) ||
        (record.state !== "sending" && !record.retryable)
      )
        return notificationSummary(record);
      if (record.state === "sending") {
        if (record.leaseUntil === null || record.leaseUntil > clock())
          throw new NotificationError("delivery-busy");
        const previous = (await store.attempts(id)).find(
          (attempt) => attempt.number === record!.attemptCount,
        );
        if (!previous) throw new NotificationError("delivery-busy");
        record = await save(
          record,
          { state: "unknown", leaseUntil: null, retryable: true, error: "transport-unknown" },
          {
            ...previous,
            state: "unknown",
            error: "transport-unknown",
            finishedAt: clock(),
          },
        );
      }
      const channel = channels.get(record.channelId);
      if (!channel) throw new NotificationError("channel-unavailable");
      // Reserve time for a request to reach the provider before its documented key expiry.
      if (
        record.firstRequestAt !== null &&
        clock() + leaseMs + 60_000 >= record.firstRequestAt + channel.idempotencyWindowMs
      ) {
        return report(
          await save(record, {
            error: "deduplication-expired",
            retryable: false,
            state: record.state === "unknown" ? "unknown" : "failed",
            leaseUntil: null,
          }),
        );
      }
      if (record.necessity === "optional" && !(await enabled(record))) {
        return report(
          await save(record, {
            state: record.state === "unknown" ? "unknown" : "suppressed",
            retryable: false,
            leaseUntil: null,
          }),
        );
      }
      context.signal?.throwIfAborted();
      const wasUncertain = record.state === "unknown";
      const now = clock();
      const attempt: DeliveryAttempt = {
        id: crypto.randomUUID(),
        notificationId: id,
        number: record.attemptCount + 1,
        state: "sending",
        startedAt: now,
        finishedAt: null,
        providerMessageId: null,
        error: null,
      };
      record = await save(
        record,
        {
          state: "sending",
          attemptCount: attempt.number,
          firstRequestAt: record.firstRequestAt ?? now,
          leaseUntil: now + leaseMs,
          error: null,
          retryable: true,
        },
        attempt,
      );
      let outcome: DeliveryResult;
      const remainingMs = record.leaseUntil! - clock();
      // Database work can consume the expiry margin or the entire claim before HTTP starts.
      if (clock() + leaseMs + 60_000 >= record.firstRequestAt! + channel.idempotencyWindowMs) {
        outcome = { state: "failed", code: "deduplication-expired", retryable: false };
      } else if (remainingMs <= 0) {
        throw new NotificationError("delivery-busy");
      } else {
        try {
          outcome = await channel.send(record.message, {
            idempotencyKey: `lenso-notification/${record.id}`,
            signal: AbortSignal.any([
              AbortSignal.timeout(remainingMs),
              ...(context.signal ? [context.signal] : []),
            ]),
          });
        } catch {
          outcome = { state: "unknown", code: "transport-unknown", retryable: true };
        }
      }
      const resultFields =
        outcome.state === "accepted"
          ? { providerMessageId: outcome.providerMessageId, error: null, retryable: false }
          : { providerMessageId: null, error: outcome.code, retryable: outcome.retryable };
      const finished = await save(
        record,
        {
          // A definite rejection of this attempt does not resolve an earlier ambiguous effect.
          state: wasUncertain && outcome.state === "failed" ? "unknown" : outcome.state,
          leaseUntil: null,
          ...resultFields,
        },
        {
          ...attempt,
          state: outcome.state,
          finishedAt: clock(),
          providerMessageId: resultFields.providerMessageId,
          error: resultFields.error,
        },
      );
      return report(finished);
    },
    /** Trusted receipt processing only: acceptance alone must never invoke this method. */
    async markDelivered(id: string, providerMessageId: string): Promise<boolean> {
      identifier(id);
      identifier(providerMessageId);
      const record = await store.get(id);
      if (!record || record.providerMessageId !== providerMessageId || record.state !== "accepted")
        return false;
      await save(record, { state: "delivered", retryable: false });
      return true;
    },
    async get(id: string) {
      identifier(id);
      const record = await store.get(id);
      return record ? notificationSummary(record) : null;
    },
    /** Trusted authorization adapters need immutable ownership, never expose this over JSON. */
    async getRecord(id: string) {
      identifier(id);
      return store.get(id);
    },
    async retryEligible(id: string): Promise<boolean> {
      identifier(id);
      const record = await store.get(id);
      if (!record) return false;
      return record.state === "sending"
        ? record.leaseUntil !== null && record.leaseUntil <= clock()
        : ["failed", "unknown"].includes(record.state) && record.retryable;
    },
    async list(filter: NotificationFilter) {
      identifier(filter.tenantId);
      if (filter.recipientId !== undefined) identifier(filter.recipientId);
      boundedLimit(filter.limit);
      return (await store.list(filter)).map(notificationSummary);
    },
    async attempts(id: string) {
      identifier(id);
      return store.attempts(id);
    },
    async getPreference(key: Omit<Preference, "enabled">) {
      Object.values(key).forEach(identifier);
      return { ...key, enabled: await enabled(key) };
    },
    async setPreference(preference: Preference) {
      for (const value of [
        preference.tenantId,
        preference.recipientId,
        preference.category,
        preference.channelId,
      ])
        identifier(value);
      if (!channels.has(preference.channelId) || typeof preference.enabled !== "boolean") {
        throw new NotificationError("invalid-input");
      }
      await store.setPreference(preference);
    },
    async recordTaskJob(id: string, taskJobId: string) {
      identifier(id);
      identifier(taskJobId);
      if (!(await store.markEnqueued(id, taskJobId)))
        throw new NotificationError("idempotency-conflict");
    },
    async recoverable(limit = 100) {
      boundedLimit(limit);
      return (await store.recoverable(clock(), limit)).map(notificationSummary);
    },
  };
}

export type NotificationService = ReturnType<typeof createNotificationService>;
