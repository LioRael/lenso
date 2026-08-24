# Architecture decision record index

## vNext decisions

ADRs 0030 through 0064 are the accepted, normative architecture decisions for
Lenso vNext. Start with
[`0030-rebuild-lenso-as-a-local-first-modular-runtime.md`](0030-rebuild-lenso-as-a-local-first-modular-runtime.md)
and use [`../../CONTEXT.md`](../../CONTEXT.md) for canonical vocabulary and
routing.

[`0058-select-json-rpc-over-http-for-bun-request-dispatch.md`](0058-select-json-rpc-over-http-for-bun-request-dispatch.md)
records the first Bun wire selection and its reproducible comparison evidence.

[`0059-stream-bidirectionally-between-rust-and-bun.md`](0059-stream-bidirectionally-between-rust-and-bun.md)
records the transport-neutral stream seam and its Rust/Bun conformance evidence.

[`0060-compose-target-web-ui-in-app-and-separate-cross-app-console.md`](0060-compose-target-web-ui-in-app-and-separate-cross-app-console.md)
supersedes ADR 0044 by distinguishing a target-owned App Web UI from an
independent cross-App Console.

[`0061-export-opentelemetry-from-a-removable-module.md`](0061-export-opentelemetry-from-a-removable-module.md)
keeps OpenTelemetry export, trace propagation, and application telemetry in an
optional removable Module.

[`0062-serve-authenticated-game-sessions-through-protocol-modules.md`](0062-serve-authenticated-game-sessions-through-protocol-modules.md)
keeps non-HTTP framing and game-session authorization in replaceable Modules
while the Kernel owns only the typed Stream and Auth seams.

[`0063-scale-native-apps-across-replicated-kernel-lanes.md`](0063-scale-native-apps-across-replicated-kernel-lanes.md)
scales native Apps by replicating the single-owner Kernel across Plan-declared
Execution Lanes and rejects work stealing, runtime Instance migration, and
handler-level thread-pool offloading.

[`0064-keep-only-portable-core-ownership-in-the-main-repository.md`](0064-keep-only-portable-core-ownership-in-the-main-repository.md)
keeps only Plan, Kernel, and core conformance ownership in this repository and
defines the one-way extraction path for runtimes, protocols, Modules, tooling,
and examples.

[`0065-govern-dynamic-plugins-above-the-kernel.md`](0065-govern-dynamic-plugins-above-the-kernel.md)
is a proposed decision that defines Plugins as installation and governance
units above Kernel and preserves immutable App Generations during replacement.

## Legacy decisions

ADRs 0001 through 0029 describe the final v0.3.x Service-oriented
architecture and the design path that preceded vNext. ADR 0030 supersedes them
for vNext. They remain immutable historical and migration evidence rather than
active vNext requirements.

Do not renumber, rewrite, or delete a superseded ADR. Record a new decision and
link the relationship when vNext changes.
