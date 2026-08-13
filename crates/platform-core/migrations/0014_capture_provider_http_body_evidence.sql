alter table platform.provider_http_calls
    add column if not exists request_body jsonb,
    add column if not exists request_body_capture_status text not null default 'not_captured',
    add column if not exists request_body_capture_reason text default 'legacy_record',
    add column if not exists request_body_observed_bytes bigint,
    add column if not exists response_body jsonb,
    add column if not exists response_body_capture_status text not null default 'not_captured',
    add column if not exists response_body_capture_reason text default 'legacy_record',
    add column if not exists response_body_observed_bytes bigint;

update platform.provider_http_calls
set request_body_capture_reason = 'legacy_record'
where request_body_capture_status = 'not_captured'
    and request_body_capture_reason is null;

update platform.provider_http_calls
set response_body_capture_reason = 'legacy_record'
where response_body_capture_status = 'not_captured'
    and response_body_capture_reason is null;

alter table platform.provider_http_calls
    add constraint provider_http_calls_request_body_capture_status_check
        check (request_body_capture_status in ('captured', 'not_applicable', 'not_captured')),
    add constraint provider_http_calls_response_body_capture_status_check
        check (response_body_capture_status in ('captured', 'not_applicable', 'not_captured')),
    add constraint provider_http_calls_request_body_capture_shape_check
        check (
            (request_body_capture_status = 'captured'
                and request_body is not null
                and request_body_capture_reason is null
                and request_body_observed_bytes is not null)
            or
            (request_body_capture_status <> 'captured'
                and request_body is null
                and request_body_capture_reason is not null)
        ),
    add constraint provider_http_calls_response_body_capture_shape_check
        check (
            (response_body_capture_status = 'captured'
                and response_body is not null
                and response_body_capture_reason is null
                and response_body_observed_bytes is not null)
            or
            (response_body_capture_status <> 'captured'
                and response_body is null
                and response_body_capture_reason is not null)
        ),
    add constraint provider_http_calls_request_body_observed_bytes_check
        check (request_body_observed_bytes is null or request_body_observed_bytes >= 0),
    add constraint provider_http_calls_response_body_observed_bytes_check
        check (response_body_observed_bytes is null or response_body_observed_bytes >= 0);
