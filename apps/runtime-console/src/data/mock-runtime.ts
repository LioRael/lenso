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

export type ExecutionNode = {
  id: string;
  parentId?: string;
  name: string;
  service: string;
  kind:
    | "http"
    | "command"
    | "database"
    | "event"
    | "handler"
    | "runtime"
    | "function"
    | "external";
  status: RuntimeStatus;
  startMs: number;
  durationMs: number;
  attributes: Record<string, unknown>;
  events: Array<{
    name: string;
    timestampMs: number;
    attributes?: Record<string, unknown>;
  }>;
  logs: string[];
  context: Record<string, unknown>;
  payload?: Record<string, unknown>;
  retryable?: boolean;
  attempts?: number;
  maxAttempts?: number;
};

export type ExecutionEdge = {
  id: string;
  source: string;
  target: string;
  type: "causation" | "sequence" | "technical" | string;
  label?: string;
};

export type RuntimeStory = {
  id: string;
  name: string;
  service: string;
  source: string;
  status: RuntimeStatus;
  durationMs: number;
  timestamp: string;
  correlationId: string;
  nodes: ExecutionNode[];
  edges?: ExecutionEdge[];
  timelineItems?: TimelineItem[];
};

/** @deprecated Use ExecutionNode. Remove after downstream runtime story imports no longer need legacy trace aliases. */
export type TraceSpan = ExecutionNode;
/** @deprecated Use RuntimeStory. Remove after downstream runtime story imports no longer need legacy trace aliases. */
export type TraceRun = RuntimeStory;

export const correlationId = "corr_01HX9A7K2R_RUNTIME";

