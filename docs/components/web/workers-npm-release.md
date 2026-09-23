# Workers npm releases

Run `release-workers-npm.yml` on `main` with the reviewed package directory,
exact manifest version, and `publish=false` first. The workflow tests and packs
the artifact and reports its SHA-256. Review the source commit and successful
run before dispatching the same source SHA with `publish=true` and that exact
hash as `reviewed_archive_sha256`. A changed
source SHA requires another dry run. Publication uses npm OIDC; no token fallback
is configured. The dry-run job has no OIDC permission; the publication job
checks the exact packed archive digest against the reviewed dry-run hash before
using that permission. Configure
each package's Trusted Publisher for this repository
and `release-workers-npm.yml` (no environment), with direct `npm publish`
explicitly allowed. New npm Trusted Publisher configurations allow staged
publishing by default; this workflow does not call `npm stage publish`.

A first package allocation requires an npm owner bootstrap before Trusted
Publishing is available. Use the reviewed artifact and an owner-authorized
interactive session; never put a long-lived token in this repository. Then
configure Trusted Publishing and verify the exact registry version, integrity,
and provenance. Workflow success alone does not establish consumer adoption.

Versions are explicit in each package manifest. Never overwrite or reuse an
existing version. Publish Runtime before Web's dependent test suite. Consumer
lockfile updates and Workers deployment are separate reviewed changes.
