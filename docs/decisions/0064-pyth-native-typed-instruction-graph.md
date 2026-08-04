# ADR 0064: Pyth Native Typed Instruction Graph

Date: 2026-08-04
Status: Accepted

## Context

PythOS currently proves a bounded custom-minimal runtime, a capability-controlled
ring-3 object shell, typed object persistence, hardware-backed syscall
boundaries, and target-limited physical evidence paths. The next execution-model
proposal must not inherit CPython, POSIX, or a conventional application runtime
merely for familiarity.

The original PythTIG handoff proposed ADR 0053 for this decision. The live
repository already uses ADR 0053 for the interactive object-shell launcher, so
this proposal is renumbered to ADR 0064.

## Decision

PythOS should define PythTIG: a canonical typed instruction graph executed in
ring 3. Graph values contain typed data and capability handles, never raw
pointers. Pure dependencies are explicit node references. Control is explicit
through blocks and terminators. Side effects are ordered by one validated
`Effect` token chain. Capability values originate only from validated imports or
capability-returning host operations.

PythCore validates package structure, maps verified packages read-only, supplies
bounded capabilities, enforces typed syscalls, and contains faults. PythCore does
not parse Pyth source, infer semantic intent, run agent policy, or grant
authority from relevance.

The first executor is a ring-3 interpreter and semantic reference
implementation. A later x86-64 backend may lower only verified graphs and must
pass differential acceptance against the interpreter.

The existing Rust object shell and custom-minimal proof runtime remain available
until a later cutover gate passes with fresh evidence.

## Consequences

PythOS owns its language and execution semantics without claiming that CPython
or MicroPython runs as the system substrate.

Universal boot and PythTIG may advance concurrently because graph semantics
contain no hardware-controller instructions. Hardware backends may expose typed
services through PythCore, but they may not change package bytes or graph
semantics.

This ADR accepts the PythTIG architecture direction only. It does not authorize
Phase 1 implementation, runtime launch behavior, compiler work, Task Steward
behavior, native code generation, package management, networking, AI authority,
or default-session cutover. ADR 0065 remains provisionally accepted for Phase 1
format experimentation; exact package bytes are not permanent stable ABI until
the first encoder, decoder, verifier, and negative corpus pass and the owner
explicitly freezes the format.

## References

- `docs/superpowers/specs/2026-08-03-pyth-typed-instruction-graph-design.md`
- `docs/superpowers/plans/2026-08-03-pyth-typed-instruction-graph-master-plan.md`
- `docs/pyth-tig/PHASE-0-RECONCILIATION-REPORT.md`
