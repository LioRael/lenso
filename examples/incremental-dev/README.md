# Incremental development

Two normal Rust Plugins in separate Cargo packages expose `/greeting` and
`/health`. Their Instances and configuration live in `plugins/`.

From the repository root:

```sh
cargo build --locked -p lenso-cli
cargo generate-lockfile --manifest-path examples/incremental-dev/Cargo.toml
cargo generate-lockfile --manifest-path examples/incremental-dev/app/health/Cargo.toml
target/debug/lenso app dev --root examples/incremental-dev
```

Use the printed HTTP address. Edit the Greeting message in
`plugins/example.greeting/default.toml`: the current CLI reuses locked execution
artifacts, re-resolves the configuration, checks readiness and starts a fresh
Host generation. Invalid configuration retains the previous preview. Adding an
Instance, changing dependency selection, resources, source or a selected
compiler uses the normal build path.

Edit Greeting's implementation: Cargo recompiles that package and the necessary
Host link. Health remains cached. This existing Cargo behavior is preserved.

For repeatable edit-to-HTTP evidence, run:

```sh
python3 crates/lenso-engine-app/tests/dev-feedback-smoke.py \
  --cli target/debug/lenso --root examples/incremental-dev --rust
```

The probe waits for the watcher to confirm activation, checks real HTTP output,
and records unaffected artifact hashes and timestamps. It restores authored
files after stopping dev. Initial startup and warm edits are reported separately.
`project_dev_feedback` on the existing `lenso mcp` entry reports the last dev
classification and outcome; generated snapshots are never authoring inputs.
