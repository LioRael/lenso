# Lenso System Runbook Plane V21 Plan

## Goal

Make system releases reviewable as generated operator runbooks without making
module authors maintain more JSON.

## Scope

- Add `lenso.system-runbook.v1` artifacts generated from system release plans.
- Add `lenso system runbook generate/check/record/history/doctor`.
- Store recorded runbooks in `.lenso/system-runbooks.json`.
- Expose `GET /admin/data/service-system/runbooks`.
- Show runbook state on Runtime Console Services.
- Add support-platform runbook fixtures and docs.

## Non-Goals

- No automatic shell execution.
- No workflow DSL.
- No Kubernetes or operator writes.
- No module authoring JSON changes.

## Verification

- CLI runbook tests.
- Admin data runbook endpoint test.
- Runtime Console model/fetch tests.
- Example runbook generate/check/history commands.
