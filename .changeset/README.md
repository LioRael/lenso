# Changesets

Run `bun run changeset` from the repository root to record a public package change.
Choose the affected packages, their SemVer bump types, and a useful changelog summary.
Commit the generated Markdown file with the source change.

`bun run release:status` previews the plan. `bun run release:version` consumes the
changesets and updates versions, changelogs, and internal dependency ranges.
After versioning, run `bun install` to update `bun.lock`; review both together.
Do not hand-maintain a second version or dependency propagation algorithm.

Packages are versioned independently. Private workspaces are not versioned or
tagged. Templates and nested CLI example plugins are not root workspaces.
Do not add them to the public release set.

This setup adds no CI, automatic commits, publication, pushes, or Git tags.
Registry, access, ownership, and dist-tag remain unconfirmed. The omitted
Changesets `access` field uses its built-in `restricted` fallback, not an approved
publication policy. No `changeset publish` command is part of this workflow:
it would publish from mutable package directories and create Git tags, rather
than publish the exact archives checked by `release:verify`.

See [local release preparation](../docs/RELEASING.md) for the verification and
separately authorized human publication procedure.
