# Engine authoring boundary

Read this for generic convention processing, discovery, authoring sessions or
embedding the DX layer in a non-App tool. Engine is a separate sibling repository
`lenso-engine`, with Rust packages under `crates/`. CLI is a consumer.

The core plans immutable inputs and selected processors, tracks dependencies and
sessions, and publishes resources. App composition lives in optional
`lenso-engine-app`; language and filename semantics belong to selected support.
Do not move a CLI compiler switch or domain payload schema into the core.

Read Engine README and its processor examples. `engine inspect/run/dev --source
./content --markdown` uses optional reading support without an App or Rust
toolchain. For custom workflows, `engine.json` identifies local source roots,
`plugin_sources`, explicitly selected plugins and presets. Sources discover
`engine-plugin.json`; discovery alone grants no activation.

Generic processors implement the versioned stdin/stdout JSON protocol and may
read, parse, compile, transform or index. They are trusted authoring tools, not a
new runtime Execution Class. Existing generated processor Capability lowering
owns execution. Bootstrap uses already executable artifacts, so the convention
that interprets source is not needed to compile its own discovery bootstrap.

Workflow `engine lock` hashes existing tools/artifacts/configuration without
installing or compiling. Execution verifies that host-local lock. Edited source
documents remain editable; tool/config changes require explicit re-locking.
Multiple processors may consume one input with explicit dependency ordering.

Verify through owner workspace tests and a real consumer: data read without
compilation, output publication, selected/disabled behavior, changed artifact
rejection and failure preserving the previous published generation where
supported. Validate CLI and independent embedding separately when affected.
