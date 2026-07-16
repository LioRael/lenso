alter table runtime.function_runs
    add column if not exists tenant_id text,
    add column if not exists tenancy_mode text not null default 'none';

alter table runtime.function_runs
    add constraint function_runs_tenancy_context_check check (
        (tenancy_mode = 'required' and tenant_id is not null)
        or tenancy_mode = 'optional'
        or (tenancy_mode = 'none' and tenant_id is null)
    );
