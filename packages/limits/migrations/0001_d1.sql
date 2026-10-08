CREATE TABLE lenso_d1_limit_counters (
  scope TEXT NOT NULL,
  kind TEXT NOT NULL,
  capacity INTEGER NOT NULL,
  period_ms INTEGER NOT NULL,
  window_start INTEGER NOT NULL,
  used INTEGER NOT NULL,
  last_now INTEGER NOT NULL,
  PRIMARY KEY (scope, kind),
  CONSTRAINT lenso_d1_counter_kind CHECK (kind IN ('rate', 'quota')),
  CONSTRAINT lenso_d1_counter_capacity CHECK (capacity BETWEEN 1 AND 2147483647),
  CONSTRAINT lenso_d1_counter_period CHECK (period_ms BETWEEN 1 AND 31622400000),
  CONSTRAINT lenso_d1_counter_used CHECK (used BETWEEN 0 AND capacity),
  CONSTRAINT lenso_d1_counter_clock CHECK (window_start >= 0 AND last_now >= 0)
);

CREATE TABLE lenso_d1_limit_concurrency (
  scope TEXT PRIMARY KEY NOT NULL,
  capacity INTEGER NOT NULL,
  last_now INTEGER NOT NULL,
  CONSTRAINT lenso_d1_lease_capacity CHECK (capacity BETWEEN 1 AND 2147483647),
  CONSTRAINT lenso_d1_lease_clock CHECK (last_now >= 0)
);

CREATE TABLE lenso_d1_limit_leases (
  scope TEXT NOT NULL REFERENCES lenso_d1_limit_concurrency(scope),
  token TEXT NOT NULL,
  quantity INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  PRIMARY KEY (scope, token),
  CONSTRAINT lenso_d1_lease_quantity CHECK (quantity BETWEEN 1 AND 2147483647),
  CONSTRAINT lenso_d1_lease_expiry_check CHECK (expires_at >= 0)
);
CREATE INDEX lenso_d1_lease_expiry ON lenso_d1_limit_leases(scope, expires_at);
