---
status: accepted
---

# Bound automatic recovery to managed-resource retirement

Automatic recovery after a supervised App Generation stops guarantees retirement
of framework-managed tasks, lifecycle work, and resources, including
Execution-Adapter-owned child processes. It does not guarantee cleanup of
arbitrary descendants deliberately spawned or escaped by trusted Native or Bun
code. This extends [ADR 0047](0047-scope-runtime-work-to-module-lifecycles.md)
and [ADR 0048](0048-make-supervision-execution-adapter-aware.md) to the Host
supervisor boundary: requiring proof about every possible trusted-code
descendant would make ordinary stop, restart, and update permanently manual
without providing an actual isolation boundary.

## Decision

The generated Host may acknowledge retirement only after Kernel shutdown is
`Clean`, binding guards are released, and its local task set and runtime are
dropped. Each selected Execution Adapter must truthfully complete its managed
cleanup, including acknowledgement and reaping of owned child processes where
applicable. Unsupported profiles cannot inherit this guarantee from another
Adapter. Cleanup evidence covers the entire App lifetime, not only the last
generation. Optional Plugin failure may leave the App available, but a failed
managed cleanup must prevent a later `Clean` shutdown; replacement cannot erase
that evidence.

Confirmed death and reaping of a crashed process may permit configured Plugin
supervision to recreate that generation. This is not a normal Host retirement:
the Adapter's lifetime evidence remains unclean after the crash, even when the
replacement succeeds. Unconfirmed termination and failed stop hooks still fail
cleanup; the Kernel's cleanup-failure record is not bypassed.

The supervisor supplies a private per-generation token through the Host
environment. It clears the durable crash fence only after receiving the matching
retirement receipt, observing successful Host exit, and confirming that the
original process group has no live members. A missing or wrong token, abnormal
exit, timeout, forced cleanup, or supervisor crash retains the fence. Neither
exit status zero nor process-group disappearance alone establishes retirement.
Kernel remains portable; the Host, Runtime Driver, Execution Adapters, and
supervisor own these mechanics.

The token correlates a receipt with one launch; it does not authenticate that
receipt against trusted code able to inspect or forge it. Process-group checks
are not cgroup containment, a security sandbox, or proof that deliberately
escaped descendants are gone. Persistent external effects are not rolled back.

## Adoption

Adoption begins with File/HTTPS configuration sources in supervised `app start`
and local `app dev` without a separate frontend process.
A bootstrap Configuration Source Plugin has a separate owner lifetime and keeps
manual crash-fence recovery until that owner implements retirement. Windows and
frontend-process automatic retirement are not covered by this adoption. Exact supported
Adapter profiles require owner-backed implementation and tests; this decision
does not qualify a target or expand its admission matrix.
