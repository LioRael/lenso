# Cross-repository release evidence

An exact dependency cohort is the delivery boundary between a local change and
a reusable artifact claim. It closes the sources and artifacts actually used by
the selected consumer instead of treating a green owner-repository suite as an
ecosystem result.

Use the retained receipts in [cohorts/](cohorts/) and the machine-readable
[release cohort schema](release-cohort.schema.json). This page defines the
required evidence policy; it does not itself assert that any package is
published or production-qualified.

The [environment and infrastructure cohort map](environment-infrastructure-cohort-map.md)
selects the first affected consumers for this program without duplicating their
current status.

| Change risk | Required evidence | Do not infer |
| --- | --- | --- |
| Local change | Owner unit or integration checks. | Consumer compatibility, a release-ready artifact, or external publication. |
| Contract change | Owner checks plus the affected consumer matrix. | A different Runtime, Adapter, or target. |
| Runtime / Adapter change | Cross-runtime conformance plus the selected target qualification. | Production capacity, deployment, or a different infrastructure implementation. |
| Release-ready candidate | Exact source closure, artifact digests, clean-room install, real package install, startup, shutdown, and the defined upgrade path. | External publication, target qualification, or production qualification. |

The last row is deliberately one cohort per changed dependency closure, not an
automatic request for the whole ecosystem. A pure documentation edit does not
need a multi-repository install run. A public contract or runtime profile change
does, but only for consumers selected by that contract.

## Cohort state is not publication state

`candidate` may name a dirty or incomplete local closure. `release-ready`
requires committed sources, artifacts, and six passed local lifecycle stages.
Its `upgrade` stage must pass too unless the artifact has no persistent upgrade
path; that single `not-applicable` exception requires both a structured reason
and a known limitation. A release-ready cohort remains only a locally retained
delivery candidate. It does not say that a package registry, release channel,
or production environment received anything.

`published` is the only cohort state that records external publication. It
requires the full release-ready closure plus one externally verifiable HTTPS
URL and retained, hash-checked publication receipt for every artifact in the
cohort. The checker verifies the recorded identities and receipts; it neither
publishes a package nor turns an unreachable URL into proof. The canonical
ledger's `Released` facet separately requires its own matching publication
receipt, so a local artifact digest can never be rendered as an external
release.

## What each receipt proves

- **Source closure** — the Git revision and candidate snapshot of every owner
  used by the test are exact. A dirty candidate is visible as such and cannot be
  relabeled as release-ready or published.
- **Artifact digest** — the package, binary, Wasm module, or bundle the
  consumer used is the exact byte sequence named by the receipt.
- **Clean-room install** — the selected consumer resolves from extracted or
  packed artifacts instead of sibling source directories or accidental caches.
- **Real package install** — the package-manager invocation used the same
  artifact closure with normal consumer resolution rules.
- **Startup / shutdown** — a real Host reaches readiness and performs the
  declared clean termination path. A unit test that constructs a type is not a
  substitute.
- **Upgrade** — the previous supported artifact reaches the new artifact using
  the declared migration/compatibility route and preserves its expected data or
  fails explicitly before mutation.

The `lenso-auth-plugin` Workers package check is the reference for Rust owner
packages: it packages real archives, extracts them into a temporary directory,
and verifies a Workers dependency graph contains archives rather than source
paths. A target test remains separate evidence; it should be referenced from
the qualification ledger only for the exact Environment and Infrastructure
combination it exercised.

## Source snapshot rules

Before a local candidate is committed, its base revision alone is insufficient.
The cohort helper computes `lenso.git-worktree-snapshot-v1` from the binary
tracked delta and all non-ignored untracked file digests. The snapshot is bound
to the full base revision. It makes the local candidate reproducible for review
without pretending the source is an immutable remote revision.

Once the candidate is committed, capture a new committed snapshot and rebuild
the artifacts. Never reuse an artifact digest, clean-room result, or upgrade
receipt after a source revision changes. Passing the required local stages may
make the closure release-ready; a stateless artifact may instead record an
explicitly reasoned `upgrade: not-applicable`. Registry publication still
requires the separate published state and publication receipts. Target
qualification and production operation remain separate ledger facets.

### Receipt location and source self-reference

Do not place an uncommitted cohort manifest or its retained receipt inside the
same worktree whose working-tree snapshot it declares. The manifest would alter
the untracked-file set and make its own digest circular. Retain candidate
receipts in a separate evidence directory, or first commit the source candidate
and add the evidence in a later documentation/evidence revision. A verifier
must use a clean checkout of the declared source revision when it checks a
`committed` snapshot.
