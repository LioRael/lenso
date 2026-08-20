# Lenso documentation map

This repository intentionally carries two documentation scopes during the
vNext transition. Document status is determined by this map and the canonical
context, not by a file's age or directory name.

## Normative vNext documentation

- [`../CONTEXT.md`](../CONTEXT.md) defines canonical vocabulary, invariants,
  routing, and delivery branches.
- [`architecture/lenso-vnext.md`](architecture/lenso-vnext.md) describes the
  agreed runtime shape.
- [`adr/README.md`](adr/README.md) identifies the authoritative ADR range. ADRs
  0030 onward are normative for vNext.
- [`roadmaps/lenso-vnext-validation.md`](roadmaps/lenso-vnext-validation.md)
  defines implementation evidence and sequencing.
- [`architecture/future-directions/distributed-module-runtime.md`](architecture/future-directions/distributed-module-runtime.md)
  records deferred distribution and microservice motivation without expanding
  the v1 Kernel.

Research under `research/lenso-vnext-*` supports decisions but is not itself a
normative contract.

## Maintained legacy documentation

The current workspace and `main` branch still implement the v0.3.x
Service-oriented framework. Its architecture, getting-started, package,
release, operations, security, contract, and runbook documents remain in their
existing paths so maintenance links and current release procedures keep
working. Direct legacy architecture pages and ADRs carry an explicit status
banner.

Use legacy documents only to maintain v0.3.x, understand migration inputs, or
build a deliberately bounded compatibility Adapter. Do not infer vNext Kernel
or Module requirements from them.

## Retirement policy

Legacy documentation is removed only after all of the following are true:

1. issue #603 assigns the behavior a vNext owner, compatibility boundary, or
   retirement decision;
2. no maintained v0.3.x workflow or supported release depends on the document;
3. code, documentation, CI, package, and external-link references are updated;
4. historical ADR evidence remains available; and
5. the deletion is reviewed separately from the implementation that replaces
   the behavior.

ADRs are historical records and are not deleted merely because they are
superseded.
