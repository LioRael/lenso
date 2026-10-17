CREATE TABLE IF NOT EXISTS lenso_event_subscriptions (
  id TEXT PRIMARY KEY,
  topic TEXT NOT NULL,
  active BOOLEAN NOT NULL
);

CREATE TABLE IF NOT EXISTS lenso_events (
  id TEXT PRIMARY KEY,
  topic TEXT NOT NULL,
  payload TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS lenso_event_deliveries (
  event_id TEXT NOT NULL REFERENCES lenso_events(id),
  subscription_id TEXT NOT NULL REFERENCES lenso_event_subscriptions(id),
  state TEXT NOT NULL CHECK (state IN ('pending', 'processing', 'delivered', 'cancelled')),
  attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  available_at BIGINT NOT NULL,
  lease_until BIGINT,
  token TEXT,
  PRIMARY KEY (event_id, subscription_id),
  CHECK ((state = 'processing') = (lease_until IS NOT NULL AND token IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS lenso_event_ready
  ON lenso_event_deliveries(subscription_id, state, available_at, lease_until);
CREATE INDEX IF NOT EXISTS lenso_event_topics
  ON lenso_event_subscriptions(topic, id) WHERE active;
