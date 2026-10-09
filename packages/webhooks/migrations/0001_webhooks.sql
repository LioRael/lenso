-- Host-applied migration. The repository never runs DDL.
-- Replace __LENSO_WEBHOOK_SCHEMA__ with the reviewed quoted schema identifier.
-- Create that schema explicitly before running this migration.
BEGIN;

CREATE TABLE __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_endpoint (
  id uuid PRIMARY KEY,
  tenant_id text NOT NULL,
  scope_id text NOT NULL,
  url text NOT NULL,
  secret_ref text NOT NULL,
  enabled boolean NOT NULL,
  revision integer NOT NULL CHECK (revision > 0),
  created_at bigint NOT NULL CHECK (created_at >= 0),
  UNIQUE (tenant_id, scope_id, id)
);

CREATE TABLE __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_subscription (
  id uuid PRIMARY KEY,
  tenant_id text NOT NULL,
  scope_id text NOT NULL,
  endpoint_id uuid NOT NULL,
  event_type text NOT NULL,
  enabled boolean NOT NULL,
  created_at bigint NOT NULL CHECK (created_at >= 0),
  UNIQUE (tenant_id, scope_id, id, endpoint_id),
  FOREIGN KEY (tenant_id, scope_id, endpoint_id)
    REFERENCES __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_endpoint (tenant_id, scope_id, id) ON DELETE RESTRICT
);

CREATE TABLE __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_event (
  id uuid PRIMARY KEY,
  tenant_id text NOT NULL,
  scope_id text NOT NULL,
  event_type text NOT NULL,
  envelope json NOT NULL CHECK (json_typeof(envelope) = 'object'),
  body text NOT NULL,
  created_at bigint NOT NULL CHECK (created_at >= 0),
  UNIQUE (tenant_id, scope_id, id)
);

CREATE TABLE __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_delivery (
  id uuid PRIMARY KEY,
  tenant_id text NOT NULL,
  scope_id text NOT NULL,
  event_id uuid NOT NULL,
  endpoint_id uuid NOT NULL,
  subscription_id uuid NOT NULL,
  endpoint_revision integer NOT NULL CHECK (endpoint_revision > 0),
  url text NOT NULL,
  secret_ref text NOT NULL,
  body text NOT NULL,
  state text NOT NULL CHECK (state IN ('pending', 'running', 'retry', 'succeeded', 'failed')),
  attempt_count integer NOT NULL CHECK (attempt_count >= 0),
  max_attempts integer NOT NULL CHECK (max_attempts BETWEEN 1 AND 100),
  due_at bigint NOT NULL CHECK (due_at >= 0),
  generation integer NOT NULL CHECK (generation >= 1),
  recovery_checked_at bigint NOT NULL DEFAULT 0,
  replay_of uuid,
  audit_intent_id text,
  lease_token text,
  lease_until bigint,
  last_code text CHECK (last_code IN (
    'success', 'timeout', 'connection-failed', 'rate-limited', 'server-error',
    'permanent-http', 'policy-rejected', 'response-too-large', 'request-too-large',
    'redirect-rejected', 'key-unavailable', 'lease-expired', 'endpoint-disabled', 'unsubscribed'
  )),
  created_at bigint NOT NULL CHECK (created_at >= 0),
  updated_at bigint NOT NULL CHECK (updated_at >= created_at),
  UNIQUE (tenant_id, scope_id, id),
  CHECK (attempt_count <= max_attempts),
  CHECK ((state = 'running' AND lease_token IS NOT NULL AND lease_until IS NOT NULL)
    OR (state <> 'running' AND lease_token IS NULL AND lease_until IS NULL)),
  CHECK (state <> 'running' OR attempt_count > 0),
  CHECK (state <> 'succeeded' OR (last_code IS NOT NULL AND last_code = 'success')),
  CHECK ((replay_of IS NULL AND audit_intent_id IS NULL)
    OR (replay_of IS NOT NULL AND audit_intent_id IS NOT NULL)),
  CHECK (replay_of IS NULL OR replay_of <> id),
  FOREIGN KEY (tenant_id, scope_id, event_id)
    REFERENCES __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_event (tenant_id, scope_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, scope_id, subscription_id, endpoint_id)
    REFERENCES __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_subscription (tenant_id, scope_id, id, endpoint_id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, scope_id, replay_of)
    REFERENCES __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_delivery (tenant_id, scope_id, id) ON DELETE RESTRICT
);

CREATE TABLE __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_attempt (
  id uuid PRIMARY KEY,
  tenant_id text NOT NULL,
  scope_id text NOT NULL,
  delivery_id uuid NOT NULL,
  number integer NOT NULL CHECK (number > 0),
  started_at bigint NOT NULL CHECK (started_at >= 0),
  finished_at bigint,
  code text CHECK (code IN (
    'success', 'timeout', 'connection-failed', 'rate-limited', 'server-error',
    'permanent-http', 'policy-rejected', 'response-too-large', 'request-too-large',
    'redirect-rejected', 'key-unavailable', 'lease-expired', 'endpoint-disabled', 'unsubscribed'
  )),
  status integer CHECK (status BETWEEN 100 AND 599),
  key_id text,
  UNIQUE (delivery_id, number),
  CHECK ((finished_at IS NULL AND code IS NULL AND status IS NULL AND key_id IS NULL)
    OR (finished_at IS NOT NULL AND code IS NOT NULL AND finished_at >= started_at)),
  FOREIGN KEY (tenant_id, scope_id, delivery_id)
    REFERENCES __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_delivery (tenant_id, scope_id, id) ON DELETE CASCADE
);

CREATE INDEX lenso_webhook_endpoint_page ON __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_endpoint (tenant_id, scope_id, created_at, id);
CREATE INDEX lenso_webhook_subscription_page ON __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_subscription (tenant_id, scope_id, created_at, id);
CREATE INDEX lenso_webhook_subscription_match ON __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_subscription (tenant_id, scope_id, event_type, id) WHERE enabled;
CREATE INDEX lenso_webhook_subscription_endpoint ON __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_subscription (tenant_id, scope_id, endpoint_id);
CREATE INDEX lenso_webhook_delivery_page ON __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_delivery (tenant_id, scope_id, created_at, id);
CREATE INDEX lenso_webhook_delivery_due ON __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_delivery (due_at, id) WHERE state IN ('pending', 'retry');
CREATE INDEX lenso_webhook_delivery_recovery ON __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_delivery (recovery_checked_at, due_at, id) WHERE state IN ('pending', 'retry', 'running');
CREATE INDEX lenso_webhook_delivery_expired ON __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_delivery (lease_until, id) WHERE state = 'running';
CREATE INDEX lenso_webhook_delivery_retention ON __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_delivery (updated_at, id) WHERE state IN ('succeeded', 'failed');
CREATE INDEX lenso_webhook_delivery_event ON __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_delivery (event_id);
CREATE INDEX lenso_webhook_delivery_subscription ON __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_delivery (subscription_id);
CREATE INDEX lenso_webhook_delivery_replay ON __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_delivery (replay_of);
CREATE INDEX lenso_webhook_attempt_page ON __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_attempt (tenant_id, scope_id, delivery_id, started_at, id);
CREATE UNIQUE INDEX lenso_webhook_attempt_open ON __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_attempt (delivery_id) WHERE finished_at IS NULL;
CREATE INDEX lenso_webhook_event_retention ON __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_event (created_at, id);

COMMIT;
