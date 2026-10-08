CREATE TABLE lenso_limit_counters (
  scope TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('rate', 'quota')),
  capacity INTEGER NOT NULL,
  period_ms INTEGER NOT NULL,
  window_start INTEGER NOT NULL,
  used INTEGER NOT NULL,
  last_now INTEGER NOT NULL,
  PRIMARY KEY (scope, kind),
  CONSTRAINT lenso_limit_counter_capacity CHECK (capacity BETWEEN 1 AND 2147483647),
  CONSTRAINT lenso_limit_counter_period CHECK (period_ms BETWEEN 1 AND 31622400000),
  CONSTRAINT lenso_limit_counter_used CHECK (used BETWEEN 0 AND capacity),
  CONSTRAINT lenso_limit_counter_clock CHECK (window_start >= 0 AND last_now >= 0)
);

CREATE TABLE lenso_limit_concurrency (
  scope TEXT PRIMARY KEY NOT NULL,
  capacity INTEGER NOT NULL,
  last_now INTEGER NOT NULL,
  leases TEXT NOT NULL,
  CONSTRAINT lenso_limit_lease_capacity CHECK (capacity BETWEEN 1 AND 2147483647),
  CONSTRAINT lenso_limit_lease_clock CHECK (last_now >= 0)
);
