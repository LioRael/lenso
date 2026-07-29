# Verifiable Provenance for Official Linked Module Releases

Research date: 2026-07-29

## Question

Which immutable crates.io, GitHub, source-repository, checksum, ownership,
provenance, and build evidence can Lenso verify automatically for an official
Linked Module Release, and which desired guarantees are unavailable or require
additional infrastructure?

## Decision

An official Linked Module Release should use the crates.io `.crate` archive as
its canonical package artifact and identify it with an exact crate name,
version, byte size, and registry-computed SHA-256 checksum. Lenso should require
crates.io Trusted Publishing from an approved GitHub repository and workflow,
plus a GitHub artifact attestation whose subject is the exact `.crate` archive
downloaded from crates.io. The official catalog should normalize and preserve
all verification results in its own immutable review receipt.

This provides a verifiable chain:

```text
approved publisher and repository
  -> approved GitHub workflow at an exact commit
  -> crates.io Trusted Publishing record
  -> immutable crate name/version and SHA-256
  -> GitHub attestation for those exact crate bytes
  -> independent Lenso compatibility build and smoke receipt
```

It does **not** prove that the crate is safe, that its source archive is exactly
the source tree at the claimed commit, that its compiled output is reproducible,
or that every possible feature/target combination works. Those are separate
claims requiring independent review or additional build infrastructure.

## What crates.io can prove

### Immutable package identity and bytes

Publishing a crate version is permanent: the version cannot be overwritten and
its code cannot be deleted. A version can be yanked, but yanking does not replace
the published archive. Cargo's registry index records a `cksum` computed by the
registry, and index records are not meant to change after publication except for
the `yanked` field. Therefore Lenso can automatically:

- resolve an exact `{crate, version}` from the crates.io sparse index or API;
- download the canonical `.crate` archive;
- calculate SHA-256 and require equality with the index/API checksum;
- record the archive byte size and publication timestamp;
- reject new installs while the version is yanked, while retaining the exact
  bytes and receipt for already-installed applications.

