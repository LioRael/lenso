# Plugin Root

The only App-owned app configuration and configuration surface is:

```text
plugins/
  <plugin-id>/
    plugin.lenso-plugin/   # external package only
    <instance>.toml
    <instance>.disabled   # present only while disabled
```

Built-in Plugins omit the package directory because the Host Catalog already
owns their implementation. A missing or empty `plugins/` selects Host defaults.
Directory names and TOML files are strict, UTF-8, deterministic inputs; unknown
entries and invalid configuration fail closed.

Normal commands are `lenso plugins list|add|configure|disable|enable|remove`.
There is no central enabled list, binding file, sidecar format, or App Definition.
`pack` validates created Plugin bytes and `plugins add` validates received bytes,
so there is no separate `plugin verify` step.
