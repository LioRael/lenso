# Historical Design Records

The plans and specifications in this directory record decisions and delivery
work at the time they were written. They are not the current architecture
contract.

Before implementing from one of these records, reconcile it with:

- `docs/architecture/overview.md`;
- `docs/architecture/rules.md`;
- `docs/architecture/service-module-boundary.md`;
- `docs/architecture/third-party-modules.md`.

In particular, references to Remote Modules, remote Module sources, or
`@lenso/remote-module-kit` are historical. The current model has Linked Modules
only; independently running code is a Service. Future Wasm support, if approved,
will be a distinct Module source and will not restore the retired remote-process
category.
