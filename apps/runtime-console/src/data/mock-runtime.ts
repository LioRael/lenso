export type RuntimeStatus =
  | "pending"
  | "processing"
  | "running"
  | "published"
  | "completed"
  | "failed"
  | "dead";

export type Actor =
  | { kind: "anonymous" }
  | { kind: "user"; id: string; scopes: string[] }
  | { kind: "service"; id: string; scopes: string[] }
  | { kind: "system" };

export type RuntimeEvent = {
  id: string;
  eventName: string;
  status: RuntimeStatus;
  attempts: number;
  maxAttempts: number;
  aggregateId: string;
  aggregateType: string;
  correlationId: string;
  causationId: string;
  createdAt: string;
  lockedAt?: string;
  publishedAt?: string;
  lastError?: string;
  actor: Actor;
  payload: Record<string, unknown>;
};

export type FunctionRun = {
  id: string;
  functionName: string;
  status: RuntimeStatus;
  attempts: number;
  maxAttempts: number;
  correlationId: string;
  createdAt: string;
  startedAt?: string;
  completedAt?: string;
  lockedBy?: string;
  lastError?: string;
  actor: Actor;
  input: Record<string, unknown>;
  output?: Record<string, unknown>;
  logs: string[];
};

export type TimelineItem = {
  id: string;
  type:
    | "http_request"
    | "command"
    | "outbox_event"
    | "function_run"
    | "flow_step"
    | "agent_tool_call"
    | "external_provider_call";
  name: string;
  status: RuntimeStatus;
  attempts: number;
  maxAttempts: number;
  correlationId: string;
  createdAt: string;
  startedAt?: string;
  completedAt?: string;
  lastError?: string;
  detailId?: string;
};

export const correlationId = "corr_01HX9A7K2R_RUNTIME";

export const runtimeEvents: RuntimeEvent[] = [
  {
    id: "evt_01HX9A7N_USER_REGISTERED",
    eventName: "identity.user_registered.v1",
    status: "published",
    attempts: 1,
    maxAttempts: 3,
    aggregateId: "usr_01HX9A7J",
    aggregateType: "user",
    correlationId,
    causationId: "req_01HX9A7G",
    createdAt: "2026-05-31T09:18:12.120Z",
    lockedAt: "2026-05-31T09:18:12.420Z",
    publishedAt: "2026-05-31T09:18:12.680Z",
    actor: { kind: "user", id: "user_123", scopes: ["identity:create"] },
    payload: {
      user_id: "usr_01HX9A7J",
      email: "alex@example.com",
      display_name: "Alex Chen",
    },
  },
  {
    id: "evt_01HX9B2_DEAD_PROFILE",
    eventName: "identity.user_registered.v1",
    status: "dead",
    attempts: 3,
    maxAttempts: 3,
    aggregateId: "usr_01HX9B2",
    aggregateType: "user",
    correlationId: "corr_01HX9B2_PROFILE",
    causationId: "req_01HX9B2",
    createdAt: "2026-05-31T09:10:03.000Z",
    lockedAt: "2026-05-31T09:12:11.000Z",
    lastError: "handler failed: runtime enqueue unavailable",
    actor: { kind: "service", id: "worker", scopes: ["runtime:dispatch"] },
    payload: {
      user_id: "usr_01HX9B2",
      email: "nora@example.com",
      display_name: "Nora Vale",
    },
  },
  {
    id: "evt_01HX9C5_OBJECT",
    eventName: "files.object_uploaded.v1",
    status: "pending",
    attempts: 0,
    maxAttempts: 3,
    aggregateId: "obj_01HX9C5",
    aggregateType: "object",
    correlationId: "corr_01HX9C5_FILES",
    causationId: "req_01HX9C5",
    createdAt: "2026-05-31T09:20:44.500Z",
    actor: { kind: "user", id: "user_456", scopes: ["files:write"] },
    payload: {
      object_id: "obj_01HX9C5",
      bucket: "avatars",
      content_type: "image/png",
    },
  },
  {
    id: "evt_01HX9D9_MESSAGE",
    eventName: "notifications.message_sent.v1",
    status: "failed",
    attempts: 2,
    maxAttempts: 3,
    aggregateId: "msg_01HX9D9",
    aggregateType: "message",
    correlationId: "corr_01HX9D9_NOTIFY",
    causationId: "fn_01HX9D9",
    createdAt: "2026-05-31T09:22:30.200Z",
    lockedAt: "2026-05-31T09:23:10.000Z",
    lastError: "smtp provider returned timeout after 5000ms",
    actor: { kind: "system" },
    payload: {
      message_id: "msg_01HX9D9",
      channel: "email",
      provider: "postmark",
    },
  },
];

