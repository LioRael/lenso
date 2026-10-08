CREATE TABLE lenso_payments (
  payment_id TEXT PRIMARY KEY,
  order_key TEXT NOT NULL UNIQUE,
  account_id TEXT NOT NULL,
  live INTEGER NOT NULL CHECK (live IN (0, 1)),
  revision INTEGER NOT NULL CHECK (revision >= 0),
  reconcile_at INTEGER NOT NULL,
  data TEXT NOT NULL CHECK (json_valid(data))
);
CREATE INDEX lenso_payments_due_idx ON lenso_payments (account_id, live, reconcile_at, payment_id);
CREATE TABLE lenso_payment_events (
  event_key TEXT PRIMARY KEY,
  event_id TEXT NOT NULL,
  payment_id TEXT NOT NULL,
  object_id TEXT NOT NULL,
  refund_id TEXT,
  account_id TEXT NOT NULL,
  live INTEGER NOT NULL CHECK (live IN (0, 1)),
  created_at INTEGER NOT NULL,
  reconcile_at INTEGER NOT NULL,
  done INTEGER NOT NULL CHECK (done IN (0, 1))
);
CREATE INDEX lenso_payment_events_due_idx ON lenso_payment_events (account_id, live, done, reconcile_at, event_key);
