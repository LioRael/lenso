# Configurable Engine Web example

The owner is one normal source-declared Plugin. `build.rs` selects the optional
Engine Web processor with `endpoints/`, excludes `endpoints/private/`, and lowers
two attributed handlers through the official Endpoint macro. No source is read
at runtime. Change the options or supply a Snapshot to replace default discovery.

```sh
cargo test --locked -p lenso-engine-web-example
cargo run --locked -p lenso-engine-web-example
curl http://127.0.0.1:18087/health
curl http://127.0.0.1:18087/items/example
```

The first request returns `"ok"`; the second returns the extracted ID `"example"`.
The excluded file contains an intentional compile error. A duplicate route ID,
parameter shape or invalid path fails before the generated bindings are built.
