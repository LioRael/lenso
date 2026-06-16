create index if not exists outbox_status_created_at_idx
    on platform.outbox (status, created_at);
