# Represent UI contributions as Capabilities

Custom application pages will be supplied through a portable UI Contribution Capability rather than Console-specific Manifest, Release, Surface, or lifecycle types. A UI consumer binds `many` contribution providers and receives their route, navigation, asset, and declared portable business-Capability requirements through the same App Composition model used elsewhere.

## Consequences

- One Module package may carry backend and browser artifacts behind explicit entrypoints and Capabilities, while a separate UI Module may also consume another Module's portable business Interface.
- Browser code never queries a global Capability Registry. A Browser Adapter exposes generated clients only for requirements declared by the contribution and resolved by App Composition.
- Browser invocation preserves ActorAssertion provenance and the target Module's final authorization rather than introducing ambient Console administrator authority.
- A UI Contribution selected by the App author or an authorized Console operator is trusted application code, just like a native Rust or Bun Module. It may be a local bundle, an installed package, or an explicitly configured remote ESM URL.
- The supported Browser Adapter and generated-client boundary provides deterministic composition and developer ergonomics; it is not presented as a security sandbox for same-realm JavaScript.
- Lenso does not define mandatory UI trust classes, signature admission, digest pinning, or sandbox policy. A mutable remote URL means that the operator continuously trusts its publisher. Installers, deployment policy, or optional Browser Adapter Modules may add review, vendoring, pinning, or isolation without changing Kernel.
- A Console Module may register remote contribution URLs as runtime product data without mutating App Composition. The immutable Composition still limits which generated Capability clients and Operations that catalog can obtain through the supported Interface.
- Removing the UI consumer or every contribution has no effect on Kernel behavior or non-UI Module operation.
