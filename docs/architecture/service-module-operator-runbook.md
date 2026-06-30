# Service Operator Runbook

Use this runbook when a Lenso service is installed but its runtime state is
unclear. A service is still controlled by the host: the remote process serves
the protocol endpoint and provides modules, while the host owns auth, proxy
policy, runtime queues, Runtime Story, Remote Calls, and Technical Operations.

## Fast Path

```sh
lenso service list
lenso service status <provider> <service>
lenso service logs <provider> <service> --tail 100
lenso service export --module <provider> --format compose
lenso service doctor <module> --json
```

If the service is not running:

```sh
lenso service start <provider> <service>
lenso service logs <provider> <service> --tail 100
```

After installing or changing a service source, restart the API and worker so
the host reloads `REMOTE_MODULES` and `.lenso/module-services.json`.

## Status Table

| Status | Meaning | CLI check | Console evidence | Fix |
| --- | --- | --- | --- | --- |
| `ready` | The source is configured, the manifest is loaded, and service readiness checks pass. | `lenso service doctor <module> --json` | Modules shows service ready. | None. |
| `restart_pending` | Desired config changed after the current API/worker process started. | Doctor shows desired vs running source. | Modules shows restart pending. | Restart API and worker. |
| `configured_not_loaded` | The host has a configured source but did not load module metadata. | Doctor source exists; module metadata absent. | Modules shows configured but not loaded. | Restart; then inspect manifest errors. |
| `manifest_unreachable` | The host cannot fetch the module manifest. | Doctor manifest status is unreachable. | Modules shows manifest unreachable. | Start the service or fix the base URL. |
| `service_not_ready` | A declared service process is not passing its ready URL. | `lenso service status <provider> <service>` and `lenso service logs <provider> <service> --tail 100` | Modules shows service not ready. | Start the service or inspect local logs. |
| `missing_config` | A host-started service declares required env that is absent from `.env`. | `lenso service verify <manifest> --env-file .env --json` | Services shows missing config. | Set the env value and restart API/worker. |
| `stale_state` | Lock or pid files exist but the ready URL is failing. | Doctor lists lock or pid paths. | Modules shows stale state. | Stop the service, then remove stale files if needed. |
| `not_configured` | The host has no service source for the module. | Doctor has no source entry. | Module is absent or install state is empty. | Install the manifest or add `REMOTE_MODULES`. |

Services can also declare `compatibility`, `statusUrl` or `statusPath`,
`deployment`, and `install.services` metadata. The host records standard
status checks in `.lenso/service-health.json` and Console shows the recent
health history without taking over process orchestration.

Local logs are only captured for services started by `lenso service start` or
host-started managed services. They live under
`.lenso/service-logs/<provider>/<service>.log` in the host repo and are not a
deployment log store.

Provider upgrades should go through a release plan when the service is already
installed:

```sh
lenso service release plan <provider> <manifest-or-package> --output .lenso/<provider>.release-plan.json
lenso service policy check .lenso/<provider>.release-plan.json --fail-on breaking
lenso service release apply .lenso/<provider>.release-plan.json
```

The plan records current and candidate manifest references, module/capability
and operation diffs, env/config changes, risk, restart requirement, and next
action. Apply writes `.lenso/service-releases.json`; Console Services renders
the latest release and the recent provider history next to health, lifecycle,
operations, and config state.

## Boundaries

The service may own its process, language, deployment package, and module-local
storage. It should not write host runtime tables, consume host
outbox rows directly, receive browser bearer tokens, or bypass host capability
checks. All user-facing evidence should still flow through the host: Runtime
Console, Remote Calls, Runtime Story, and Technical Operations.

## Minimal Proof

The recommended proof path is the support-ticket service in
`lenso-examples`:

```sh
pnpm start:support-ticket
lenso service install http://127.0.0.1:4110/lenso/service/v1/manifest
lenso service doctor support-suite-provider --json
```

Use `pnpm host-api-smoke:support-ticket` for the one full host proof when
validating a release slice.
