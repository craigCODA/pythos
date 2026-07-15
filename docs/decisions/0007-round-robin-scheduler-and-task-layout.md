# ADR 0007: Round-Robin Scheduler And Fixed Task Layout

Date: 2026-07-15

## Status

Accepted

## Context

Phase 2 needs native task scheduling before IPC, capabilities, Python, or any
service runtime exists. The preceding slices established fixed task records,
guarded stack proof, and a cooperative context-switch primitive. The scheduler
slice must choose the first scheduling algorithm without pulling in later
features such as priority scheduling, idle-task behavior, preemption, task
termination, dynamic spawning, or untrusted inputs.

The task-control data layout also becomes a dependency for later Phase 2
slices and for Phase 3 service identity work. It must remain simple enough to
verify in QEMU serial output and host tests.

## Decision

PythCore starts with a fixed-size, statically defined task model:

```text
TaskId
TaskState
SavedTaskFrame
TaskControlBlock
TaskTable[MAX_TASKS]
```

The first scheduler is round-robin over ready tasks only. It keeps a cursor
into a fixed ready set and returns the next ready task in cyclic order. There
is no priority field, weight, dynamic task creation, idle-task fallback,
preemption policy, or task-termination behavior in this slice.

The scheduler acceptance proof runs two statically defined native contexts on
separate stacks. The scheduler selects them in this order:

```text
TASK_A
TASK_B
TASK_A
TASK_B
```

Each selected task runs by using the existing context-switch primitive, emits
its serial marker, yields back to the bootstrap scheduler loop, and is selected
again by the round-robin cursor. The slice emits:

```text
PYTHOS:CORE:SCHEDULER:TASK_A
PYTHOS:CORE:SCHEDULER:TASK_B
PYTHOS:CORE:SCHEDULER:TASK_A
PYTHOS:CORE:SCHEDULER:TASK_B
PYTHOS:CORE:SCHEDULER_READY
```

## Consequences

The first scheduler is intentionally deterministic and cooperative. It proves
policy selection plus context-switch integration, but it does not claim
preemption, idle behavior, task exit, resource reclamation, or hostile-code
isolation.

Priority scheduling is explicitly deferred. Any future priority or fairness
policy must be introduced by a later ADR and its own acceptance test rather
than silently extending this slice.
