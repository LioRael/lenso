# Native greeting Host with a business snapshot

`greetings-web` is an explicit, single-process Native Host example. Without
`--business-policy`, it runs the original static greeting Plugin. With a Host
policy, it binds one versioned business object to the Plugin and will not open
the TCP listener until the first value is authorized and validated.

Create an absolute-path JSON snapshot file:

```json
{
  "schema": "lenso.business-snapshot.v1",
  "object": {
    "plugin_id": "company.greetings-http",
    "instance_key": "default",
    "object_key": "greeting-policy"
  },
  "revision": 1,
  "value": { "exclamation_count": 2 }
}
```

Create a separate Host-owned policy file, replacing the source path with the
snapshot's absolute path:

```json
{
  "schema": "lenso.example-greeting-business-policy.v1",
  "source_reference": "greeting-operator",
  "source": { "type": "file", "path": "/absolute/path/to/business.json" },
  "max_stale_seconds": 10,
  "poll_millis": 100
}
```

Run `cargo run --locked -p lenso-web-greetings-app-example --bin greetings-web -- --business-policy /absolute/path/to/operator-policy.json --bind 127.0.0.1:8080`, then POST JSON such as `{"name":"Ada"}` to `/greetings`. The response includes `policy_revision` and uses the pinned value for that request. Replace the snapshot atomically with a higher revision to refresh it. Reusing a revision with different content, changing the object/source, adding a field, or supplying an invalid value is rejected. The last accepted value remains available only until `max_stale_seconds` elapses without a valid source proof. Changing or removing the Host policy stops the listener.

The source may instead be `{"type":"https","url":"https://example.invalid/policy","admitted_origins":["https://example.invalid"]}`. HTTPS polling and ETag validation are supplied by `lenso-engine-authoring`; this example's real-TCP test covers the file source. The policy file and source location must be controlled by the Host operator. Diagnostics expose stable event codes and revisions, not snapshot values.

This is a custom Host slice. It does not configure the default generated App Host, persist a knowledge base, or implement a production policy-distribution/control plane.
