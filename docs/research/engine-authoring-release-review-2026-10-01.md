# Combined authoring release: parent review checkpoint

The multi-Plugin and configurable Web/Contracts implementations are integrated
locally. Final Relay acceptance passed 13 suites / 21 tests plus TypeScript.
Both default and customized Web source policies passed real loopback HTTP tests
and clean shutdown. The original 87 projection hashes, previous receipts, prior
package and approved Relay MCP configuration are preserved.

Relay receipts: `lenso-ai-relay/tests/phase1/engine-authoring-20261001/`.
Tested package: `/tmp/lenso-relay-engine-authoring-20261001-final`.
Reviewed package: `/tmp/lenso-relay-engine-authoring-20261001-reviewed`.
The reviewed package includes the publication allowlist metadata; its four
executable/JavaScript payload hashes match the package actually tested.

## Candidate MCP result and boundary

The candidate CLI was built from this combined worktree, then passed explicitly
to the existing read-only adapter in an ephemeral process. Persistent config
was not changed. Facts returns `status=invalid`, discovers 15 source Plugins,
and reports `LENSO_APP_RESOLUTION_FAILED`. Project check returns an error.

The bounded local explanation identifies the actual missing authority:
`.lenso/host-catalog.json` is absent. Engine authoring's `inspect_plugin_root`
requires that Catalog before resolving Root Instances and bindings. Relay is a
custom Host with independently verified runtime composition; source discovery
is not a persisted Engine App admission record. The new App freshness gate also
rejects `.lenso/contracts` without an App `freshness.json`; the direct Contracts
processor has its own accepted/cache policy and does not manufacture App records.
No metadata was fabricated and the failure was not converted to a passing check.

This is a verified limitation for parent review, not a CLI-version-only bug.
Changing custom Host admission/MCP semantics is outside this implementation.
Current Relay packaging correctly uses its direct read-only Contracts check.

## Proposed version/cohort plan

The reviewable 22-package proposal is
[`engine-authoring-proposed-release-set-2026-10-01.json`](engine-authoring-proposed-release-set-2026-10-01.json).
Reasons and existing versions are in
[`engine-authoring-version-plan-2026-10-01.json`](engine-authoring-version-plan-2026-10-01.json).
These are proposals; manifest versions and generated headers have not yet been
changed. The set includes the changed packages and conservative reverse closure
of exact-pinned runtime dependencies, so new facade/adapter/Host type identities
are published as one compatible cohort.

Key choices:

- `lenso-engine-authoring 0.3.0`: public `Candidate`/`Compilation` struct additions
  break downstream struct literals. A pre-1.0 minor is appropriate.
- `lenso-cli 0.7.0`: its `lenso_app_authoring` library reexports that API, so it
  should not describe the breaking reexport change as a compatible patch.
- `lenso-contract-codegen 0.10.2`, native macros `0.2.9`, native adapter `0.3.20`,
  facade `0.5.29`: additive implementation plus exact dependency propagation.
- `lenso-engine-app 0.3.3` remains a valid new release: registry readback shows
  only `0.3.2` published. The two new support crates start at `0.1.0`.
- Other existing runtime/Host consumers in the proposal receive patch versions
  and updated cohort dependency requirements. CLI/Agent conventions are still
  outside scope; dependency version propagation adds no such feature.

Registry readback was performed for all 22 identities. The two new support
names return 404. Engine authoring `0.2.5`, App `0.3.3` and CLI `0.6.6` in the
current source are still unpublished; do not infer publication from manifests.

Before the final release candidate, apply the chosen versions, update exact and
minimum internal dependency requirements, scaffold/local Host cohort pins,
published examples and affected assertions. Update lockfiles with Cargo.
Changing the codegen package version changes generated headers: regenerate
with the official generator, preserve the historical 87-byte-identity evidence,
and separately verify the expected header changes and stable contract semantics.
Synchronize the consumer overlay and record its new hashes.

## First-publication blocker and staging

Both new manifests now explicitly contain `publish = true`, as required by the
repository allowlist. However `release-gate.sh` in publish mode rejects a crate
identity that has never been first-published. This was confirmed by source and
the two registry 404 results; dry-run does not waive the publish restriction.

Parent must coordinate the existing owner bootstrap mechanism and Trusted
Publisher registration for the two names. This task did not access credentials,
alter publisher/security configuration or publish a placeholder crate. The
current implementation cannot be first-published against old generator
`0.10.1`, because it uses the new generator fingerprint API.

A concrete staging order with the current gate is:

1. Land and dry-run the final exact candidate/full cohort after review and CI.
2. Publish the approved existing-identity generator dependency stage, including
   any unpublished prerequisites found by the gate, from that same source SHA.
3. Owner first-publishes the exact support artifacts, with Contracts depending
   on the now-available new generator, and verifies publisher ownership.
4. Recompute the remaining unpublished release set; already published bootstrap
   and generator versions must be removed because the gate rejects them in the
   requested set. Dry-run and publish the remaining approved cohort, with exact
   artifact and registry readback for each stage.

If parent chooses another existing approved bootstrap path, record it explicitly.
Do not relax the gate or introduce a parallel publisher in this task.

## Final candidate commit and required checks

The implementation review snapshot precedes version preparation. Its local
base is `119b9af70b82816588c00c8bf812f7c62d1a18b5`; no candidate CI or landing has
occurred. The final publishable candidate must include all version/pin/lock and
generator-header changes before its full SHA is recorded. Suggested final
commit subject: `feat(authoring): ship configurable Web and Contracts cohort`.

Parent should read back current remote main, integrate any changed base without
discarding task changes, and obtain Delta review of the final immutable SHA.
Run focused checks for changed preparation files, then one candidate push CI.
The existing `CI` workflow requires formatting, release-script/cohort tests,
workspace Clippy with warnings denied, test compilation/tests, native/Bun
conformance, and the `wasm32-unknown-unknown`/`wasm32-wasip2` checks. Local macOS
acceptance is not a replacement for that Linux/Rust 1.94/Wasm gate.

Record the successful run for that exact SHA and fast-forward the same commit
to main. A changed candidate/base requires fresh applicable evidence. Do not
automatically repeat the same full suite after landing the identical SHA.
Release-plz dry-run/publication uses the landed SHA and exact package/version
set; publication additionally requires current main and owner cutover status.
Keep landing, CI, registry publication and deployment as separate recorded facts.
No PR delivery or deployment is proposed.

Remaining risks: publisher bootstrap/ownership; choosing and applying the
breaking API version increments; coherent exact pins and Rust type identities;
generator header/cache invalidation during preparation; final Linux/Rust 1.94
and portable checks; the intentionally unresolved custom Host MCP authority gap.
