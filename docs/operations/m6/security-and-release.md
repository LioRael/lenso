# M6 Security and Reviewed Release Runbook

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

Follow the authoritative Lenso release runbook. Prepare the exact component
set, versions, source and release commits, dependency order, changelogs,
digests, generated lock, Policy Evidence, shadow evidence, receipts,
attestations, and production configuration.

The normal coordinator owns protected execution refs and publisher dispatch.
Never manually parameterize a publisher. Changed plan, ref, nonce, digest,
package set, or publisher revision invalidates approval.

Shadow mode verifies exact npm, Cargo, GitHub, provenance, SBOM, receipt, and
attestation bytes without production writes. Receipt recovery may reconcile a
missing receipt but must not republish an immutable version. Remote byte
mismatch is a supply-chain incident.

Production mode, public publication, channel promotion, temporary workflow
changes, and break-glass are named Approval Boundaries. Stop with `ready` or
`blocked` until the user approves the exact repositories, packages, versions,
and plan digest. Restore temporary configuration and verify public bytes,
fresh installation, downstream CI, receipts, attestations, and channel state.
