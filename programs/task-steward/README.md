# Task Steward

Deterministic PythTIG graph program for Phase 5.

Imports:

- `system.log` with write syntax, lowered to the existing log capability.
- `task.context` with read authority.
- `task.proposal` with create-proposal authority.

The program reads the bounded task context score. Scores below 70 return without
mutating task state. Scores 70 or above read the candidate task id, emit a
proposal through the frozen ADR 0065 `TaskProposalEmit` shape, log creation, and
return.

The low-score path is intentionally side-effect-free because PythTIG v1 has a
single effect chain and no effect-join block parameter for branch-local host
calls. Stable-context acceptance markers belong to the bounded task service and
QEMU harness, not to a second Task Steward branch-side host call.

It does not receive task approval, state-control, object-store, graph-query, or
command authority.
