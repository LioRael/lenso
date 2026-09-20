# Execution-target preflight

`lenso app explain` is a read-only, machine-readable preflight for the exact
App derived from the current Host Catalog and Plugin Root. It evaluates the
selected Plan only. Host policy remains responsible for selecting one Plugin
implementation before resolution; this command never invents another Resolver,
changes a selection, starts a Host, or retries an incompatible target.

## Input

Pass one JSON file for every runtime profile selected by the App:

```json
{
  "profile": "lenso.execution-target-capability-profile@1",
  "target_profile": "lenso.native-rust@1",
  "capabilities": ["host-imports", "request", "stream"]
}
```

The contract identifier, field names, and capability vocabulary are shared with
the target admission contract. `target_profile` is the exact opaque runtime
profile from the selected Plan—not a deployment label or an execution-class
guess. It must be 1 through 128 ASCII characters using only letters, digits,
`.`, `_`, `-`, and `@`.

The capability list is deliberately closed and must be strictly sorted and
unique. The current vocabulary is:

- `browser`
- `event`
- `host-imports`
- `native-process`
- `remote`
- `request`
- `stream`
- `wasm-component`
- `websocket`
- `workers`

Unknown values, duplicate values, unordered lists, unexpected fields, or an
incorrect contract identifier are rejected rather than repaired. A missing
declaration means unsupported.

For an App with more than one selected runtime profile, repeat `--profile`:

```sh
lenso app explain \
  --root ./my-app \
  --profile native-target.json \
  --profile bun-target.json
```

Use `--require <target-profile>:<capability>` for an additional adapter-specific
need that is not derivable from the Plan's Capability operation kinds:

```sh
lenso app explain \
  --profile native-target.json \
  --require lenso.native-rust@1:host-imports
```

Repeated target profiles and repeated explicit requirements are rejected as
ambiguous. A supplied profile that does not match a selected target profile is
also rejected, so CI cannot silently validate an unrelated declaration.

## Output and exit status

The command always writes JSON with `kind: "lenso.app-explain"`. It exits zero
only when every selected operation's interaction kind is supported by its exact
target profile. Capability operations contribute `request`, `stream`, or
`event` requirements. A rejected report groups missing features with the Plan
instances, descriptors, and operations that need them.

```json
{
  "schema_version": 1,
  "kind": "lenso.app-explain",
  "status": "rejected",
  "reasons": [{
    "kind": "missing_target_capability",
    "target_profile": "lenso.native-rust@1",
    "feature": "stream",
    "requirements": [{
      "source": "capability_operation",
      "instance": "orders",
      "capability_id": "company.orders@1",
      "operation": "watch",
      "feature": "stream"
    }]
  }]
}
```

This is a pre-readiness gate, not a substitute for an Adapter's real target
qualification. It proves that the declared target profile covers the resolved
Plan; process, browser, Wasm, Worker, storage, failure, and lifecycle evidence
remain owned by their respective target/Host qualification suites.
