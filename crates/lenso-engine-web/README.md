# Engine Web support

Optional configurable Rust route processing through `lenso_engine::Plugin`.
Use `build(source_root, out_dir, WebOptions)` from a build script, or register
`WebAuthoring` against your own Snapshot. Default `src/routes/**/*.rs` discovery
can be replaced with roots/exclusions or exact entries with any filename.

Handlers use the existing explicit HTTP route attributes. Output uses the official
Endpoint macro, keeping typed extraction, Plugin registration and runtime behavior
in their existing owners. Method/path/ID/parameter conflicts fail with source
evidence. Filesystem readers and generated destinations reject symlinks and escapes.

See the workspace's `docs/architecture/engine-web-contracts.md` and
`examples/engine-web` for copyable configuration and runtime proof.