export const runtimeStories: RuntimeStory[] = [
  {
    id: "tr_01HX9A7_RUNTIME",
    name: "POST /v1/identity/users",
    service: "app-api",
    source: "http",
    status: "failed",
    durationMs: 6412,
    timestamp: "2026-05-31T09:18:11.980Z",
    correlationId,
    nodes: [
      {
        id: "sp_http_create_user",
        name: "POST /v1/identity/users",
        service: "app-api",
        kind: "http",
        status: "completed",
        startMs: 0,
        durationMs: 140,
        attributes: {
          "http.method": "POST",
          "http.route": "/v1/identity/users",
          "http.status_code": 201,
        },
        events: [{ name: "request.accepted", timestampMs: 4 }],
        logs: ["request context created", "identity route matched"],
        context: {
          actor: "user:user_123",
          request_id: "req_01HX9A7G",
          correlation_id: correlationId,
        },
        payload: {
          email: "alex@example.com",
          display_name: "Alex Chen",
        },
      },
      {
        id: "sp_identity_command",
        parentId: "sp_http_create_user",
        name: "identity.create_user",
        service: "identity",
        kind: "command",
        status: "completed",
        startMs: 24,
        durationMs: 91,
        attributes: {
          "module.name": "identity",
          "command.name": "identity.create_user",
        },
        events: [{ name: "validation.passed", timestampMs: 31 }],
        logs: ["validated email", "opened database transaction"],
        context: {
          actor: "user:user_123",
          tenant_id: "local",
        },
      },
      {
        id: "sp_outbox_insert",
        parentId: "sp_identity_command",
        name: "platform.outbox.insert",
        service: "postgres",
        kind: "database",
        status: "completed",
        startMs: 72,
        durationMs: 22,
        attributes: {
          "db.system": "postgresql",
          "db.schema": "platform",
          "db.operation": "insert",
        },
        events: [{ name: "row.inserted", timestampMs: 87 }],
        logs: ["inserted outbox event identity.user_registered.v1"],
        context: {
          transaction_id: "tx_01HX9A7",
        },
        payload: {
          event_name: "identity.user_registered.v1",
          aggregate_id: "usr_01HX9A7J",
        },
      },
      {
        id: "sp_user_registered",
        parentId: "sp_outbox_insert",
        name: "identity.user_registered.v1",
        service: "outbox-relay",
        kind: "event",
        status: "published",
        startMs: 440,
        durationMs: 260,
        attributes: {
          "event.name": "identity.user_registered.v1",
          "event.version": 1,
          "outbox.attempt": 1,
        },
        events: [
          { name: "event.claimed", timestampMs: 441 },
          { name: "event.dispatched", timestampMs: 687 },
        ],
        logs: ["claimed outbox row", "dispatching in-process handlers"],
        context: {
          locked_by: "worker-local-1",
          correlation_id: correlationId,
        },
      },
      {
        id: "sp_notifications_handler",
        parentId: "sp_user_registered",
        name: "notifications.handle_user_registered",
        service: "notifications",
        kind: "handler",
        status: "completed",
        startMs: 712,
        durationMs: 118,
        attributes: {
          "handler.event": "identity.user_registered.v1",
          "module.name": "notifications",
        },
        events: [{ name: "handler.completed", timestampMs: 826 }],
        logs: ["resolved welcome-email runtime function"],
        context: {
          causation_id: "evt_01HX9A7N_USER_REGISTERED",
        },
      },
      {
        id: "sp_enqueue_function",
        parentId: "sp_notifications_handler",
        name: "runtime.enqueue_function",
        service: "platform-runtime",
        kind: "runtime",
        status: "completed",
        startMs: 841,
        durationMs: 38,
        attributes: {
          "runtime.function": "notifications.send_welcome_email.v1",
          "runtime.max_attempts": 3,
        },
        events: [{ name: "function_run.created", timestampMs: 871 }],
        logs: ["inserted runtime.function_runs row"],
        context: {
          function_run_id: "fn_01HX9A7Q_WELCOME_DEAD",
        },
      },
      {
        id: "sp_send_welcome",
        parentId: "sp_enqueue_function",
        name: "notifications.send_welcome_email.v1",
        service: "runtime-worker",
        kind: "function",
        status: "dead",
        startMs: 1120,
        durationMs: 5290,
        attributes: {
          "runtime.attempt": 3,
          "runtime.function": "notifications.send_welcome_email.v1",
          "runtime.status": "dead",
        },
        events: [
          { name: "function.claimed", timestampMs: 1120 },
          { name: "function.retry_exhausted", timestampMs: 6410 },
        ],
        logs: [
          "attempt 3/3",
          "rendered welcome template",
          "smtp provider timed out after 5000ms",
        ],
        context: {
          locked_by: "worker-local-1",
          actor: "system",
        },
        payload: {
          user_id: "usr_01HX9A7J",
          email: "alex@example.com",
          template: "welcome",
        },
        retryable: true,
        attempts: 3,
        maxAttempts: 3,
      },
      {
        id: "sp_smtp_provider",
        parentId: "sp_send_welcome",
        name: "smtp.provider.call",
        service: "postmark",
        kind: "external",
        status: "failed",
        startMs: 1285,
        durationMs: 5000,
        attributes: {
          "net.peer.name": "api.postmarkapp.com",
          provider: "postmark",
          timeout_ms: 5000,
        },
        events: [{ name: "socket.timeout", timestampMs: 6285 }],
        logs: ["connect ETIMEDOUT"],
        context: {
          retry_after_ms: 30_000,
        },
        retryable: false,
      },
    ],
  },
  {
    id: "tr_01HX9C5_FILES",
    name: "files.object_uploaded.v1",
    service: "files",
    source: "outbox",
    status: "pending",
    durationMs: 820,
    timestamp: "2026-05-31T09:20:44.500Z",
    correlationId: "corr_01HX9C5_FILES",
    nodes: [
      {
        id: "sp_files_upload",
        name: "files.object_uploaded.v1",
        service: "outbox-relay",
        kind: "event",
        status: "pending",
        startMs: 0,
        durationMs: 820,
        attributes: {
          "event.name": "files.object_uploaded.v1",
          queue: "platform.outbox",
        },
        events: [{ name: "available", timestampMs: 0 }],
        logs: ["waiting for relay claim"],
        context: {
          correlation_id: "corr_01HX9C5_FILES",
        },
      },
    ],
  },
  {
    id: "tr_01HX9A9_CLEANUP",
    name: "identity.cleanup_expired_sessions.v1",
    service: "identity",
    source: "runtime-worker",
    status: "completed",
    durationMs: 128,
    timestamp: "2026-05-31T09:14:01.000Z",
    correlationId: "corr_01HX9A9_CLEANUP",
    nodes: [
      {
        id: "sp_cleanup_run",
        name: "identity.cleanup_expired_sessions.v1",
        service: "runtime-worker",
        kind: "function",
        status: "completed",
        startMs: 0,
        durationMs: 128,
        attributes: {
          "runtime.function": "identity.cleanup_expired_sessions.v1",
          deleted_sessions: 17,
        },
        events: [{ name: "function.completed", timestampMs: 128 }],
        logs: ["scanned sessions", "deleted 17 expired rows"],
        context: {
          locked_by: "worker-local-1",
        },
        payload: {
          older_than_minutes: 60,
        },
      },
    ],
  },
];

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
