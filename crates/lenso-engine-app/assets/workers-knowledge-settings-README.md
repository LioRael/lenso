# Local knowledge-settings Workers slice

This generated App contains the verified, import-free
`lenso.reference.knowledge-settings` Component and the pinned local-workerd
adapter. It is not the full linked knowledge-base Plugin and does not provide
production Workers Auth, Jobs, Secrets, PostgreSQL, or network capabilities.

Start the task-owned PostgreSQL database, then run the settings bridge and
workerd in the **same Linux network namespace** so workerd's Fetch to
`127.0.0.1` reaches that bridge. Do not expose the bridge on `0.0.0.0` or add
a host-network proxy. The bridge must be configured with a short-lived, 0600
Host-owned policy that maps the SHA-256 digests of two high-entropy opaque Auth
Plugin tokens to their Native user IDs. Neither raw tokens nor the policy file
belong in this distribution. Pass the bridge's exact local origin explicitly
as `KNOWLEDGE_SETTINGS_BRIDGE_ORIGIN` in the local workerd environment. The
adapter has no implicit network fallback and admits only this KB `/settings`
read/CAS protocol; it does not expose a generic database or Host import.

The `workers-build.json` receipt records the selected Bundle, Component,
Descriptor, Plan, Jco output, and exact JS runtime module digests. It proves
build inputs, not deployment or production target qualification. Run the
Native → local workerd → Native PostgreSQL acceptance before claiming
cross-target persistence.
