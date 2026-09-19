# Changesets

Run `pnpm changeset` for every user-facing npm CLI package change. The
Changesets workflow is currently dispatch-only: it builds and inspects the four
platform binaries but does not create a version pull request or publish
`@lenso/cli`. Publishing requires a separately authorized maintainer change.

Configure the npm Trusted Publisher for `@lenso/cli` before any future live
publish. Cargo publication is handled independently by the read-only Release-plz
dry-run workflow.
