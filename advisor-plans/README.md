# Deep improvement implementation plans

Generated from the 2026-08-30 deep audit and executed in the isolated
`codex/deep-improvements-20260830` worktree. This repository records the Kernel
ownership decision; the concrete fix and validation live in the paired
`lenso-runtime-rust` delivery.

## Execution order and status

| Plan | Title | Status |
|---|---|---|
| F06 | Process invocation deadline cleanup | VALIDATED — NO KERNEL CHANGE; IMPLEMENTED IN PROCESS ADAPTER |

## Boundary and review state

The portable Kernel already selects the deadline result and drops the losing
provider future. Concrete pending-request cleanup belongs to the Process
Execution Adapter and is implemented and validated in the paired
`lenso-runtime-rust` worktree. Existing Kernel deadline coverage passed; no
Kernel source or contract change was needed or made.