export const functionRuns: FunctionRun[] = [
  {
    id: "fn_01HX9A7Q_WELCOME",
    functionName: "notifications.send_welcome_email.v1",
    status: "failed",
    attempts: 2,
    maxAttempts: 3,
    correlationId,
    createdAt: "2026-05-31T09:18:13.000Z",
    startedAt: "2026-05-31T09:18:13.160Z",
    completedAt: "2026-05-31T09:18:18.180Z",
    lockedBy: "worker-local-1",
    lastError: "email provider timeout: connect ETIMEDOUT",
    actor: { kind: "system" },
    input: {
      user_id: "usr_01HX9A7J",
      email: "alex@example.com",
      template: "welcome",
    },
    logs: [
      "loaded welcome template",
      "resolved recipient alex@example.com",
      "provider request timed out after 5000ms",
    ],
  },
  {
    id: "fn_01HX9A7Q_WELCOME_DEAD",
    functionName: "notifications.send_welcome_email.v1",
    status: "dead",
    attempts: 3,
    maxAttempts: 3,
    correlationId,
    createdAt: "2026-05-31T09:23:13.000Z",
    startedAt: "2026-05-31T09:23:13.160Z",
    completedAt: "2026-05-31T09:23:18.220Z",
    lockedBy: "worker-local-1",
    lastError: "dead after retry: email provider timeout: connect ETIMEDOUT",
    actor: { kind: "system" },
    input: {
      user_id: "usr_01HX9A7J",
      email: "alex@example.com",
      template: "welcome",
    },
    logs: [
      "retry attempt 3/3",
      "provider request timed out after 5000ms",
      "marked dead after exhausting attempts",
    ],
  },
  {
    id: "fn_01HX9A9_CLEANUP",
    functionName: "identity.cleanup_expired_sessions.v1",
    status: "completed",
    attempts: 1,
    maxAttempts: 3,
    correlationId: "corr_01HX9A9_CLEANUP",
    createdAt: "2026-05-31T09:14:01.000Z",
    startedAt: "2026-05-31T09:14:01.040Z",
    completedAt: "2026-05-31T09:14:01.128Z",
    lockedBy: "worker-local-1",
    actor: { kind: "service", id: "scheduler", scopes: ["runtime:enqueue"] },
    input: { older_than_minutes: 60 },
    output: { deleted_sessions: 17 },
    logs: ["scanned sessions", "deleted 17 expired rows"],
  },
  {
    id: "fn_01HX9E2_RUNNING",
    functionName: "notifications.send_welcome_email.v1",
    status: "running",
    attempts: 1,
    maxAttempts: 3,
    correlationId: "corr_01HX9E2_RUNNING",
    createdAt: "2026-05-31T09:24:10.000Z",
    startedAt: "2026-05-31T09:24:11.000Z",
    lockedBy: "worker-local-2",
    actor: { kind: "system" },
    input: {
      user_id: "usr_01HX9E2",
      email: "maya@example.com",
      template: "welcome",
    },
    logs: ["claimed run", "rendering template"],
  },
  {
    id: "fn_01HX9F6_DEAD",
    functionName: "notifications.send_welcome_email.v1",
    status: "dead",
    attempts: 3,
    maxAttempts: 3,
    correlationId: "corr_01HX9F6_DEAD",
    createdAt: "2026-05-31T09:05:42.000Z",
    startedAt: "2026-05-31T09:08:42.000Z",
    completedAt: "2026-05-31T09:08:47.000Z",
    lockedBy: "worker-local-1",
    lastError: "template renderer rejected missing locale",
    actor: { kind: "system" },
    input: {
      user_id: "usr_01HX9F6",
      email: "sam@example.com",
      template: "welcome",
      locale: null,
    },
    logs: ["claimed run", "template renderer rejected missing locale"],
  },
];