Sources: [Cargo publishing permanence](https://doc.rust-lang.org/cargo/reference/publishing.html),
[Cargo registry index fields and mutation rule](https://doc.rust-lang.org/cargo/reference/registry-index.html),
and the source-owned [crates.io version API schema](https://github.com/rust-lang/crates.io/blob/96652a2cf8dfebd129c83820094eb1af365750a4/crates/crates_io_api_types/src/lib.rs#L897-L914).

The checksum is an integrity identifier, not a signature. It proves that the
downloaded archive is the same archive represented by the crates.io index; it
does not identify who created its contents.

### Publication actor and Trusted Publishing origin

The version API exposes two distinct publication forms:

- token publication can expose `published_by`, the crates.io user recorded for
  the version, and `audit_actions`;
- Trusted Publishing exposes the unstable `trustpub_data` field. For GitHub it
  contains `provider`, `repository`, workflow `run_id`, and commit `sha`.

During token exchange, crates.io verifies the GitHub OIDC token against the
configured repository owner identity, repository name, workflow filename, and
optional environment before issuing a short-lived publishing token. The
crates.io implementation then stores the repository, workflow run ID, and commit
SHA on the published version. This proves that crates.io accepted the upload
through a configured GitHub workflow identity; it is materially stronger than a
self-declared `repository` URL in `Cargo.toml`.

Sources: the official [Trusted Publishing documentation](https://crates.io/docs/trusted-publishing),
the crates.io [OIDC token-exchange implementation](https://github.com/rust-lang/crates.io/blob/96652a2cf8dfebd129c83820094eb1af365750a4/src/controllers/trustpub/tokens/exchange/mod.rs#L90-L226),
the persisted [Trusted Publisher data model](https://github.com/rust-lang/crates.io/blob/96652a2cf8dfebd129c83820094eb1af365750a4/crates/crates_io_database/src/models/trustpub/data.rs#L7-L26),
and the [public version API definition](https://github.com/rust-lang/crates.io/blob/96652a2cf8dfebd129c83820094eb1af365750a4/crates/crates_io_api_types/src/lib.rs#L930-L949).

`trustpub_data` is explicitly marked unstable. Lenso should read it at catalog
review time, normalize the supported fields into a versioned Lenso receipt, and
preserve the raw response digest. Runtime installs should depend on the stable
Lenso receipt rather than assume the crates.io response shape will never change.

Trusted Publishing cannot create the first version of a new crate. If it is a
hard official-release requirement, a publisher must bootstrap the crate with a
non-official initial version and submit a later Trusted-Published version for
official inclusion. The official crates.io documentation lists an already
published crate as a prerequisite.

### Ownership evidence and its limit

The Cargo registry Owners API can list current crate owners. This is useful for
detecting current control changes, but owners can be invited and removed, so the
current list is not immutable publication-time evidence. Likewise,
`published_by` identifies the recorded publishing user for token publication but
does not prove that the user's GitHub repository built the archive.

Source: [Cargo Registry Web API owners endpoints](https://doc.rust-lang.org/cargo/reference/registry-web-api.html#owners).

Official publisher governance must therefore be separate from crates.io crate
ownership. The catalog should pin the approved publisher, GitHub account or
organization numeric ID, repository numeric ID, and allowed repository. It
should snapshot the crates.io owners at review time and re-check them as a drift
signal, not use the live owners list as the root of trust.

## What GitHub can prove

### Exact repository, commit, workflow run, and signature state

Given `trustpub_data.repository`, `run_id`, and `sha`, Lenso can query GitHub's
source-owned APIs and verify:

- the repository's immutable numeric ID and owner's numeric ID match the
  catalog-approved publisher identity;
- the full commit exists in that repository;
- the workflow run belongs to that repository, has the same `head_sha`, follows
  an allowed workflow path, and completed successfully;
- the run event, attempt, actor, and runner policy satisfy catalog policy;
- GitHub's commit `verification` result if signed commits are required.

The workflow-run API exposes the run ID, workflow path, `head_sha`, event,
status/conclusion, attempt, actor, and repository identity. The commit API
exposes GitHub's signature-verification result and reason.

Sources: [GitHub workflow runs REST API](https://docs.github.com/en/rest/actions/workflow-runs),
[GitHub repository REST API](https://docs.github.com/en/rest/repos/repos), and
[GitHub commit signature verification](https://docs.github.com/en/rest/commits/commits#signature-verification-object).

A verified commit signature authenticates the signed commit according to
GitHub's verification record. It does not bind that commit to crates.io bytes;
that binding comes from the package attestation and checksum checks below.

### Artifact attestation for the registry archive

GitHub artifact attestations bind a named artifact digest to a signed in-toto
statement using a short-lived Sigstore certificate. The certificate records the
GitHub Actions identity. For public repositories, GitHub uses the Sigstore Public
Good Instance and writes the bundle to a public immutable transparency log.
GitHub describes a normal artifact attestation as SLSA v1 Build Level 2.

The release workflow should download the newly published `.crate` archive,
verify its SHA-256 against crates.io, and attest that exact file. Lenso can then
verify the downloaded archive with a policy equivalent to:

```sh
gh attestation verify module-name-1.2.3.crate \
  --repo approved-owner/approved-repository \
  --signer-workflow approved-owner/approved-repository/.github/workflows/release.yml \
  --source-digest <trusted-publishing-commit-sha> \
  --deny-self-hosted-runners
```

The verifier must check the SLSA provenance predicate type, subject name and
SHA-256, repository, signer workflow, source commit, OIDC issuer, and hosted
runner policy. Repository-only verification is too broad. The official
`actions/attest` action accepts an exact subject path, digest, or checksums file,
and `gh attestation verify` supports the policy constraints above.

Sources: [GitHub artifact attestation model and SLSA level](https://docs.github.com/en/actions/concepts/security/artifact-attestations),
the official [`actions/attest` inputs](https://github.com/actions/attest#inputs),
and [`gh attestation verify` policy options](https://cli.github.com/manual/gh_attestation_verify).

GitHub warns that an attestation is not itself a safety judgment. In particular,
workflow-controlled predicate fields can be falsified if the workflow execution
context is compromised. A vetted reusable builder can raise the build-provenance
assurance, but the artifact digest, certificate identity, and verified timestamp
remain the facts Lenso can validate directly.

### Immutable GitHub Releases are optional corroboration

GitHub immutable releases lock the release tag to a commit, prevent attached
assets from being changed or deleted, and automatically create a release
attestation covering the tag, commit, and assets. The release-assets API also
exposes SHA-256 digests. If a module publisher mirrors the exact `.crate` file as
an immutable Release asset, Lenso can verify it with `gh release verify` and
`gh release verify-asset`.

Sources: [GitHub immutable releases](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases),
[release integrity verification](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/secure-your-dependencies/verify-release-integrity),
and [release asset digest API](https://docs.github.com/en/rest/releases/assets).

This should be optional in V1. crates.io is already the canonical Linked package
store, and requiring a duplicate GitHub asset adds another publication step
without strengthening the crate-to-source claim beyond the required artifact
attestation. Auto-generated GitHub source archives also cannot be checked with
`gh release verify-asset`; only uploaded release assets can.

## What the crate archive can and cannot prove about source

`cargo package` normally adds `.cargo_vcs_info.json` with the VCS commit, dirty
flag, and repository-relative package path. Lenso can inspect the downloaded
archive and require:

- package name and version match the Module Release exactly;
- the normalized and original Cargo manifests identify the expected crate;
- `.cargo_vcs_info.json` exists, has `dirty: false`, and its full commit and
  `path_in_vcs` agree with Trusted Publishing and catalog data;
- the declared repository URL agrees with the approved repository;
- dependency declarations, features, targets, build script, license files, and
  console package metadata pass catalog policy.

However, Cargo explicitly calls `.cargo_vcs_info.json` a best-effort snapshot and
says the package provenance is not verified: there is no guarantee that the
tarball's source matches the stated VCS information. `Cargo.toml` repository
metadata and Git tags are publisher-controlled claims with the same limitation.

Source: [`cargo package` archive behavior and provenance warning](https://doc.rust-lang.org/cargo/commands/cargo-package.html#cargo_vcs_infojson-format).

Consequently, Lenso may label this evidence `source_claim_matches`, but must not
label it `source_reproduced` or `source_equivalent`.

## Independent Lenso build evidence

Official compatibility is a separate first-party claim made by Lenso's catalog
verification pipeline. For each accepted release, that pipeline should start
from the verified crates.io archive and record a signed, immutable receipt with:

- the crate SHA-256 and Module Release identity;
- exact Lenso Starter and Lenso versions;
- pinned Rust/Cargo toolchain, builder image digest, host and target triples;
- requested features and the final generated application `Cargo.lock` digest;
- build, migration, startup, architecture/contract, smoke, and optional Runtime
  Console compatibility commands and outcomes;
- logs or log digests, workflow identity, commit, timestamps, and receipt digest.

The check should fetch dependencies first and then use Cargo's locked/offline
mode where practical. `--locked` prevents dependency resolution from changing
the lockfile; `--frozen` combines locked and offline operation.

Source: [Cargo build locked, offline, and frozen semantics](https://doc.rust-lang.org/cargo/commands/cargo-build.html#manifest-options).

The receipt should live in the reviewed Git catalog and may itself receive a
GitHub artifact attestation. Ephemeral Actions logs are supporting detail, not
the durable receipt.

This evidence proves only the tested matrix. A library crate's version ranges do
not lock the dependency graph selected when it is linked into an application;
the application's final `Cargo.lock` is the install-specific dependency record.
The final Host binary is also a different artifact and belongs to the
application's own build/deployment provenance, outside Module Ecosystem V1.

## Guarantees unavailable without more infrastructure

| Desired guarantee | Why current evidence is insufficient | Additional requirement |
| --- | --- | --- |
| Crate source exactly equals a repository subtree at the claimed commit | Cargo explicitly does not verify `.cargo_vcs_info.json`; repository metadata, tags, and commit signatures do not bind every archive byte | An independent source checkout and deterministic `cargo package` comparison, with pinned Cargo and packaging inputs, or a trusted builder that produces and publishes the exact attested archive |
| Compiled module output is reproducible | The distributed Linked artifact is source; rustc, linker, target libraries, environment, build scripts, and application dependency resolution affect output | Hermetic build definitions, pinned toolchain and system inputs, at least two independent rebuilders, and byte-for-byte comparison of final Host artifacts |
| Module code is safe or non-malicious | Checksums and attestations authenticate bytes and origin, not behavior | Source review, dependency/security policy, malware/static analysis, capability disclosure, and sandboxing where applicable |
| Every feature and target works | A successful receipt covers only the tested matrix | Explicit supported matrix plus CI for every claimed combination |
| Current crate owners are the historical authorized publisher | Owner membership is mutable; token publication and ownership are not source provenance | Catalog-owned publisher identity, review history, and Trusted Publishing evidence captured per version |
| No supply-chain compromise occurred in the release workflow | Trusted Publishing and attestations authenticate the workflow identity, but a compromised allowed workflow can publish bad bytes | Protected repository/environment policy, pinned actions, vetted reusable builder, independent catalog verification, and incident revocation/yank handling |
| A verified module implies a verified deployed application | Linked code is compiled together with the user's Host and dependency graph | Separate application/Host artifact provenance in the user's build and deployment pipeline |

## Required V1 acceptance record

The catalog's normalized record for an official Linked release should contain at
least:

```text
module: immutable publisher/module id and module version
crate: crates.io name, exact version, SHA-256, size, publication time, yank state
publisher: approved publisher id, GitHub owner id, repository id
publish_origin: provider, repository, workflow run id, commit SHA
source_claim: repository, commit SHA, path_in_vcs, dirty=false
attestation: subject SHA-256, Sigstore bundle/URL, signer workflow, source digest
verification: tested Lenso versions, builder/toolchain, lock digest, checks, receipt digest
observations: current crate owners and any drift, deprecation/security state
```

Acceptance must fail closed when the archive checksum, Trusted Publishing
repository/commit, attestation subject, approved signer workflow, source claim,
or independent compatibility receipt disagree. Yank and ownership are live drift
signals and should be re-queried before install or upgrade; historical evidence
must remain preserved even when live state changes.

## Bottom line

Lenso can automatically establish a strong and useful chain from an approved
publisher, repository, workflow, and commit to the exact immutable crates.io
archive, then add its own tested-compatibility receipt. It should call that
`verified provenance`, not `verified source equivalence`, `reproducible build`,
or `safe module`. Those stronger statements require infrastructure beyond
crates.io metadata, GitHub repository state, and a single CI build.
