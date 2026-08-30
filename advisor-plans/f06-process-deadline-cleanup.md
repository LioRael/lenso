# Process invocation deadline cleanup

Status: VALIDATED — NO KERNEL CHANGE; IMPLEMENTED IN PROCESS ADAPTER

## Outcome

The portable Kernel already drops the provider future on deadline. Request-scoped abandonment cleanup belongs to the concrete Process Execution Adapter and is implemented in the runtime repository plan of the same name; no Kernel contract change is required.

## Implementation

- The portable Kernel contract remains unchanged: it selects the deadline result and drops the losing admitted provider future.
- The Process Execution Adapter owns the concrete abandonment guard. It removes only a still-pending request and retires the child Generation without blocking destructor execution on a pipe write/flush; a response already settled by the reader does not retire a healthy Generation.

## Validation

- Existing Kernel regression `deadline_stops_one_native_call_without_retrying_it` passed and proves the timed-out provider call becomes inactive without retry.
- Runtime Process tests prove unresolved future abandonment retires the child, while dropping an already-settled response preserves it. No new Kernel test or Kernel source change was needed.
