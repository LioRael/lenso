CREATE TABLE lenso_payments (
  payment_id text PRIMARY KEY,
  order_key text NOT NULL UNIQUE,
  account_id text NOT NULL,
  live integer NOT NULL CHECK (live IN (0, 1)),
  revision integer NOT NULL CHECK (revision >= 0),
  reconcile_at bigint NOT NULL,
  data jsonb NOT NULL
);
CREATE INDEX lenso_payments_due_idx ON lenso_payments (account_id, live, reconcile_at, payment_id);
CREATE TABLE lenso_payment_events (
  event_key text PRIMARY KEY,
  event_id text NOT NULL,
  payment_id text NOT NULL,
  object_id text NOT NULL,
  refund_id text,
  account_id text NOT NULL,
  live integer NOT NULL CHECK (live IN (0, 1)),
  created_at bigint NOT NULL,
  reconcile_at bigint NOT NULL,
  done integer NOT NULL CHECK (done IN (0, 1))
);
CREATE INDEX lenso_payment_events_due_idx ON lenso_payment_events (account_id, live, done, reconcile_at, event_key);
