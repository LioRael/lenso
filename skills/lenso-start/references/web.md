# Web task map

Use this map for an observable HTTP result. Web work is a product path across
Core owners, not a separate framework ownership model.

## Start from the request path

Inspect the current Web Host, `lenso.http.endpoint@1` source and generated
projection, linked Ingress Plugin, Auth role, App Host Catalog, and real socket
tests. Use the current public Web guide at <https://lenso.dev/docs/web> as the
journey map; current owner source and selected versions remain authoritative.

## Choose the first owner

| Needed change | Primary Skill |
| --- | --- |
| Business rule or HTTP Endpoint behavior | `lenso-plugin-authoring` |
| New or changed typed role behind the endpoint | `lenso-capability-authoring` |
| Add/configure Endpoint, Auth, client, or Ingress Instances | `lenso-app-configuration` |
| Missing generic listener, process, wire, lane, or Host integration mechanism | `lenso-runtime-extension` |
| Ownership of domain, route, authorization, or failure policy is unclear | `lenso-business-planning` |

Keep route shape and HTTP mapping in the Endpoint Plugin. Keep final business
authorization with the target behavior Plugin. Keep listener, parsing limits,
readiness, and transport failures with the selected Ingress behavior or Host
mechanism already established by current source. Keep upstream allowlists and
credentials in typed configuration and external secret authority.

## Complete one vertical request

1. Prove the Endpoint Plugin directly through its generated provider/client.
2. Add only the intended `plugins/` differences and inspect the derived App.
3. Exercise the real socket through Ingress, including success, declared
   Domain Errors, malformed input, method/path mismatch, and Runtime Failure.
4. Prove duplicate routes or incomplete bindings fail before readiness.
5. Disable or remove the optional Endpoint and confirm the remaining App still
   resolves.
6. For deployment work, verify the actual Host boundary, TLS/proxy ownership,
   readiness, graceful shutdown, configuration, and secret injection. Do not
   invent a framework-owned deployment artifact when the repository does not
   ship one.

The Web path is complete when the real HTTP behavior is observable and every
artifact has one Core owner; a route diagram or handler-only test is not enough.
