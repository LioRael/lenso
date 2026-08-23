# Execution Adapter recipe

An Execution Adapter owns one open execution class and translates selected
Module Instances into generation-owned endpoints/lifecycle. It may own process,
wire, isolation, codec, and host-failure mechanics. It receives the already
resolved Plan and cannot acquire packages, select providers, or resolve a
second graph.

## 1. Implement the current Interface

Inspect the selected `lenso-kernel::ExecutionAdapter` trait first:

```rust,ignore
pub trait ExecutionAdapter: std::fmt::Debug + 'static {
    fn execution_class(&self) -> ExecutionClassId;
    fn prepare(
        &self,
        plan: &ResolvedAppPlan,
    ) -> Result<PreparedNativeApp, RuntimeFailure>;
    fn recreate(
        &self,
        plan: &ResolvedAppPlan,
        instance_key: &str,
    ) -> Result<PreparedNativeModule, RuntimeFailure>;
}
```

`recreate` may retain the default truthful failure when the Adapter has no
recoverable generation boundary. Do not claim restart support without a fresh
resource/process generation and cleanup proof.

## 2. Prepare deterministically

Implement `prepare` in this order:

1. validate the canonical `ResolvedAppPlan`;
2. select only Instances whose `execution_class` equals this Adapter's class;
3. validate every package/factory/entrypoint/configuration input the Adapter
   owns;
4. load or construct the generated codecs required by each declared endpoint;
5. verify Capability identity, exact Descriptor version, Operation table, and
   request/stream/event kinds before business dispatch;
6. create one fresh `PreparedNativeModule` with exact endpoint sets and a
   generation-owned lifecycle for every selected Instance;
7. materialize `PreparedBinding`, `PreparedStreamBinding`, and
   `PreparedEventBinding` only from the Plan's explicit provider keys; and
8. return one `PreparedNativeApp` contribution for the catalog to merge.

Reject missing/duplicate factories, invalid entrypoints, missing codecs,
endpoint mismatches, duplicate Instance generations, unsupported kinds, and
protocol handshake mismatches. Stop spawned processes/reserved resources when
later preparation fails.

## 3. Own the host mechanics

For an out-of-process Adapter, keep these inside the Adapter:

- process spawn, environment and working-directory policy;
- handshake/session identity and exact endpoint agreement;
- frame/body limits, bounded queues, request IDs, deadlines, cancellation, late
  messages, and protocol violations;
- typed generated value encoding/decoding at the wire boundary;
- child exit/stderr and transport error mapping; and
- shutdown, cleanup, and fresh-process recreation.

The Module package implements generated Providers. It does not parse Lenso wire
messages or manage Adapter child-process flags.

Current source anchors are the native registry in
`LioRael/lenso-runtime-rust/crates/lenso-native-adapter` and the production
process implementation in
`LioRael/lenso-bun-adapter/crates/lenso-bun-adapter/src/adapter.rs`.

## 4. Prove the boundary

Run the product-neutral request/stream/event conformance supported by the
Adapter, plus real host tests for:

- absent executable/entrypoint and startup handshake rejection;
- exact endpoint/version/Operation mismatch;
- success, open/unknown Domain Error, and every mapped Runtime Failure;
- bounded admission/frame/message behavior;
- deadline, cancellation, duplicate/late frame, provider crash, and shutdown;
- recreation with a fresh generation; and
- consumer/provider combinations across every claimed runtime direction.

## Completion

The Adapter branch is complete when its execution class can be added/removed
from a Runner catalog independently, all selected Instances contribute exact
Plan-owned generations/bindings, malformed host state fails before readiness,
and real host conformance passes without changing a Capability contract.
