create table if not exists platform.runtime_observation_changes (
    sequence bigint generated always as identity primary key,
    queue_kind text not null check (queue_kind in ('outbox', 'functions')),
    resource_id text not null,
    change_kind text not null check (change_kind in ('upserted', 'deleted')),
    recorded_at timestamptz not null default now()
);

create index if not exists runtime_observation_changes_recorded_idx
    on platform.runtime_observation_changes (recorded_at, sequence);

create or replace function platform.record_outbox_observation_change()
returns trigger
language plpgsql
as $$
declare
    changed_resource_id text;
    recorded_change_kind text;
begin
    if tg_op = 'DELETE' then
        changed_resource_id := old.id;
        recorded_change_kind := 'deleted';
    else
        changed_resource_id := new.id;
        recorded_change_kind := 'upserted';
    end if;
    insert into platform.runtime_observation_changes (queue_kind, resource_id, change_kind)
    values ('outbox', changed_resource_id, recorded_change_kind);
    return null;
end;
$$;

drop trigger if exists record_outbox_observation_change on platform.outbox;
create trigger record_outbox_observation_change
after insert or update or delete on platform.outbox
for each row execute function platform.record_outbox_observation_change();

create or replace function platform.record_function_observation_change()
returns trigger
language plpgsql
as $$
declare
    changed_resource_id text;
    recorded_change_kind text;
begin
    if tg_op = 'DELETE' then
        changed_resource_id := old.id;
        recorded_change_kind := 'deleted';
    else
        changed_resource_id := new.id;
        recorded_change_kind := 'upserted';
    end if;
    insert into platform.runtime_observation_changes (queue_kind, resource_id, change_kind)
    values ('functions', changed_resource_id, recorded_change_kind);
    return null;
end;
$$;

drop trigger if exists record_function_observation_change on runtime.function_runs;
create trigger record_function_observation_change
after insert or update or delete on runtime.function_runs
for each row execute function platform.record_function_observation_change();
