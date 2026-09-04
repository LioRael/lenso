# Plugin failure scope after readiness

Status: **Design approved on 2026-09-04; ADR 0074 is authoritative, implementation pending.**
Date: 2026-09-04.

This is the fault-scope decision companion to the
[consolidated authoring review](2026-09-04-plugin-usage-walkthrough.md).
[ADR 0074](../adr/0074-scope-terminal-failure-to-host-essential-instances.md)
records the accepted amendment to ADRs 0032, 0046, and 0048.
It is independent of named dependency syntax and needs no new Plugin category.

## Accepted decision

Keep strict initial startup. After readiness, distinguish dependencies required
by one consumer from instances necessary for the App's minimum useful operation.
The product Host declares the latter, and resolution derives their transitive
required dependencies. Exhausted runtime recovery is terminal for that set;
outside it, the failed instance remains unavailable and unrelated work continues.

This deliberately excludes partial startup. An App with a selected Plugin that
cannot prepare or activate still fails startup and cleans up under ADR 0046.
An owner can disable an optional feature and start a newly validated composition.
Kernel does not silently drop desired instances to make boot succeed.

## Current behavior and actual change

The current Plan's `plugin_instance_is_required` treats a provider as required
when any consumer binds it through a `one` requirement. Supervision uses that
fact together with explicit criticality when determining terminal failure.

Consider an App whose main function is an editor:

```text
editor -> document store
statistics -> metrics store
```

Both arrows are required dependencies. Under the accepted target the Host marks the
editor essential, so the document store is essential transitively. Statistics
still requires its metrics store; that relationship alone does not make either
one essential to the whole App. If the editor also requires statistics, the
transitive closure includes both statistics and its store automatically.

Today, the metrics store can cause terminal App failure solely because
statistics has a required binding to it. Changing that outcome is the substantive
architecture change. Existing bounded restart, stable handles, no fallback, and
no replay remain unchanged.

## Rules

1. **Host-owned importance.** The product Host declares which selected logical
   instances are essential. This is composition policy, not an author-level
   `critical` annotation or a user-editable way to bypass required authorization.
   Retain existing explicit criticality conservatively during migration. Do not
   add separate ranks, priorities, or reliability classes.
2. **Derive the required closure.** Start with the Host's essential instances and
   traverse resolved `one` requirements from consumer to provider until no new
   instance is added. Include shared and transitive providers. Optional bindings
   and zero-or-more collections do not imply indispensable membership. If a
   particular member is indispensable, the Host must declare that instance
   essential or the consumer must express an actual required dependency. There
   is no implicit quorum, redundant-provider inference, or fallback.
3. **Resolve before execution.** Materialize the effective terminal-failure
   policy into a complete immutable Plan. Invalid Host declarations fail
   validation; absence of declarations must not silently opt an existing Host
   into weaker behavior. Kernel executes the policy without interpreting which
   product features matter. Supported composition changes recompute the closure
   through the existing Plan Transition or Generation path.
4. **Keep bounded recovery.** A genuine runtime failure makes the provider
   unavailable and uses its existing finite restart policy. During supported
   recovery, consumers retain their objects and stable handles. No in-flight
   request, stream, or event is automatically replayed. `never` or unsupported
   recreation reaches the terminal decision without inventing another attempt.
5. **Contain exhausted failure logically.** An essential instance exhausting
   recovery produces terminal App failure and bounded shutdown. A nonessential
   instance remains unavailable with an explicit cause. Do not automatically
   destroy its consumers or restart them as a group. Their calls to that
   dependency return unavailability; independent operations may still succeed.
   Consumer failure is supervised separately under the same resolved policy.
6. **Report what remains usable.** Initial readiness is not a promise that every
   feature stays available forever. Inspection shows failed/restarting instances
   and affected required dependency paths while preserving the desired graph.
   Do not label a live consumer dead merely because one of its dependencies is
   unavailable, and do not report an unavailable feature as successful. Product
   Hosts translate these facts into their health and user interfaces.
7. **Respect physical isolation.** Native abort, memory faults, a failed shared
   process, or Runner/Driver failure can affect more than one instance. The
   Execution Adapter reports the actual scope. Apply policy to every affected
   instance; logical nonessential status cannot guarantee process survival.

An ordinary domain error, authorization denial, or invocation timeout is not
by itself evidence that an instance needs restarting. Kernel does not infer
business health from arbitrary error results. An optional absent binding also
remains different from a selected provider becoming unavailable.

Disabling, uninstalling, and replacing an instance are desired-state changes,
not supervision events. Reject a candidate that removes a provider still required
by a selected consumer. A Host-permitted edit can disable an optional feature
together with its now-unused provider after complete validation. Recovery does
not undo user disablement. Existing replacement/data-compatibility rules remain.

## Review scenarios

| Situation | Required result |
| --- | --- |
| Statistics cannot activate during initial startup | App never becomes ready; reverse cleanup follows the shared deadline. |
| Metrics store fails after readiness and recreation succeeds | Calls report temporary unavailability; the same binding becomes usable again; no replay. |
| Metrics store exhausts recovery and only statistics needs it | Store stays unavailable; statistics calls needing it fail; editor continues. |
| The whole App restarts while that selected store is still broken | Strict startup applies again; repair it or explicitly disable the optional feature in a valid composition. |
| Document store exhausts recovery and the essential editor requires it | App reports terminal failure and shuts down within its cleanup budget. |
| A shared store serves both editor and statistics | It is essential through the editor path; do not treat it as isolated to statistics. |
| An optional notification target fails | It stays bound but unavailable; no automatic unbinding or alternative account. |
| A native statistics plugin aborts the containing process | No claim that the editor survives, regardless of logical importance. |
| User removes metrics store while statistics still requires it | Reject the composition edit; runtime degradation is not permission to publish an invalid graph. |

## Adoption and ownership

Existing Plans retain their current semantics. Opting into the new policy
requires a Host declaration and a reviewable explanation of which instances
would cease to cause terminal failure. Existing explicit critical instances
remain essential unless the Host deliberately changes that policy. Merely
upgrading the SDK or adding requirement names must not change fault scope.

Allocate executable format/profile changes against the implementation baseline.
An older executor may run a lowered Plan only when it can preserve the exact
chosen behavior; otherwise reject before activation. It must not silently
substitute the old any-required-consumer rule or discard terminal policy.

Product Hosts own essential-instance selection. Core owns resolved Plan data,
validation, terminal decisions, and structural diagnostics. Drivers and Runners
own host scheduling and shutdown; Execution Adapters own physical failure and
recreation. Product health and recovery of durable effects remain with Plugins.

The accepted decision does not select new attributes, file layouts, status
enums, or a generic health framework. Implementation acceptance
must demonstrate the scenario table with portable supervision conformance and
the actual supported Adapter failure scopes. No runtime tests or implementation
delivery are claimed by this design document.
