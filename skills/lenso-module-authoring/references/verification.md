# Module verification

Prove the Module at four layers. Use the owning repository's actual commands
from its manifest, CI, or instructions; examples here name the evidence rather
than caching commands that may drift.

## 1. Package proof

- generated Capability artifacts are fresh;
- the package compiles/typechecks and its focused unit tests exercise business
  rules, input validation, and Domain Errors;
- configuration rejects unknown, missing, malformed, or secret-valued fields
  before readiness; and
- every generated/provider endpoint reports the exact Capability identity,
  Descriptor version, Operations, and interaction kinds selected by the Plan.

## 2. Factory and lifecycle proof

- an unknown entrypoint, package revision mismatch, missing factory, duplicate
  factory, incomplete endpoint set, or unsupported execution class fails before
  activation;
- `prepare` validates/reserves without exposing work or calling ordinary
  dependencies;
- `activate` obtains clients from explicit `ModuleDependencies`, tracks spawned
  work, and keeps ingress behind the App Ready Gate;
- rollback, shutdown, and supervision restart release resources and cancel/join
  managed work; and
- recreation produces a fresh generation while stable handles either route to
  it or report the Adapter's truthful unavailable/failure result.

## 3. Composition and product proof

Resolve an App containing the real package input, keyed Instance, endpoint and
requirement declarations, explicit bindings, configuration Schema, execution
class, and package lock. Exercise the smallest useful behavior through a real
Capability consumer or owned black-box fixture:

- success plus at least one Domain Error;
- unavailable dependency, deadline/cancellation, durable-state failure, or
  another relevant Runtime Failure;
- Web behavior through the HTTP or Browser Adapter boundary;
- Bun behavior through the real child-process Adapter when wire, lifecycle, or
  generated TypeScript changed; and
- stateful behavior through transaction/recovery or honest storage failure.

## 4. Removal proof

Remove the Module package and Composition entry from the test App. Remove or
rebind consumers that declared it as required; leave unrelated Instances
untouched. Resolve and run the remaining App. The proof fails if the concern
leaves a Kernel hook, policy branch, background task, global registry entry,
mandatory storage, or unexplained generated artifact.

## Evidence to return

Record exact commands and outcomes, the Plan/Composition path, the consumer
path exercised, the startup/runtime failure observed, lifecycle cleanup
evidence, and the diff or fixture demonstrating removal. A list of test names
without their contract purpose is incomplete.
