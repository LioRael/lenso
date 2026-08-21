# Architecture decision record index

## vNext decisions

ADRs 0030 through 0059 are the accepted, normative architecture decisions for
Lenso vNext. Start with
[`0030-rebuild-lenso-as-a-local-first-modular-runtime.md`](0030-rebuild-lenso-as-a-local-first-modular-runtime.md)
and use [`../../CONTEXT.md`](../../CONTEXT.md) for canonical vocabulary and
routing.

[`0058-select-json-rpc-over-http-for-bun-request-dispatch.md`](0058-select-json-rpc-over-http-for-bun-request-dispatch.md)
records the first Bun wire selection and its reproducible comparison evidence.

[`0059-stream-bidirectionally-between-rust-and-bun.md`](0059-stream-bidirectionally-between-rust-and-bun.md)
records the transport-neutral stream seam and its Rust/Bun conformance evidence.

## Legacy decisions

ADRs 0001 through 0029 describe the maintained v0.3.x Service-oriented
architecture and the design path that preceded vNext. ADR 0030 supersedes them
for vNext. They remain immutable historical and migration evidence rather than
active vNext requirements.

Do not renumber, rewrite, or delete a superseded ADR. Record a new decision and
link the relationship when vNext changes.
