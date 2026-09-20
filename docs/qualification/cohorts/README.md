# Exact dependency cohorts

This directory stores retained, machine-checked release-cohort receipts. A
cohort is a closed source-and-artifact set for one cross-repository delivery
candidate. It is deliberately narrower than a workspace-wide CI run.

The receipt format is declared by
[`../release-cohort.schema.json`](../release-cohort.schema.json) and checked by
[`scripts/check-release-cohort.mjs`](../../../scripts/check-release-cohort.mjs).
Each receipt names all of the following independently:

1. the exact source closure, including its full Git revision and deterministic
   source snapshot digest;
2. each built package or executable artifact, its byte size, and SHA-256
   digest;
3. a retained receipt for source closure, artifact digest, clean-room install,
   real package install, startup, shutdown, and upgrade; and
4. any remaining limitations.

`state: "candidate"` means an owner may use a committed tree or an explicit
working-tree snapshot while preparing local evidence. It never means
release-ready or published. `state: "release-ready"` requires committed
sources, artifacts, and six passed local stages. Its `upgrade` stage must also
pass unless the package has no persistent upgrade path; only then may it be
`not-applicable`, with both `not_applicable_reason` and a known limitation. It
is still only a locally retained delivery candidate and never claims registry
publication.
`state: "published"` additionally requires one externally verifiable HTTPS
URL and retained publication receipt for every artifact in the closure. The
checker verifies the recorded URL and receipt identity but does not publish or
network-validate on behalf of the owner.

Only a published cohort can support the qualification ledger's `Released`
facet, and that ledger entry must also carry its own retained publication
receipt. Target and production qualification remain separate evidence facets;
an immutable release-ready or published source closure does not replace their
exact deployment or operational evidence.

No current environment-and-infrastructure implementation is recorded here
until its same-repository changes are assembled into one source closure. The
active worktrees intentionally do not form a fake cohort just because they
share a base revision.

## Capture procedure

Use the snapshot command once per candidate owner checkout. It includes both
the binary Git delta and non-ignored untracked files, relative to `HEAD`:

```sh
node scripts/snapshot-release-cohort.mjs \
  --repository LioRael/lenso-runtime-rust \
  --workspace /absolute/path/to/assembled-runtime-checkout \
  --coordinate cargo:lenso-plugin-bundle@NEXT
```

Place the emitted source object in a receipt, then collect the real artifacts.
The Auth owner package check already provides the desired Rust pattern: it
packages `.crate` archives, extracts them into a temporary clean room, patches
only those archives, and compiles the extracted Workers closure. Do not replace
that with source-path patches when making the cohort receipt.

Check a retained receipt structurally and against the exact local sources and
artifacts:

```sh
node scripts/check-release-cohort.mjs \
  docs/qualification/cohorts/example.json \
  --workspace LioRael/lenso-runtime-rust=/absolute/path/to/runtime \
  --artifact cargo:lenso-plugin-bundle@NEXT=/absolute/path/to/lenso-plugin-bundle-NEXT.crate
```

The check fails if a worktree, artifact, or retained receipt is altered after
capture. A release coordinator must make a new cohort after any source revision
or artifact changes; a previous pass does not transfer to an amended candidate.

## Ledger linkage

A source reference in `qualification-status.json` may carry a `cohort` object
only when its repository, source revision, and snapshot digest match a retained
cohort receipt. Working-tree cohort sources may support **local** qualification
only. Target and production claims using a cohort require an immutable
`release-ready` or `published` source closure plus their relevant retained
evidence. A `Released` ledger facet requires `published` and its external
publication receipt; it cannot be inferred from a release-ready clean-room
result.
