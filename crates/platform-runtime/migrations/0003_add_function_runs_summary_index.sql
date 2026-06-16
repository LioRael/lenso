create index if not exists function_runs_status_created_at_idx
    on runtime.function_runs (status, created_at);
