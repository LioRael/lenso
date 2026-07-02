# Lenso Capability Library V28 Design

## Summary

V28 turns Capability Packs from path-only local folders into a small local
library that App Composer, Runtime Console, and agents can read.

The goal is not a marketplace, remote registry, signing system, or package
solver. The goal is a boring file-backed workflow:

```sh
lenso capability library init
lenso capability library add ./capabilities/support-sla
lenso capability library list
lenso capability fit support-sla --repo-root .
lenso app compose --repo-root . --pack support-sla --write-plan
```

Capability Packs remain local authoring metadata. Modules and services remain
the runtime units. `lenso module install` and `lenso service install` keep their
current jobs.

## Product Shape

V28 introduces `lenso.capability-library.json` under `.lenso/`.

The library records pack references that a generated app can use:

- pack name
- local path
- label and summary
- supported blueprints
- declared modules
- declared services

Users can still pass a pack directory to `--pack`. If the value is not a local
path, Composer resolves it as a library name.

## CLI

Add these commands:

```sh
lenso capability library init [--repo-root .]
lenso capability library add <pack-path> [--repo-root .]
lenso capability library list [--repo-root .] [--json]
lenso capability library check [--repo-root .] [--json]
lenso capability fit <pack> [--repo-root .] [--json]
```

`fit` checks a pack against the current Launchpad app:

- pack can be read
- pack supports the app blueprint
- pack module names do not already exist
- pack service names do not already exist
- pack is already applied or pending

No command installs modules, starts services, or modifies service source files.

## App Composer

`lenso app compose --pack <value>` accepts either:

- a pack directory or manifest path
- a pack name from `.lenso/lenso.capability-library.json`

App Change Plan composition gains `packFit`, a list of fit records with:

- name
- status: `ready`, `blocked`, or `applied`
- path
- issues
- command

The existing safe-change and blocked-change behavior stays unchanged.

## Runtime Console

Console stays read-only. It displays `packFit` inside Launchpad's App Change
Plan panel so the operator sees whether a requested pack is ready, blocked, or
already applied.

Console does not write the library file and does not apply packs.

## Agent Handoff

`lenso agent task --for-capability <name>` can use pack data from either the App
Change Plan or the local library. The output includes:

- pack path
- modules
- services
- fit issues when known
- host-owned runtime boundary reminder

## Boundaries

Do not add:

- remote registry
- signing or trust policy
- dependency solver
- automatic module/service install
- browser-side mutations
- Kubernetes-specific behavior

V28 is a local library and fit report.

## Success Criteria

- A user can add a local pack to the library and compose it by name.
- A user can run one command to see why a pack fits or does not fit an app.
- Console can show pack fit status from App Change Plan state.
- Existing path-based `--pack` continues to work.
- Module install and service install semantics do not change.
