# Auth failures

Read the [public Auth guide](https://github.com/LioRael/lenso/blob/main/packages/auth/README.md) for source outcomes, session requirements and safe error semantics. Compare existing boundaries with [Notes operations](https://github.com/LioRael/lenso/blob/main/examples/notes/src/operations.ts) or the [authorized Tasks service](https://github.com/LioRael/lenso/blob/main/examples/tasks/src/authorized-service.ts), not a new authentication wrapper.

Trace the failing request through its existing credential extractor, verifying source, realm, operation audience, exact Auth instance and service policy:

- A source must verify evidence through the trusted identity system. Decoded claims, input `userId`, role/tenant JSON or credential presence are not verification. Local operation audience is distinct from a token's external `aud`.
- Actors require exact instance provenance and audience. JSON, object spreads and actors from another instance/process cannot authorize. Reauthenticate through the existing entry; never construct an actor or weaken `enforce`.
- Load actual object ownership/current membership, not a caller-selected tenant. Login success is not object access. Enforcement revalidates source credentials and configured current membership; stale or revoked access can fail after initial authentication.
- Distinguish genuinely absent evidence from rejected/unresolved evidence. Optional authentication permits null only for absence. Check supported source capabilities and freshness/assurance requirements for `REAUTHENTICATION_REQUIRED`; do not relax them to make the call pass.
- Known Auth codes are safe at an explicitly mapped app boundary. Unknown provider/policy errors may surface as `SERVICE_UNAVAILABLE` or opaque invocation failures. Use authorized safe logs to distinguish infrastructure from rejection, without disclosing internals to clients.

CLI/stdio uses launch identity, not a remote identity supplied per request. Credentials stay in the existing protected environment/entry, never arguments, logs or copied evidence. If identity or ownership cannot be verified, stop and identify the missing authorized evidence. Reading session secrets or mutating session tables is not a troubleshooting fallback.
