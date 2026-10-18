# Explicit first-run setup (source candidate)

This runnable **local Bun example** uses the existing config source and CLI
contracts, with an opt-in first-run definition. It is not the published Bun Web
consumer template. The new Core entry and native `setup` command require this
unreleased source candidate; registry Core 0.3.1 and released native v0.2.0 do not
provide them. No new version is published by this change.

After preparing/building this reviewed source checkout, put the matching local
native CLI candidate on PATH. From the example directory:

```sh
lenso setup status --json
# uninitialized; no directory, database, user or secrets are created by status
lenso call greeting greet --stdin --json
# With {} on stdin: initialization-required, before plugin config/setup
```

Supply a single JSON object through `lenso setup apply --stdin --json` or a
private input file via `--input-file`. Do not put passwords in argv, shell
history or an environment-dump command. Input shape:

```json
{
  "requestId": "an-application-owned-retry-id",
  "input": {
    "username": "your-chosen-local-owner",
    "password": "your-own-password-of-at-least-12-characters",
    "providerKey": "your-own-provider-key"
  }
}
```

An interactive Console form can collect these fields from the user and retain a
non-secret request ID. Do not have an agent invent real passwords/provider keys.
The native sample uses an opaque local launch capability with a ten-minute
monotonic expiry; it checks expiry again under the SQL write lock. That binding
is not an HTTP endpoint. A remote setup host needs its own verified, target-bound,
short-lived initialization authorization, acquired through its existing trusted
operator/deployment owner, before it exposes this service.

One apply creates the local schema, owner, Argon2id password hash, private provider
configuration, random internal signing key and pending revision in one SQLite
transaction, then preflights/starts/stops the declared local app and marks the
same revision ready. No provider request, billing action or production authority
is created. Ready means this configuration is qualified for startup; it does not
mean a server process remains running after the finite CLI exits.

The first role is explicitly the **sample application's owner**. This is not a
Relay Finance/operator role or global Lenso account. The private `.state`
directory/file are created with 0700/0600 permissions; existing broad permissions
or symlinks are rejected rather than changed. The application state DB contains
provider/internal secrets in plaintext under those filesystem protections; it is
not a general vault. A real host should use its existing secret/config provider.
`LENSO_FIRST_RUN_HOME` explicitly selects a separate private local state directory.

After apply, `lenso call greeting greet --stdin --json` with `{}` returns a greeting
for the committed owner. Status and restart reuse persisted configuration. The
same request ID and identical input return the same revision without replacing
secrets; different input conflicts, and a different first-run request is closed.
A preexisting owner never becomes claimable merely because the initialization
marker is missing.

If activation fails after the transaction, `setup status` reports the durable
pending revision. Fix the named boundary, then use `lenso setup activate --json`;
no password replay, second user or new internal key is needed. An unknown write
outcome is an error requiring status inspection, not automatic retry.

Workers use the same portable state DTO but **not this native persistence or
launcher binding**. Missing platform bindings return `blocked` with
`nextAction: external_configuration`; the trusted external deployment/secret owner
must write the selected version and prove the actual active environment/revision.
No code here installs platform API credentials in a Worker or mutates its secrets.
Database migration and first-owner creation remain explicit host-owned
transactions. Uploading a secret version alone is not activation proof.

The framework's existing `lenso-develop` / `lenso-diagnose` instructions still
apply. No extra prompt framework or per-initialization script is required.
