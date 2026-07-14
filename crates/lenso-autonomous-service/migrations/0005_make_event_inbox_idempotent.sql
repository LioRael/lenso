with ranked_inbox as (
    select ctid,
           row_number() over (
               partition by consumer_id, event_id
               order by case status
                            when 'completed' then 0
                            when 'rejected' then 1
                            when 'retryable' then 2
                            else 3
                        end,
                        completed_at desc nulls last,
                        received_at,
                        delivery_id
           ) as duplicate_rank
    from platform.service_event_inbox
)
delete from platform.service_event_inbox inbox
using ranked_inbox ranked
where inbox.ctid = ranked.ctid
  and ranked.duplicate_rank > 1;

alter table platform.service_event_inbox
    add constraint service_event_inbox_consumer_event_key
    unique (consumer_id, event_id);
