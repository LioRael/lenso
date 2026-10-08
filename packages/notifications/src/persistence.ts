import type { DeliveryAttempt, NotificationRecord } from "./contracts";

export function boundedLimit(limit: number): number {
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 100) {
    throw new RangeError("Notification query limit must be an integer from 1 to 100");
  }
  return limit;
}

export function validateSave(
  record: NotificationRecord,
  expected: number,
  attempt?: DeliveryAttempt,
) {
  if (!Number.isSafeInteger(expected) || expected < 0 || record.revision !== expected + 1) {
    throw new Error("Notification revision must advance exactly once");
  }
  if (!Number.isSafeInteger(record.attemptCount) || record.attemptCount < 0) {
    throw new Error("Notification attempt count must be a nonnegative integer");
  }
  if (attempt && (attempt.notificationId !== record.id || attempt.number !== record.attemptCount)) {
    throw new Error("Attempt must belong to the notification and current attempt count");
  }
}

export function mutableValues(record: NotificationRecord) {
  return {
    state: record.state,
    revision: record.revision,
    attemptCount: record.attemptCount,
    firstRequestAt: record.firstRequestAt,
    leaseUntil: record.leaseUntil,
    providerMessageId: record.providerMessageId,
    error: record.error,
    retryable: record.retryable,
    updatedAt: record.updatedAt,
  };
}

export function insertValues(record: NotificationRecord) {
  return {
    ...mutableValues(record),
    id: record.id,
    tenantId: record.tenantId,
    scope: record.scope,
    idempotencyKey: record.idempotencyKey,
    recipientId: record.recipientId,
    taskJobId: record.taskJobId,
    createdAt: record.createdAt,
    snapshot: structuredClone(record),
  };
}

export function decodeRecord(row: ReturnType<typeof insertValues>): NotificationRecord {
  return {
    ...structuredClone(row.snapshot),
    state: row.state,
    revision: row.revision,
    attemptCount: row.attemptCount,
    firstRequestAt: row.firstRequestAt,
    leaseUntil: row.leaseUntil,
    taskJobId: row.taskJobId,
    providerMessageId: row.providerMessageId,
    error: row.error,
    retryable: row.retryable,
    updatedAt: row.updatedAt,
  };
}
