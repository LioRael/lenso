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
  eventVersion?: number;
  status: RuntimeStatus;
  attempts: number;
  maxAttempts: number;
  aggregateId: string;
  aggregateType: string;
  correlationId: string;
  causationId: string;
  createdAt: string;
  occurredAt?: string;
  lockedAt?: string;
  lockedBy?: string;
  publishedAt?: string;
  lastError?: string;
  sourceModule?: string;
  actor: Actor;
  headers?: Record<string, unknown>;
  payload: Record<string, unknown>;
  trace?: Record<string, unknown>;
};

export type FunctionRun = {
  id: string;
  functionName: string;
  runtimeDeclaration?: {
    moduleName: string;
    moduleSource: "linked" | "remote" | string;
    name: string;
    version: number;
    queue: string;
    inputSchema?: string;
    retryPolicy?: {
      maxAttempts: number;
      initialDelayMs: number;
    };
  };
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
    | "remote_proxy_call"
    | "failure"
    | "retry"
    | "flow_step"
    | "agent_tool_call"
    | "external_provider_call"
    | string;
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
  canonicalName?: string;
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

export type ExecutionPayload = {
  input?: unknown;
  output?: unknown;
  metadata?: unknown;
  redactedFields: string[];
};

export type ExecutionLogEntry = {
  id: string;
  nodeId: string;
  nodeType: string;
  correlationId: string;
  storyId: string;
  executionName: string;
  occurredAt: string;
  severity: "trace" | "debug" | "info" | "warn" | "error" | string;
  body: string;
  attributes: Record<string, unknown>;
  serviceName: string;
  traceId?: string;
  spanId?: string;
  redactedFields: string[];
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

export type TechnicalOperation = {
  id: string;
  storyId: string;
  correlationId: string;
  relatedNodeId?: string;
  category:
    | "http"
    | "db"
    | "redis"
    | "s3"
    | "ses"
    | "worker"
    | "runtime"
    | "external"
    | "unknown";
  name: string;
  status: string;
  startedAt: string;
  endedAt: string;
  durationMs: number;
  attributes: Record<string, unknown>;
  source: "otel" | "remote_proxy" | "remote_runtime";
};

export type RemoteProxyCall = {
  capability?: string | null;
  correlation_id: string;
  declared_path: string;
  duration_ms: number;
  error_code?: string | null;
  error_details: unknown;
  id: string;
  method: string;
  module_name: string;
  occurred_at: string;
  path_params: unknown;
  remote_path: string;
  remote_status?: number | null;
  request_id: string;
  retryable: boolean;
  span_id?: string | null;
  success: boolean;
  trace_id?: string | null;
};

export const correlationId = "corr_01HX9A7K2R_RUNTIME";

export const runtimeStories: RuntimeStory[] = [
  {
    id: "story_resource_published_fanout",
    name: "Resource Published Fan-out",
    service: "content-api",
    source: "runtime-story",
    status: "completed",
    durationMs: 9600,
    timestamp: "2026-06-01T10:00:00.000Z",
    correlationId: "corr_resource_published_fanout",
    nodes: [
      {
        id: "publish_resource_request",
        name: "PublishResource",
        service: "content-api",
        kind: "http",
        status: "completed",
        startMs: 0,
        durationMs: 320,
        attributes: {
          "http.method": "POST",
          "http.route": "/v1/resources/:id/publish",
          resource_id: "res_01J2PUBLISH",
        },
        events: [{ name: "request.accepted", timestampMs: 0 }],
        logs: ["validated publish request", "opened publish workflow"],
        context: {
          actor: "user:editor_42",
          correlation_id: "corr_resource_published_fanout",
          request_id: "req_resource_publish",
        },
        payload: {
          resource_id: "res_01J2PUBLISH",
          version: 7,
        },
      },
      {
        id: "create_resource_version",
        parentId: "publish_resource_request",
        name: "CreateResourceVersion",
        service: "content",
        kind: "function",
        status: "completed",
        startMs: 260,
        durationMs: 1040,
        attributes: {
          module: "content",
          version: 7,
        },
        events: [
          { name: "version.snapshot_created", timestampMs: 520 },
          { name: "version.persisted", timestampMs: 1260 },
        ],
        logs: ["snapshotted draft", "persisted resource version"],
        context: {
          correlation_id: "corr_resource_published_fanout",
          resource_id: "res_01J2PUBLISH",
        },
        payload: {
          resource_id: "res_01J2PUBLISH",
          version_id: "rv_01J2PUBLISH_V7",
        },
      },
      {
        id: "resource_version_published",
        parentId: "create_resource_version",
        name: "ResourceVersionPublished",
        service: "outbox-relay",
        kind: "event",
        status: "published",
        startMs: 1400,
        durationMs: 420,
        attributes: {
          event_name: "content.resource_version_published.v1",
          fanout_handlers: 3,
        },
        events: [
          { name: "event.claimed", timestampMs: 1400 },
          { name: "fanout.dispatched", timestampMs: 1820 },
        ],
        logs: ["published resource version event", "dispatched fan-out work"],
        context: {
          correlation_id: "corr_resource_published_fanout",
          causation_id: "create_resource_version",
        },
      },
      {
        id: "generate_search_index",
        parentId: "resource_version_published",
        name: "GenerateSearchIndex",
        service: "search",
        kind: "function",
        status: "completed",
        startMs: 2000,
        durationMs: 4500,
        attributes: {
          index: "resources_live",
          documents_written: 18,
        },
        events: [
          { name: "index.batch_started", timestampMs: 2050 },
          { name: "index.batch_committed", timestampMs: 6500 },
        ],
        logs: ["generated searchable document", "committed search index batch"],
        context: {
          correlation_id: "corr_resource_published_fanout",
          function_run_id: "fn_generate_search_index",
        },
        payload: {
          resource_id: "res_01J2PUBLISH",
          version_id: "rv_01J2PUBLISH_V7",
        },
      },
      {
        id: "sync_cdn_metadata",
        parentId: "resource_version_published",
        name: "SyncCDNMetadata",
        service: "cdn",
        kind: "function",
        status: "completed",
        startMs: 2200,
        durationMs: 1800,
        attributes: {
          provider: "fastly",
          surrogate_keys: 6,
        },
        events: [
          { name: "cdn.metadata_patch_started", timestampMs: 2220 },
          { name: "cdn.metadata_patch_completed", timestampMs: 4000 },
        ],
        logs: ["patched CDN metadata", "queued soft purge"],
        context: {
          correlation_id: "corr_resource_published_fanout",
          function_run_id: "fn_sync_cdn_metadata",
        },
        payload: {
          resource_id: "res_01J2PUBLISH",
          version_id: "rv_01J2PUBLISH_V7",
        },
      },
      {
        id: "send_subscriber_notifications",
        parentId: "resource_version_published",
        name: "SendSubscriberNotifications",
        service: "notifications",
        kind: "function",
        status: "completed",
        startMs: 2100,
        durationMs: 6900,
        attributes: {
          subscribers: 1248,
          batches: 13,
        },
        events: [
          { name: "notification.batches_created", timestampMs: 2140 },
          { name: "notification.batches_sent", timestampMs: 9000 },
        ],
        logs: ["created subscriber batches", "sent publish notifications"],
        context: {
          correlation_id: "corr_resource_published_fanout",
          function_run_id: "fn_send_subscriber_notifications",
        },
        payload: {
          resource_id: "res_01J2PUBLISH",
          version_id: "rv_01J2PUBLISH_V7",
        },
      },
      {
        id: "mark_publish_complete",
        parentId: "resource_version_published",
        name: "MarkPublishComplete",
        service: "content",
        kind: "function",
        status: "completed",
        startMs: 9150,
        durationMs: 450,
        attributes: {
          completed_after: "fanout",
          version_id: "rv_01J2PUBLISH_V7",
        },
        events: [{ name: "publish.completed", timestampMs: 9600 }],
        logs: ["all fan-out work completed", "marked publish complete"],
        context: {
          correlation_id: "corr_resource_published_fanout",
          resource_id: "res_01J2PUBLISH",
        },
        payload: {
          resource_id: "res_01J2PUBLISH",
          published: true,
        },
      },
    ],
    edges: [
      {
        id: "edge_publish_to_create_version",
        source: "publish_resource_request",
        target: "create_resource_version",
        type: "causation",
      },
      {
        id: "edge_create_version_to_event",
        source: "create_resource_version",
        target: "resource_version_published",
        type: "causation",
      },
      {
        id: "edge_event_to_search",
        source: "resource_version_published",
        target: "generate_search_index",
        type: "causation",
      },
      {
        id: "edge_event_to_cdn",
        source: "resource_version_published",
        target: "sync_cdn_metadata",
        type: "causation",
      },
      {
        id: "edge_event_to_notifications",
        source: "resource_version_published",
        target: "send_subscriber_notifications",
        type: "causation",
      },
      {
        id: "edge_event_to_mark_complete",
        source: "resource_version_published",
        target: "mark_publish_complete",
        type: "sequence",
        label: "join",
      },
    ],
    timelineItems: [
      {
        id: "timeline_publish_resource_request",
        type: "http_request",
        name: "PublishResource",
        status: "completed",
        attempts: 1,
        maxAttempts: 1,
        correlationId: "corr_resource_published_fanout",
        createdAt: "2026-06-01T10:00:00.000Z",
        startedAt: "2026-06-01T10:00:00.000Z",
        completedAt: "2026-06-01T10:00:00.320Z",
        detailId: "publish_resource_request",
      },
      {
        id: "timeline_create_resource_version",
        type: "function_run",
        name: "CreateResourceVersion",
        status: "completed",
        attempts: 1,
        maxAttempts: 1,
        correlationId: "corr_resource_published_fanout",
        createdAt: "2026-06-01T10:00:00.260Z",
        startedAt: "2026-06-01T10:00:00.260Z",
        completedAt: "2026-06-01T10:00:01.300Z",
        detailId: "create_resource_version",
      },
      {
        id: "timeline_resource_version_published",
        type: "outbox_event",
        name: "ResourceVersionPublished",
        status: "published",
        attempts: 1,
        maxAttempts: 3,
        correlationId: "corr_resource_published_fanout",
        createdAt: "2026-06-01T10:00:01.400Z",
        startedAt: "2026-06-01T10:00:01.400Z",
        completedAt: "2026-06-01T10:00:01.820Z",
        detailId: "resource_version_published",
      },
      {
        id: "timeline_generate_search_index",
        type: "function_run",
        name: "GenerateSearchIndex",
        status: "completed",
        attempts: 1,
        maxAttempts: 3,
        correlationId: "corr_resource_published_fanout",
        createdAt: "2026-06-01T10:00:02.000Z",
        startedAt: "2026-06-01T10:00:02.000Z",
        completedAt: "2026-06-01T10:00:06.500Z",
        detailId: "generate_search_index",
      },
      {
        id: "timeline_sync_cdn_metadata",
        type: "function_run",
        name: "SyncCDNMetadata",
        status: "completed",
        attempts: 1,
        maxAttempts: 3,
        correlationId: "corr_resource_published_fanout",
        createdAt: "2026-06-01T10:00:02.200Z",
        startedAt: "2026-06-01T10:00:02.200Z",
        completedAt: "2026-06-01T10:00:04.000Z",
        detailId: "sync_cdn_metadata",
      },
      {
        id: "timeline_send_subscriber_notifications",
        type: "function_run",
        name: "SendSubscriberNotifications",
        status: "completed",
        attempts: 1,
        maxAttempts: 3,
        correlationId: "corr_resource_published_fanout",
        createdAt: "2026-06-01T10:00:02.100Z",
        startedAt: "2026-06-01T10:00:02.100Z",
        completedAt: "2026-06-01T10:00:09.000Z",
        detailId: "send_subscriber_notifications",
      },
      {
        id: "timeline_mark_publish_complete",
        type: "function_run",
        name: "MarkPublishComplete",
        status: "completed",
        attempts: 1,
        maxAttempts: 1,
        correlationId: "corr_resource_published_fanout",
        createdAt: "2026-06-01T10:00:09.150Z",
        startedAt: "2026-06-01T10:00:09.150Z",
        completedAt: "2026-06-01T10:00:09.600Z",
        detailId: "mark_publish_complete",
      },
    ],
  },
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
    runtimeDeclaration: {
      moduleName: "notifications",
      moduleSource: "linked",
      name: "notifications.send_welcome_email.v1",
      version: 1,
      queue: "notifications",
      inputSchema: "notifications.send_welcome_email.v1",
    },
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
    runtimeDeclaration: {
      moduleName: "notifications",
      moduleSource: "linked",
      name: "notifications.send_welcome_email.v1",
      version: 1,
      queue: "notifications",
      inputSchema: "notifications.send_welcome_email.v1",
    },
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
    runtimeDeclaration: {
      moduleName: "identity",
      moduleSource: "linked",
      name: "identity.cleanup_expired_sessions.v1",
      version: 1,
      queue: "identity",
      inputSchema: "identity.cleanup_expired_sessions.v1",
    },
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
    runtimeDeclaration: {
      moduleName: "notifications",
      moduleSource: "linked",
      name: "notifications.send_welcome_email.v1",
      version: 1,
      queue: "notifications",
      inputSchema: "notifications.send_welcome_email.v1",
    },
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
    runtimeDeclaration: {
      moduleName: "notifications",
      moduleSource: "linked",
      name: "notifications.send_welcome_email.v1",
      version: 1,
      queue: "notifications",
      inputSchema: "notifications.send_welcome_email.v1",
    },
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
  {
    id: "fn_01HX9R_REMOTE_SYNC",
    functionName: "remote_crm.sync_contact.v1",
    runtimeDeclaration: {
      moduleName: "remote-crm",
      moduleSource: "remote",
      name: "remote_crm.sync_contact.v1",
      version: 1,
      queue: "remote-crm",
      inputSchema: "remote_crm.sync_contact.v1",
      retryPolicy: {
        maxAttempts: 3,
        initialDelayMs: 1000,
      },
    },
    status: "completed",
    attempts: 1,
    maxAttempts: 3,
    correlationId: "corr_01HX9R_REMOTE_SYNC",
    createdAt: "2026-05-31T09:26:10.000Z",
    startedAt: "2026-05-31T09:26:11.000Z",
    completedAt: "2026-05-31T09:26:11.420Z",
    lockedBy: "worker-remote-1",
    actor: { kind: "system" },
    input: { contact_id: "contact_1" },
    output: { synced: true },
    logs: ["claimed remote runtime function", "remote module returned success"],
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

export const remoteProxyCalls: RemoteProxyCall[] = [
  {
    capability: "crm.accounts.read",
    correlation_id: "corr_resource_published_fanout",
    declared_path: "/accounts/:account_id",
    duration_ms: 86,
    error_code: null,
    error_details: null,
    id: "rpc_01J2REMOTE_OK_ACCOUNTS",
    method: "GET",
    module_name: "remote-crm",
    occurred_at: "2026-06-03T10:12:04.120Z",
    path_params: {
      account_id: "acct_01J2A9",
    },
    remote_path: "/v1/accounts/acct_01J2A9",
    remote_status: 200,
    request_id: "req_remote_accounts_lookup",
    retryable: false,
    span_id: "span_remote_accounts_lookup",
    success: true,
    trace_id: "trace_remote_accounts_lookup",
  },
  {
    capability: "billing.invoices.create",
    correlation_id: "corr_resource_published_fanout",
    declared_path: "/invoices",
    duration_ms: 1420,
    error_code: "remote_http_429",
    error_details: {
      message: "remote module rate limited the request",
      retry_after_seconds: 45,
      upstream: "billing-sandbox",
    },
    id: "rpc_01J2REMOTE_FAIL_BILLING",
    method: "POST",
    module_name: "remote-billing",
    occurred_at: "2026-06-03T10:08:41.880Z",
    path_params: {},
    remote_path: "/api/invoices",
    remote_status: 429,
    request_id: "req_remote_invoice_create",
    retryable: true,
    span_id: "span_remote_invoice_create",
    success: false,
    trace_id: "trace_remote_invoice_create",
  },
  {
    capability: "fulfillment.shipments.read",
    correlation_id: "corr_remote_shipment_detail",
    declared_path: "/shipments/:shipment_id",
    duration_ms: 314,
    error_code: null,
    error_details: null,
    id: "rpc_01J2REMOTE_OK_SHIPMENT",
    method: "GET",
    module_name: "remote-fulfillment",
    occurred_at: "2026-06-03T09:58:15.004Z",
    path_params: {
      shipment_id: "shp_01J2Z8",
    },
    remote_path: "/v2/shipments/shp_01J2Z8",
    remote_status: 206,
    request_id: "req_remote_shipment_detail",
    retryable: false,
    span_id: null,
    success: true,
    trace_id: "trace_remote_shipment_detail",
  },
  {
    capability: "crm.contacts.write",
    correlation_id: "corr_remote_contact_sync",
    declared_path: "/contacts/:contact_id",
    duration_ms: 5012,
    error_code: "remote_timeout",
    error_details: {
      message: "remote proxy timed out before headers",
      timeout_ms: 5000,
    },
    id: "rpc_01J2REMOTE_TIMEOUT_CONTACT",
    method: "PATCH",
    module_name: "remote-crm",
    occurred_at: "2026-06-03T09:52:32.440Z",
    path_params: {
      contact_id: "con_01J2C4",
    },
    remote_path: "/v1/contacts/con_01J2C4",
    remote_status: null,
    request_id: "req_remote_contact_sync",
    retryable: true,
    span_id: "span_remote_contact_sync",
    success: false,
    trace_id: "trace_remote_contact_sync",
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
    };

export function isRetryable(status: RuntimeStatus) {
  return status === "failed" || status === "dead";
}

/**
 * Maps a story-graph execution node to a retry target, or null when the node
 * has no backend retry endpoint. Only outbox events (`event`) and function runs
 * (`function`) are retryable; HTTP requests and story events are not, so their
 * ids must never reach the outbox/functions retry routes.
 */
export function retryTargetForNode(node: ExecutionNode): RetryTarget | null {
  if (!(node.retryable && isRetryable(node.status))) {
    return null;
  }

  const kind =
    node.kind === "event"
      ? "event"
      : node.kind === "function"
        ? "function"
        : null;
  if (kind === null) {
    return null;
  }

  return {
    attempts: node.attempts ?? 1,
    id: node.id,
    kind,
    maxAttempts: node.maxAttempts ?? 3,
    name: node.name,
    status: node.status,
  };
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
  // Timeline items only map to a backend retry endpoint when they represent an
  // outbox event or a function run; map via `detailId` (the real backend id),
  // never the synthetic timeline id, and skip kinds with no retry route.
  const timelineKind =
    record.item.type === "outbox_event" || record.item.type === "event"
      ? "event"
      : record.item.type === "function_run" || record.item.type === "function"
        ? "function"
        : null;
  if (timelineKind === null) {
    return null;
  }
  return {
    kind: timelineKind,
    id: record.item.detailId ?? record.item.id,
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