export const timelineItems: TimelineItem[] = [
  {
    id: "req_01HX9A7G",
    type: "http_request",
    name: "POST /v1/identity/users",
    status: "completed",
    attempts: 1,
    maxAttempts: 1,
    correlationId,
    createdAt: "2026-05-31T09:18:11.980Z",
    startedAt: "2026-05-31T09:18:11.980Z",
    completedAt: "2026-05-31T09:18:12.120Z",
  },
  {
    id: "cmd_01HX9A7L",
    type: "command",
    name: "identity.create_user",
    status: "completed",
    attempts: 1,
    maxAttempts: 1,
    correlationId,
    createdAt: "2026-05-31T09:18:12.020Z",
    startedAt: "2026-05-31T09:18:12.030Z",
    completedAt: "2026-05-31T09:18:12.118Z",
  },
  {
    id: "evt_01HX9A7N_USER_REGISTERED",
    type: "outbox_event",
    name: "identity.user_registered.v1",
    status: "published",
    attempts: 1,
    maxAttempts: 3,
    correlationId,
    createdAt: "2026-05-31T09:18:12.120Z",
    startedAt: "2026-05-31T09:18:12.420Z",
    completedAt: "2026-05-31T09:18:12.680Z",
    detailId: "evt_01HX9A7N_USER_REGISTERED",
  },
  {
    id: "fn_01HX9A7Q_WELCOME_DEAD",
    type: "function_run",
    name: "notifications.send_welcome_email.v1",
    status: "dead",
    attempts: 3,
    maxAttempts: 3,
    correlationId,
    createdAt: "2026-05-31T09:23:13.000Z",
    startedAt: "2026-05-31T09:23:13.160Z",
    completedAt: "2026-05-31T09:23:18.220Z",
    lastError: "dead after retry: email provider timeout: connect ETIMEDOUT",
    detailId: "fn_01HX9A7Q_WELCOME_DEAD",
  },
];

export const futureTimelineSlots: Array<Pick<TimelineItem, "type" | "name">> = [
  { type: "flow_step", name: "flow step" },
  { type: "agent_tool_call", name: "agent tool call" },
  { type: "external_provider_call", name: "external provider call" },
];

export const queueHealth = [
  {
    name: "outbox",
    pending: 12,
    running: 1,
    failed: 2,
    dead: 1,
    oldest: "38s",
  },
  {
    name: "runtime.functions",
    pending: 7,
    running: 3,
    failed: 1,
    dead: 1,
    oldest: "12s",
  },
];

export type RuntimeRecord =
  | { kind: "event"; item: RuntimeEvent }
  | { kind: "function"; item: FunctionRun }
  | { kind: "timeline"; item: TimelineItem };

export type RetryTarget =
  | {
      kind: "event";
      id: string;
      name: string;
      status: RuntimeStatus;
      attempts: number;
      maxAttempts: number;
    }
  | {
      kind: "function";
      id: string;
      name: string;
      status: RuntimeStatus;
      attempts: number;
      maxAttempts: number;
    }
  | {
      kind: "timeline";
      id: string;
      name: string;
      status: RuntimeStatus;
      attempts: number;
      maxAttempts: number;
    };

export function isRetryable(status: RuntimeStatus) {
  return status === "failed" || status === "dead";
}

export function retryTargetFor(record: RuntimeRecord): RetryTarget | null {
  if (record.kind === "event") {
    if (!isRetryable(record.item.status)) {
      return null;
    }
    return {
      kind: "event",
      id: record.item.id,
      name: record.item.eventName,
      status: record.item.status,
      attempts: record.item.attempts,
      maxAttempts: record.item.maxAttempts,
    };
  }

  if (record.kind === "function") {
    if (!isRetryable(record.item.status)) {
      return null;
    }
    return {
      kind: "function",
      id: record.item.id,
      name: record.item.functionName,
      status: record.item.status,
      attempts: record.item.attempts,
      maxAttempts: record.item.maxAttempts,
    };
  }

  if (!isRetryable(record.item.status)) {
    return null;
  }
  return {
    kind: "timeline",
    id: record.item.id,
    name: record.item.name,
    status: record.item.status,
    attempts: record.item.attempts,
    maxAttempts: record.item.maxAttempts,
  };
}

export function findEvent(id: string) {
  return runtimeEvents.find((event) => event.id === id);
}

export function findFunctionRun(id: string) {
  return functionRuns.find((run) => run.id === id);
}
