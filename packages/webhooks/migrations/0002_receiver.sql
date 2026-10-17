-- Optional inbound inbox. Host-applied; no automatic schema creation or retention deletion.
-- Replace __LENSO_WEBHOOK_SCHEMA__ with the host-reviewed quoted schema identifier.
BEGIN;
CREATE TABLE __LENSO_WEBHOOK_SCHEMA__.lenso_webhook_receipt (
  receiver_id text NOT NULL CHECK (length(receiver_id) BETWEEN 1 AND 256),
  event_id text NOT NULL CHECK (event_id ~ '^[A-Za-z0-9_-]{1,128}$'),
  body_sha256 text NOT NULL CHECK (body_sha256 ~ '^[a-f0-9]{64}$'),
  received_at bigint NOT NULL CHECK (received_at >= 0),
  PRIMARY KEY (receiver_id, event_id)
);
COMMIT;
