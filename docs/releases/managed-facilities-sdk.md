# Managed facilities SDK release candidate

This stage packages the managed facility changes landed at
`c0464691b6ae1c80b5606065dd4073e8b7d6a59e`, plus registered snapshot
compatibility and selected Root feature/linking fixes, under fresh versions.
It has not been published. Existing Native and Workers qualification receipts
remain bound to their original source commits and artifacts.

The exact proposed set is
[`managed-facilities-sdk.json`](../../.github/release-sets/managed-facilities-sdk.json).
It contains 35 packages. The set includes every incompatible exact reverse
dependency of the new Runtime, Native Adapter and authoring packages, including
the target-specific Workers Driver dependency and the optional CLI engines.
Package versions and generated scaffold requirements advance together.

The principal versions are Kernel `0.3.12`, runtime codec `0.4.4`, Native
Adapter macros `0.2.8`, Native Adapter `0.3.19`, facade `0.5.28`, Workers Driver
`0.1.2`, HTTP Endpoint `0.3.6`, Engine App `0.3.2` and CLI `0.6.5`.
Kernel includes dependency-ordered Prepare/Construct before the unchanged
Activate/Ready phases. Publishing only the macros, Adapter and facade would
leave the previously reproduced mixed-authoring startup failure unresolved.

Contract authoring advances to `0.1.2`, codegen to `0.10.1`, and authoring macros
to `0.1.1`. The registered base snapshot layout is preserved; only contracts
that explicitly select request admission return the new admitted snapshot.
Default macro expansions remain compatible with existing registered Roles
that use the older build-time authoring package. New codegen accepts both
snapshot forms. The finite queue and concurrency settings remain source-owned;
existing contract identity, descriptor and projection bytes are not rewritten
merely to bump dependency versions.

The crates.io HTTP Endpoint `0.3.5` archive was checked on 2026-09-30. Its
source files and descriptor match the preceding source checkpoint exactly;
its normalized manifest still requires Kernel `=0.3.11`. The new dependency
cohort therefore uses `0.3.6`; neither `0.3.5` nor any other registered version
is overwritten. The Web template now declares the new coherent cohort and
requires its publication before a registry-only generated consumer can run.

## Artifact validation and publication boundary

From the committed candidate, run the existing repository preflight with its
exact SHA and set:

```sh
RELEASE_SHA="$(git rev-parse HEAD)" \
EXPECTED_RELEASE_SET="$(cat .github/release-sets/managed-facilities-sdk.json)" \
bash .github/scripts/release-cohort-preflight.sh
```

The preflight derives package order from declared dependencies, replaces
out-of-stage sources with their exact registry archives, and compiles extracted
cohort archives in a clean workspace. Its source overlay models staged
predecessors; it does not prove registry visibility or authorize publication.
Retain its terminal result and per-package digests with the exact candidate CI.

The selected version must be the sole identity in its SemVer-compatible line.
Older incompatible registry codegen and legacy Codec lines remain independent
dependencies and are recorded separately. This permits immutable older Role
archives without accepting a second Kernel or Codec identity in the selected
runtime line. Missing candidates, alternate sources and compatible-line
duplicates fail the preflight.

The dependency order is authoring/native macros and Kernel first, followed by
authoring/codegen and codec, Runtime Drivers and Adapters, Capability/provider
packages, then the facade and Engine/CLI closure. The script resolves the exact
order, including optional and package validation dependencies.

The `release-plz.yml` workflow is dispatch-only and defaults to `dry-run`.
Main or candidate pushes do not publish this set. After successful candidate
CI, a fresh fast-forward landing and artifact review, a separately authorized
dispatch may publish only the reviewed SHA and set. Read back every resulting
registry version and archive before qualifying a registry-only consumer.

## Downstream owners

Auth, Access Control, Audit, Approval, Console and Agent source qualification
can continue using their explicit immutable Git cohort. Their required
normalized archive checks cannot be called passed while the published Adapter
macros lack private facility lowering.

For registry delivery, first make the Core stage visible. Then update each
selected owner's direct SDK/Kernel/codec requirements and package its normal
archive against that published stage. Release unchanged public Role archives
again only if their packaged source or dependency manifest changes; an existing
caret requirement that already admits Kernel `0.3.12` can keep its registered
archive. Existing exact codec or authoring/codegen requirements must be rolled
when the selected graph needs the new cohort. Registered owner versions that
change need fresh versions; first publication of an unregistered owner package
remains an explicit owner action.

Qualify the resulting ordinary Native/Workers Source App after owner archives
resolve one runtime type identity. Publication, owner archive gates, actual
management behavior, and production deployment remain separate evidence.
