create index if not exists outbox_story_correlation_idx
    on platform.outbox (correlation_id, created_at, id);

create index if not exists outbox_story_updated_idx
    on platform.outbox ((coalesce(published_at, locked_at, created_at)) desc, correlation_id, id);
