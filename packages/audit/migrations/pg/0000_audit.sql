CREATE TABLE lenso_audit_events (
  tenant_key TEXT NOT NULL,
  scope_id TEXT NOT NULL,
  id TEXT NOT NULL,
  recorded_at BIGINT NOT NULL,
  action TEXT NOT NULL,
  target_type TEXT NOT NULL,
  target_id TEXT NOT NULL,
  result TEXT NOT NULL,
  correlation_id TEXT,
  event_json TEXT NOT NULL,
  PRIMARY KEY (tenant_key, scope_id, id)
);
CREATE INDEX lenso_audit_events_scope_order
  ON lenso_audit_events (tenant_key, scope_id, recorded_at DESC, id DESC);
