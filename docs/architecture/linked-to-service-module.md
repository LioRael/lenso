# Linked To Service Module Extraction

Use this guide when a linked Rust module has a stable boundary and should move
to an independently running service module. This is a manual extraction path,
not an automatic migration tool.

## Keep Stable

Keep the public module contract unchanged:

- manifest `name`;
- capability names;
- HTTP route names and paths;
- runtime function names and payload schemas;
- event handler names and event schemas;
- admin action/query names and schemas;
- console surface route and package identity when one exists.

Runtime Console, Remote Calls, Runtime Story, Technical Operations, and host
admin APIs should keep showing the same business operation names after the
implementation moves out of process.

## Move The Implementation

1. Copy the linked module's `ModuleManifest` shape into the service module.
2. Rebuild the behavior behind the service module protocol endpoint.
3. Keep module-owned storage inside the service process or its own database.
4. Install the service manifest with `lenso module install <manifest-url>`.
5. Remove the linked registration from the host composition root.
6. Restart the API and worker so `REMOTE_MODULES` is loaded.
7. Run `lenso module doctor <module> --json` and verify `/console`.

For JavaScript or TypeScript service modules, prefer
`@lenso/remote-module-kit`. Other languages can serve the same manifest and
protocol endpoints directly.

## Do Not Move

The host keeps:

- caller authentication and capability checks;
- runtime queues and retry policy;
- outbox claiming;
- Runtime Story and Technical Operations writes;
- Remote Calls persistence;
- browser bearer token handling.

The service module receives host-mediated calls. It should not write host
runtime tables, consume host outbox rows directly, or accept browser bearer
tokens as its trust boundary.

## Support Ticket Shape

The recommended proof path is `support-ticket`:

```text
linked support-ticket module
  -> same manifest name and capabilities
  -> service process exposes /lenso/module/v1/manifest
  -> host switches from linked registration to REMOTE_MODULES
  -> Runtime Story and Remote Calls stay host-owned
```

Run the service-module proof in `lenso-examples` with:

```sh
pnpm host-api-smoke:support-ticket
```

That smoke verifies the manifest install, host proxy, admin/runtime paths, and
Runtime Story evidence without adding a service mesh, gateway, or orchestrator.

Use [`service-module-operator-runbook.md`](service-module-operator-runbook.md)
when the extracted service reports `restart_pending`,
`configured_not_loaded`, `manifest_unreachable`, `service_not_ready`, or
`stale_state`.
