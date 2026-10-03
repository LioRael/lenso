# Two Instances, one source App

`plugins/example.label/plugin.rs` owns a typed Plugin with two independently
configured Instances, `left.toml` and `right.toml`. The HTTP Plugin declares
two named dependencies on the existing metadata Capability package. Saved
App-owned choices select the two providers; the resolver derives the bindings.
No Descriptor, codec, factory, Plan or release Artifact is authored here.

With a matching local CLI, from this directory:

```sh
lenso app discover --json
lenso app build --out dist
lenso app start --from dist
```

Request `/instances` at the printed address. The first response is
`["left:1:hello","right:1:hello"]`; the next is
`["left:2:hello","right:2:hello"]`. Config and counters belong to separate
Instance generations. Change one label and rebuild to observe the change.
Native Rust is statically linked and must be relinked; this is not dynamic
replacement. Config, resources and dependency choices remain in built
`intent/plugins/`; source module files are projected out of that runtime Root.

The same source and resolver can be lowered with the existing static Workers
profile:

```sh
lenso app build --target workers --out dist-workers \
  --workers-runtime /exact/lenso-js/packages/lenso-workers-runtime \
  --wasm-bindgen /exact/wasm-bindgen-0.2.127
```

Run the generated Wrangler config locally. Each Workers HTTP event creates a
fresh App, so each response starts at counter 1. Both targets retain the same
logical Instances, configurations and Capability bindings. Workers selects
its Driver and event Ingress implementation; executable bytes need not match.
Target-specific Plugins remain valid and incompatible targets fail admission.

Verify the built Apps together with the pinned local workerd binary:

```sh
python3 verify.py --cli /exact/lenso --native dist --workers dist-workers \
  --workerd /exact/workerd --output verification.json
```

This checks real HTTP responses, independent Native Instance state, event-local
Workers state, Native clean exit and equality of this fixture's resolved graph.
It starts only local processes and stops/reaps them. Omit both Workers arguments
for the Native check. It does not qualify deployed Workers or external HTTP
abnormal-EOF behavior.

Source dependencies use normal Cargo identities and generated linkage. Shared
source availability requires explicit Instance selection. Default development
does not require `plugin pack`, a frozen release, or installation. Bundles and
exact Artifact identities remain the distribution/portable boundary. Existing
single-Plugin package metadata and custom Hosts retain their existing behavior.

The same built App also has a deterministic Simulated entry. It reads the same
CLI distribution authority and Root through `resolve_runtime_app`, uses the
same linked source Plugins, and starts Kernel with that exact Plan. Only the
Driver and the socket-free event Ingress entry differ; no routes, configs,
Instances, permissions or business bindings are rebuilt in the test:

```sh
LENSO_SOURCE_INSTANCE_DISTRIBUTION=/absolute/dist \
  cargo test -p lenso-source-first-instances --test simulated -- --ignored
```

This exercises the exact Rust App in Native, Workers and Simulated. The mixed
TypeScript example is qualified in Native and Workers; its process Adapter is
not claimed as a deterministic Simulated combination.
