# Plugin-owned local Workers integrations

An import-free HTTP Component may need a Plugin-owned JavaScript Host entrypoint
for private infrastructure. The local Workers builder can assemble that code
without knowing the Plugin's identity or business protocol. This is a source
integration for the existing restricted Workers target, not general multi-Plugin
Workers support.

The Plugin owner supplies a profile and its Host files. The Host operator
separately approves the exact profile digest:

```sh
lenso app build --target workers --root ./app --out ./dist-workers \
  --workers-runtime /path/to/pinned/workers-runtime \
  --jco /path/to/jco \
  --workers-integration /path/to/integration/integration.json \
  --trust-workers-integration sha256:APPROVED_PROFILE_DIGEST
```

Both integration arguments are required together. Review the Host code and
obtain the digest from that reviewed source; a digest supplied by an untrusted
download is not independent authorization. These modules execute as trusted
Host JavaScript, outside the Component's isolation boundary. Bundle verification
does not authorize them, and this mechanism does not add a sandbox.

## Source contract

The profile uses `lenso.workers-integration.v1` with these required fields:

| Field | Meaning |
| --- | --- |
| `plugin_id` | Exact selected Plugin ID |
| `instance_key` | Local suffix, such as `default`; the builder compares the full resolved Plan key |
| `authoring_version` | Exact selected source authoring version, 1 or 2 |
| `world` | Owner-declared private WIT world label, not an independent compiler attestation |
| `manifest_digest` | Canonical verified Bundle manifest SHA-256 |
| `artifact_digest` | Selected Component SHA-256 |
| `runtime_version` | Pinned generic runtime version, `0.1.4` or `0.1.5` |
| `files` | Map of each staged filename to its SHA-256 |

Every digest has the form `sha256:` followed by 64 lowercase hexadecimal digits.
Unknown profile fields are rejected. The builder checks the operator's pin
before interpreting the profile, then matches its identities against the
ordinary resolver and Bundle selector; the profile cannot select another
implementation or edit the Plan.

Keep the profile in a dedicated directory containing only that file and the
declared assets. Include `worker.mjs` and `README.md`; other assets must be flat
ASCII `.mjs` filenames. Paths, subdirectories, symlinks, undeclared files and
framework output names are rejected. There may be 2–16 assets, each at most
1 MiB and at most 16 MiB in total; the profile is limited to 64 KiB. Changed
inputs fail the final source recheck before output publication.

The entrypoint may import the generated `plan.mjs`, `descriptor-digests.mjs`,
`artifact.mjs` (`world` and `digest`), and Jco's `guest.js` / `guest.core.wasm`.
Generic runtime modules are independently pinned by the builder. Runtime
`0.1.5` supplies `component-admission.mjs` as well as `component-requests.mjs`;
the runtime package no longer supplies product-specific bridge code.

The integration owns its private export checks, request mapping, resource
authorization, deadlines, error semantics and operational README. The builder
does not interpret its business protocol. Keep those checks in the Plugin's
integration tests and exercise the actual local runtime before claiming support.

## Build evidence and limits

The distribution contains the exact approved profile as
`workers-integration.json`. The `integration` object in `workers-build.json`
records that filename, its digest, selected identities and asset digests.
The profile and output bytes can be checked against that receipt. This is
source/digest verification, not publisher-signed Bundle ownership of sidecars.

The ordinary target restrictions remain: one selected HTTP Endpoint Instance,
no Capability bindings or required Capabilities, empty configuration, no
selected convention compiler, and an import-free single-core Component.
Jco must still match the pinned version and output closure. Omitting the
integration selects the ordinary HTTP entrypoint for every Plugin identity;
there is no business-specific fallback.

A build receipt proves neither deployment nor infrastructure availability.
Refer to the exact Environment-plus-Infrastructure test result, not merely
the target name.
