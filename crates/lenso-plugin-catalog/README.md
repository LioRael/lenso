# Lenso Plugin catalog protocol

The shared signed catalog protocol for native CLI and Marketplace Workers consumers.
The default feature set supports `wasm32-unknown-unknown` and contains no CLI,
network client, filesystem installer, or runtime adapter dependencies.

`Snapshot`, `Release`, `Envelope`, `Trust`, `Checkpoint`, and `VerifiedSnapshot`
retain the `lenso.marketplace.snapshot.v1` wire format. `sign` and `verify` use
the existing domain-separated Ed25519 protocol and exact decoded payload bytes.
Callers supply trusted keys, current time, and the last durable checkpoint.
Verification preserves freshness, rollback, equivocation, and immutable release
identity checks; it never grants installation or execution authority.

For display-only callers, `verify_for_browse` returns a `BrowseSnapshot` that
permits expired metadata while retaining signature, schema, identity, future-issue,
rollback, and equivocation checks. It exposes expiry for a visible stale notice
and has no installation selection API. `verify` and `VerifiedSnapshot::select`
continue to reject expired catalogs; browsing must never substitute for them.

The optional `bundle-verification` feature adds the native
`Release::verify_bundle_directory` compatibility method using the framework's
Bundle verifier. It is enabled by `lenso-cli`; Workers consumers must leave it
disabled. Download, extraction, origin policy, installation, and persistence
remain caller responsibilities.

Canonical Plugin identity and exact release-version validators are exposed in
`identity` and reexported by the CLI's existing `identity` module. Existing CLI
catalog imports continue through `lenso_app_authoring::signed_plugin_catalog`.

`ReleaseDetailsSnapshot` is an additive, separately signed document keyed by an
exact existing `plugin_id@version`. It describes portable Bundles, exact
Cargo/npm packages, target filters, and digest-bound Markdown documentation
without changing the v1 `Snapshot` or `Release` API. Its distinct signature
context and checkpoint prevent replay as a base catalog. Details remain data;
the Host still owns compatibility, permission, trust and installation admission.
Release details still require their exact Portable base; they cannot represent
an npm-only release.

`package` is a separate signed package-only base channel. Its snapshot can
publish an npm Plugin release without a fictitious Portable artifact, binding
the logical Plugin identity to exact npm package names, versions, registry
references, and SHA-256 archive digests. It uses a distinct signature context
and checkpoint; old Portable, release-details, and linked Cargo payloads are
unchanged. Verification selects metadata only. Market publication must enforce
identity uniqueness across channels, and an App client must separately verify
archive bytes and use its package manager's lock with lifecycle scripts disabled.

`linked_cargo` is a separate signed source-only channel. It describes an exact
Cargo crate archive checksum, target list and integration kind for a Host-linked Plugin, with
append-only documentation revisions. It never claims the crate is a portable
Bundle or a loadable runtime artifact. The old v1 snapshot and details wire
formats remain unchanged. A consumer must independently verify the registry
bytes, build the Host, and check the resulting linked Plugin identity before
admission; signature verification alone does not perform those steps.
`linked_plugin` identifies a generic linked entrypoint; `host_provided` requires
product-specific Host integration and must not be advertised as generically
adoptable.

`release_content` is a separately signed v2 channel for optional editable
templates and development extensions under the same exact Plugin ID and version.
It does not add fields to either v1 signed payload, so existing v1 signatures
and readers remain unchanged. Each v2 entry binds the immutable identity of an
exact listed Portable or linked Cargo base release. Content references carry an
HTTPS URL, SHA-256 digest and byte size for a bounded `.tar.gz` source tree.
The URL is only a reference: verification never fetches, copies, selects or
executes it. A consumer must verify both signed snapshots, their independent
checkpoints, the selected base identity and the received archive bytes before
copying. Content identity is immutable across revisions; publication of a new
version is required to change it.

This new package is prepared for local review. It has not been published; registry
release requires the repository's Trusted Publisher workflow and explicit approval.
