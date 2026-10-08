CREATE TABLE lenso_notifications (
  id TEXT PRIMARY KEY NOT NULL,
  tenant_id TEXT NOT NULL,
  scope TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  recipient_id TEXT NOT NULL,
  snapshot JSONB NOT NULL,
  state TEXT NOT NULL,
  revision INTEGER NOT NULL,
  attempt_count INTEGER NOT NULL,
  first_request_at BIGINT,
  lease_until BIGINT,
  task_job_id TEXT,
  provider_message_id TEXT,
  error TEXT,
  retryable BOOLEAN NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  mutation_token TEXT,
  CONSTRAINT lenso_notifications_key_unique UNIQUE (tenant_id, scope, idempotency_key),
  CONSTRAINT lenso_notifications_revision_check CHECK (revision >= 0 AND attempt_count >= 0)
);
CREATE INDEX lenso_notifications_tenant_recipient ON lenso_notifications (tenant_id, recipient_id, created_at);
CREATE INDEX lenso_notifications_recovery ON lenso_notifications (state, retryable, lease_until, updated_at);
CREATE TABLE lenso_notification_attempts (
  id TEXT PRIMARY KEY NOT NULL,
  notification_id TEXT NOT NULL REFERENCES lenso_notifications(id),
  number INTEGER NOT NULL,
  state TEXT NOT NULL,
  started_at BIGINT NOT NULL,
  finished_at BIGINT,
  provider_message_id TEXT,
  error TEXT,
  CONSTRAINT lenso_notification_attempt_number_unique UNIQUE (notification_id, number),
  CONSTRAINT lenso_notification_attempt_number_check CHECK (number > 0)
);
CREATE TABLE lenso_notification_preferences (
  tenant_id TEXT NOT NULL,
  recipient_id TEXT NOT NULL,
  category TEXT NOT NULL,
  channel_id TEXT NOT NULL,
  enabled BOOLEAN NOT NULL,
  CONSTRAINT lenso_notification_preference_unique UNIQUE (tenant_id, recipient_id, category, channel_id)
);
