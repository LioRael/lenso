# Portable Rust Agent Tool

Use this path only for the ordinary Agent Tool shape shipped by the current
`lenso plugin` CLI. Inspect `lenso plugin new --help` and the generated package
before relying on an option or file.

```sh
lenso plugin new company.uppercase
cd company.uppercase
lenso plugin check
lenso plugin dev --operation execute \
  --request-json '{"name":"company.uppercase","arguments_json":"{\"text\":\"hello\"}"}'
# Only when delivering a portable archive:
lenso plugin pack
```

The default path builds one trusted Process implementation. Select
`--runtime multi` only when both Wasm and Process are needed; it produces a
V4 `.lenso-plugin` Release with both implementations of one Plugin Contract.
`--runtime wasm` and `--runtime process` select one output when current help
confirms them. The generated source uses `lenso-plugin-sdk::AgentTool` and
`export_agent_tool!`.

This scaffold is not a universal generator for arbitrary Capability providers,
stateful Plugins, Bun, Web UI, or every interaction kind. Route a different
shape to its owning SDK rather than reshaping it to fit this template.

`check` validates generated descriptor evidence in a temporary Bundle. `dev`
must cross the real selected Adapter. `pack` builds, validates, and reopens the
exact Bundle it writes; a receiving Host validates it again during `plugins
add`.

This path is complete when the selected implementation's real `dev` invocation
preserves changed success and rejection behavior. Archive delivery additionally
requires reopening the packed bytes; multiple published implementations need
the affected common Contract vectors without runtime fallback.
