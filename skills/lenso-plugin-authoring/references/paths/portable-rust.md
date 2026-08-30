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
lenso plugin pack
```

The default `--runtime multi` path produces a V3 `.lenso-plugin` Release with
portable Wasm and trusted Process implementations of one Plugin Contract.
`--runtime wasm` and `--runtime process` narrow the output when current help
confirms them. The generated source uses `lenso-plugin-sdk::AgentTool` and
`export_agent_tool!`.

This scaffold is not a universal generator for arbitrary Capability providers,
stateful Plugins, Bun, Web UI, or every interaction kind. Route a different
shape to its owning SDK rather than reshaping it to fit this template.

`check` validates generated descriptor evidence in a temporary Bundle. `dev`
must cross the real selected Adapter. `pack` builds, validates, and reopens the
exact Bundle it writes; a receiving Host validates it again during `plugins
add`.

This path is complete when the supported implementations are explicit, the
real `dev` invocation preserves success and honest failure, the V3 Release
reopens successfully, and every published implementation passes the same
Contract vectors without runtime fallback.
