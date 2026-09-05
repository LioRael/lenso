# Plugin design adoption and delivery boundary

Status: **Design approved on 2026-09-04; the specified first Request delivery
completed across owner repositories on 2026-09-05.**
Date: 2026-09-04.

[Specification #699](https://github.com/LioRael/lenso/issues/699) records the
accepted formats and owner-local cases. [Delivery tracker #695](https://github.com/LioRael/lenso/issues/695)
records the released Core, Runtime, Bun, Agent, CLI, and example evidence. Later
language/runtime expansion remains demand-driven rather than part of this delivery.

This is the adoption decision for the
[consolidated authoring review](2026-09-04-plugin-usage-walkthrough.md).
The repository owner approved the design set, including ADRs 0073 and 0074,
on 2026-09-04. The subsequent implementation specification and owner-local
issues delivered that first scope without another round of language/runtime
expansion. The example annotations remain design explanations rather than the
versioned SDK API reference.

## Three independent adoption decisions

| Decision | Changes | Must not change implicitly |
| --- | --- | --- |
| Use new authoring forms | Source declarations, construction glue, generated bindings, product SDK conveniences | Account selection, Host fault policy, durable data format, or isolation class. |
| Adopt named dependencies | Requirement identity, scoped routing, saved single/optional choices, and supported metadata/profile versions | Capability business meaning, provider substitution, collection membership semantics, or authorization. |
| Adopt Host essential-instance policy | Terminal failure impact after readiness, derived from Host roots and required closure | Strict initial startup, retries/replay, physical isolation, or dependency choices. |

An application may adopt them at different times, subject to the actual format
and peer support needed by each. There are no three new Plugin kinds or a
mandatory matrix of user-selected compatibility modes. Tooling infers support
from exact versions and contracts and explains unsupported combinations.

## Existing source and releases

Existing immutable releases remain unchanged. Running an existing release with
its supported, pinned Host/runtime combination needs no source rewrite merely
because a new SDK is published. This is not a promise that every old binary or
source file will run under every future runtime version.

For supported current vNext authoring surfaces, a new SDK may retain old Port
and lifecycle syntax when it lowers unambiguously to the same runtime model.
Where source compatibility cannot be preserved, declare the version boundary
and provide a focused migration; do not silently reinterpret hooks, construction
order, or error results. Do not create a second runtime or revive v0.3.x shims.

Migrating a source Plugin produces a new immutable release. Review its generated
configuration, dependencies, provided contracts, tool catalog where applicable,
and lifecycle behavior against the prior release. Keeping the same schema or
hashing a generated artifact does not prove unchanged business meaning.
Retain the old reproducible build/version path until the replacement is admitted;
do not patch existing bundles or overwrite their artifacts.

A prebuilt old bundle is inspected as data if its format is supported. New
non-evaluating TS declaration extraction is not retroactively true of its old
source build. Source using an unsupported declaration pattern needs explicit
migration or its documented prior trusted build toolchain; a new checker must
not silently fall back to importing application code while claiming the new
extraction guarantee.

No deprecation date or indefinite dual-source-support promise is set here.
Remove a retained surface only through an explicit compatibility policy with a
working replacement, migration instructions, and affected-owner evidence.

## Named-dependency migration

Run adoption through the normal configuration authority, not during startup.
The operation previews the old resolved bindings and the proposed named ones.
Preserve each exact logical provider identity, not its label, installation
position, or a newly matching account. Persist explicit optional absence too.

An old requirement can map automatically only when its old-to-new identity is
unambiguous. A single old Store requirement does not tell the tool whether it
became `source`, `destination`, or both. Splits and public identity changes need
an explicit migration decision. Private Rust field or TS variable renames do
not change stable public requirement IDs.

Validate the complete candidate with Host policy and all required peers before
publication. Existing semantic revision and source-conflict checks protect
concurrent edits. Preserve dormant consumer intent and report what must be
repaired before reactivation. An unresolved old Root may be repaired, but must
not be treated as evidence for an automatic previous account choice.

Unsupported old readers reject the adopted format before dispatch. A legacy
Capability-only consumer is compatible only where its single requirement can
still be projected exactly; multiple named requirements never collapse into a
Capability-only lookup even when they select the same instance. Conversely, an
older provider implementation can serve a new named consumer only when its
Capability and the complete runtime/profile path preserve the binding and
invocation semantics. Do not equate provider language or package age with
compatibility.

Publication records desired intent; runtime activation remains a coordinated
supported update. If validation fails, preserve the previous intent/runtime.
If activation later fails, show desired versus active state and the cause; do
not claim that every file publication provides an atomic runtime rollback.
Going back requires a supported previous artifact/configuration combination.
Do not erase named choices or optional absence to make an old reader accept it.
Any downgrade that changes meaning requires explicit review.

## Configuration, private data, and fault policy

New configuration declarations preserve existing defaults and validation unless
an intentional release change says otherwise. A field rename or tightened
validation may require configuration migration. SDK adoption grants no access
to or understanding of private Plugin data and does not impose DataDir or a
common storage engine. Data and implementation switches retain their own
compatibility checks; code rollback does not undo a schema migration or write.

Existing Host fault behavior stays in effect until separate adoption of the
[fault-scope proposal](2026-09-04-plugin-fault-scope.md). The Host reviews which
instances cease to be terminal after exhausted recovery, including transitive
and shared required providers. Existing explicit critical instances remain
essential unless deliberately changed. A broken selected Plugin still prevents
initial readiness. Native execution limits still apply.

## First delivery: one complete Request authoring path

The approved first delivery boundary is the document-sync example, not a framework-wide
API replacement. Keep an official Rust implementation and the TS comparison
implementation against the same public contract. Use Rust Native Store instances
in the test Host, an installable Rust Process sync implementation, and a TS Bun
sync implementation. Exercise each sync implementation against the same named
`source` and `destination` bindings; mixing languages is ordinary Capability use.
Select one implementation per instance under existing Host policy, never fallback.

These are design proof targets, not claims that all required SDK/profile paths
exist today. The implementation specification must verify their exact owners and
baseline and explicitly revise the target if an unavailable Adapter blocks it.
No dynamic Rust library ABI, new JS engine, or compulsory Wasm variant is needed
to complete this first slice. Existing supported execution profiles remain
subject to their compatibility gates, and unsupported new semantics must be
rejected rather than silently approximated.

Deliver the slice in dependency order:

1. **Standard contracts and rejection paths.** Specify named requirements,
   client projection, reader/profile support, build-output shape, and lifecycle
   ownership against exact released versions. Introduce supporting readers and
   validators before writers publish new required formats. Keep current Host
   fault semantics. Allocate version numbers here, not in exploratory sketches.
2. **Authoring and runtime binding.** Add Rust construction and TS declaration
   extraction/factory binding through existing SDK owners. Agent helpers lower
   to ordinary ToolProvider declarations. Implement the necessary scoped client
   and cancellation/cleanup accounting at their actual owners; do not advertise
   the complete authoring path from macros or generated types alone.
3. **Configuration, packaging, and adoption.** Add exact-choice materialization,
   migration previews, artifact/profile checks, and ordinary scaffold/dev/pack
   integration. A newly scaffolded sample starts without hand-editing generated
   files. Existing immutable releases and supported single-requirement consumers
   remain covered by focused compatibility cases.

The first slice is complete only when source, generated contracts, bundle,
Host binding, invocation, failure, and cleanup form one demonstrated path.
Required evidence includes independent same-Capability account selection,
optional absence preservation, declaration drift rejection, offline bundle
inspection without Plugin execution, constructor failure/late return, and
cancellation without unsafe cleanup or duplicate write replay. Use the existing
core conformance and real selected Adapter paths; avoid an all-language-by-all-
runtime matrix. Do not claim new performance gains without measurement; retain
native typed dispatch where applicable.

## Follow-on work and implementation handoff

- Expand first-class Stream/Event and managed scheduling authoring using their
  accepted contracts and actual product needs. A Request-only SDK is not the
  long-term ceiling. Do not bundle unrelated runtime rewrites into this work.
- Bring new semantics to additional Wasm/other supported profiles with explicit
  host-import, cancellation, resource, and interaction support. Existing Wasm
  functionality is not removed by this sequencing.
- Deliver accepted ADR 0074 Host essential-instance failure policy as an independent
  architecture change. It need not wait for every language feature, and no SDK
  update silently enables it.

The owner approved authoring semantics, the bounded product SDK
build interface, named-dependency adoption, and the first delivery boundary.
Fault-scope policy is accepted separately in ADR 0074. New syntax spelling, TS supported
expression grammar, file representation/transaction details, exact versions,
and owner-local implementation tickets must be made concrete before their
respective code changes; they are implementation specifications constrained by
this design, not reasons to add more framework concepts.

Implementation tickets and their completed dependency order are recorded in
[Issue #695](https://github.com/LioRael/lenso/issues/695). Design approval alone
did not perform a migration or release; the linked owner-local deliveries did.

Start with [specification #699](https://github.com/LioRael/lenso/issues/699).
That issue records the inspected baseline, resolved prerequisites, and exact
version table used by the installable Rust Process and TS Bun proof. The parent
records the eight completed first-slice delivery tasks and the independent
[ADR 0074 delivery #702](https://github.com/LioRael/lenso/issues/702).
Ticket bodies and prerequisite evidence live in GitHub, not duplicate local plans.
