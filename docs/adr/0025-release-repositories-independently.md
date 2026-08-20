---
status: accepted
---

# Release repositories independently

> **Status: superseded for vNext.** Retained as historical and v0.3.x
> maintenance context. [ADR 0030](0030-rebuild-lenso-as-a-local-first-modular-runtime.md)
> and ADRs 0031 onward are normative for vNext. The repository-local release
> ownership rule remains applicable to the maintained `main` release line.

Lenso repositories publish their own changed packages from a reviewed release PR, using only repository-local dependency relationships to select additional packages. We will retire `lenso-release` as a central authorization and coordination layer because organizational or ecosystem relationships do not justify coupling otherwise independent release cadences; cross-repository compatibility belongs in SemVer and machine-readable contracts, consumer dependency-update pull requests, and integration verification instead.

Cargo workspaces use Release-plz. npm packages use an npm-oriented release PR tool, and OCI artifacts are built and published by their owning repository after its release is approved. A repository may group artifacts only when they form one genuine release unit, such as a native CLI and its npm wrapper. Ordinary releases retain CI, exact-commit publication, Trusted Publishing/OIDC, registry verification, and provenance, while shadow publication, central nonces, receipts, system candidates, and global `stable` or `next` channels are not default release gates. Framework crates, CLI, Console, Modules, Starters, and examples keep independent versions; a tested combination recorded by a lockfile is compatibility evidence rather than a synchronized System Release.

Cargo workspaces specifically use Release-plz, while npm workspaces use Changesets. The Console OCI image follows the Console application version and is built once from its exact release tag; independently versioned npm libraries release only when they change. Repositories migrate one at a time, with each repository atomically disabling its central and legacy publishers when its replacement is enabled. After every repository has migrated, the `lenso-release` repository is made read-only and archived rather than repurposed as another central compatibility service.

Migration starts with `lenso-audit-log-module`, then proceeds through `lenso`, `lenso-auth-module`, `lenso-organization-module`, `lenso-cli`, and `lenso-console`. Existing public versions, tags, and changelogs remain authoritative and are never rewritten or republished. A repository has migrated only after its new release PR path completes a real small release through OIDC, registry and provenance verification succeed, the old publishers and runtime hooks are removed, and its documentation names one release path. Failures are repaired in the new workflow using registry state as truth; the central coordinator is not re-enabled, and any one-use manual break-glass publication requires explicit approval.

After the final repository migrates, the coordinator stops accepting plans and must have no pending or in-flight release before retirement. Static indexes of historical plans, versions, commits, artifact digests, receipts, attestation locations, final states, and export checksums are retained permanently without credentials, nonces, secrets, or reusable authorization. Coordinator permissions, GitHub App installations, repository variables and secrets, and recovery workflows are then revoked or disabled before `lenso-release` is archived read-only. Live D1, R2, and Shadow Gateway state remains read-only for 90 days and is deleted with its remaining secrets after the export is rechecked.
