import { AuthError, type Access, type Actor, type Policy, type PolicyContext } from "@lenso/auth";
import { z } from "zod";
import { NotificationError } from "./errors";
import type { NotificationRecord } from "./contracts";
import type { NotificationService, NotificationSummary } from "./service";

const identifier = z.string().min(1).max(256).regex(/^\S+$/);
export const notificationQueryInput = z.strictObject({ id: identifier });
export const notificationListInput = z.strictObject({
  tenantId: identifier,
  limit: z.number().int().min(1).max(100),
});
export const notificationPreferenceInput = z.strictObject({
  tenantId: identifier,
  category: identifier,
  channelId: identifier,
});
export const notificationSetPreferenceInput = notificationPreferenceInput.extend({
  enabled: z.boolean(),
});

export interface NotificationResource {
  readonly tenantId: string | null;
  readonly ownerId: string | null;
  readonly scope: string | null;
  readonly notificationId: string | null;
  readonly action:
    | "query"
    | "list"
    | "preference-read"
    | "preference-write"
    | "admin-query"
    | "attempts"
    | "retry";
}

export interface AuthorizedNotificationOptions<
  R extends string,
  E,
  S extends string,
  A extends string,
  M = undefined,
> {
  readonly service: NotificationService;
  readonly access: Access<R, E, S, A, NotificationResource, M>;
  /** Trusted realm-aware mapping, evaluated only for an Auth-reverified principal. */
  readonly tenantFor: (actor: Actor<R, S, A>) => string | null | Promise<string | null>;
  readonly managePolicy?: Policy<PolicyContext<Actor<R, S, A>, NotificationResource, M>>;
  /** Durable Tasks dispatcher; this wrapper never invokes delivery. */
  readonly requeue?: (id: string) => Promise<boolean>;
}

function status(record: NotificationSummary) {
  return {
    id: record.id,
    state: record.state,
    attemptCount: record.attemptCount,
    retryable: record.retryable,
    createdAt: record.createdAt,
    updatedAt: record.updatedAt,
  };
}

export function createAuthorizedNotificationService<
  R extends string,
  E,
  S extends string,
  A extends string,
  M = undefined,
>(options: AuthorizedNotificationOptions<R, E, S, A, M>) {
  const { service, access, tenantFor, managePolicy, requeue } = options;
  async function enforce(actor: Actor<R, S, A>, resource: NotificationResource, admin = false) {
    try {
      return await access.enforce(actor, resource, async (context) => {
        const tenant = await tenantFor(context.principal);
        if (tenant === null || tenant !== context.resource.tenantId) return false;
        if (admin) return managePolicy ? (await managePolicy(context)) === true : false;
        return context.resource.ownerId === context.principal.subjectId;
      });
    } catch (error) {
      if (error instanceof AuthError) throw new NotificationError("access-denied");
      throw error;
    }
  }
  async function recordFor(
    id: string,
    actor: Actor<R, S, A>,
    action: NotificationResource["action"],
    admin = false,
  ): Promise<NotificationRecord> {
    const record = await service.getRecord(id);
    await enforce(
      actor,
      {
        tenantId: record?.tenantId ?? null,
        ownerId: record?.recipientId ?? null,
        scope: record?.scope ?? null,
        notificationId: record?.id ?? null,
        action,
      },
      admin,
    );
    if (!record) throw new NotificationError("access-denied");
    return record;
  }
  return {
    async query(raw: z.input<typeof notificationQueryInput>, actor: Actor<R, S, A>) {
      const input = notificationQueryInput.parse(raw);
      return status(await recordFor(input.id, actor, "query"));
    },
    async list(raw: z.input<typeof notificationListInput>, actor: Actor<R, S, A>) {
      const input = notificationListInput.parse(raw);
      const principal = await enforce(actor, {
        tenantId: input.tenantId,
        ownerId: actor?.subjectId ?? null,
        scope: null,
        notificationId: null,
        action: "list",
      });
      return (await service.list({ ...input, recipientId: principal.subjectId })).map(status);
    },
    async getPreference(raw: z.input<typeof notificationPreferenceInput>, actor: Actor<R, S, A>) {
      const input = notificationPreferenceInput.parse(raw);
      const principal = await enforce(actor, {
        tenantId: input.tenantId,
        ownerId: actor?.subjectId ?? null,
        scope: null,
        notificationId: null,
        action: "preference-read",
      });
      const preference = await service.getPreference({
        ...input,
        recipientId: principal.subjectId,
      });
      return {
        category: preference.category,
        channelId: preference.channelId,
        enabled: preference.enabled,
      };
    },
    async setPreference(
      raw: z.input<typeof notificationSetPreferenceInput>,
      actor: Actor<R, S, A>,
    ) {
      const input = notificationSetPreferenceInput.parse(raw);
      const principal = await enforce(actor, {
        tenantId: input.tenantId,
        ownerId: actor?.subjectId ?? null,
        scope: null,
        notificationId: null,
        action: "preference-write",
      });
      await service.setPreference({ ...input, recipientId: principal.subjectId });
      return { updated: true };
    },
    async adminQuery(raw: z.input<typeof notificationQueryInput>, actor: Actor<R, S, A>) {
      const input = notificationQueryInput.parse(raw);
      return status(await recordFor(input.id, actor, "admin-query", true));
    },
    async attempts(raw: z.input<typeof notificationQueryInput>, actor: Actor<R, S, A>) {
      const input = notificationQueryInput.parse(raw);
      await recordFor(input.id, actor, "attempts", true);
      return (await service.attempts(input.id)).map((attempt) => ({
        number: attempt.number,
        state: attempt.state,
        startedAt: attempt.startedAt,
        finishedAt: attempt.finishedAt,
      }));
    },
    async retry(raw: z.input<typeof notificationQueryInput>, actor: Actor<R, S, A>) {
      const input = notificationQueryInput.parse(raw);
      const record = await recordFor(input.id, actor, "retry", true);
      if (!requeue || !(await service.retryEligible(record.id))) {
        throw new NotificationError("delivery-retry");
      }
      return { queued: await requeue(record.id) };
    },
  };
}
