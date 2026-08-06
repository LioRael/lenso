# M6 Security and Repository Release Guidance

Version: `m6.v1`.

## Security gate

Use `docs/security/m6-threat-model.md` and
`lenso.security-review-evidence.v1`. Bind every threat model, finding, scan,
disposition, accepted risk, SBOM, provenance, source commit, and artifact
digest to the exact GA Support Manifest.

Open critical or high findings block release. Accepted risk requires a named
approver, reason, future expiry, and the exact finding digest. Public evidence
contains no credentials, private keys, raw tokens, backup bytes, or sensitive
finding material. Stale or future-dated reviews block.

## Reviewed release

Follow the repository-local process in `docs/release-process.md`. Release-plz
owns Cargo release pull requests and crates.io Trusted Publishing; Changesets
owns npm version pull requests and npm Trusted Publishing. The pull request
binds the exact source commit, package versions, dependency closure, and any
new changelog entry produced by its ecosystem tool.

Do not introduce a central coordinator, shadow registry, reusable release nonce,
or long-lived registry credential. A failed publication is repaired from the
registry's immutable state in the same repository workflow; it is never
reconciled by republishing an existing version.

Record public archive checksums, provenance URLs, and fresh-install evidence
after publication. Evidence must contain no credentials, private keys, tokens,
or mutable authorization state.
