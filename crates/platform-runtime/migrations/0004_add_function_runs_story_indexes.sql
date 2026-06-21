create index if not exists function_runs_story_correlation_idx
    on runtime.function_runs (correlation_id, created_at, id);

create index if not exists function_runs_story_updated_idx
    on runtime.function_runs ((coalesce(completed_at, started_at, locked_at, created_at)) desc, correlation_id, id);
