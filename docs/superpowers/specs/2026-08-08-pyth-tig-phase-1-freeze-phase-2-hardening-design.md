# PythTIG Phase 1 Freeze and Phase 2 Hardening Design

**Status:** Owner-approved on 2026-08-08

## Purpose

Freeze the PythTIG version 1 package ABI using the completed Phase 1 evidence,
then make the existing Phase 2 ring-3 runtime branch safe to review and merge
without changing the default object-shell boot path or implementing Phase 3
authority.

## Scope

This work may:

- record the existing Phase 1 record layouts, numeric IDs, opcode IDs, limits,
  verifier errors, version behavior, canonicalization, and checksum behavior as
  the stable PythTIG v1 ABI;
- isolate Phase 2 boot selection and bundle packaging behind explicit test-only
  opt-in controls;
- reject a package before ring-3 entry when it uses an opcode outside the
  Phase 2 execution profile;
- preserve and strengthen the success, invalid-package, budget-exhaustion, and
  contained-fault QEMU acceptance cases; and
- bring the Phase 2 branch onto the current ADR 0066 compatibility baseline.

This work must not implement Phase 3 object/task/capability host operations,
change the frozen package bytes, cut over the default object shell, expose test
control through production boot, or begin compiler, Task Steward, native
backend, networking, package-management, AI, or additional hardware work.

## Considered Approaches

### 1. Implement every verifier-known opcode in Phase 2

This would make verifier admission and runtime execution identical, but it
would pull Phase 3 host authority and later object/task semantics into the
current milestone. It violates the phase boundary and is rejected.

### 2. Shrink the version 1 ABI to the currently implemented interpreter subset

This would remove already tested IDs and signatures immediately before the
format freeze. It would also force a byte-level redesign rather than repairing
the runtime boundary. It is rejected.

### 3. Freeze the shared v1 ABI and add a Phase 2 execution profile

This is the selected approach. The shared verifier continues to validate the
complete frozen v1 graph contract. Before mapping a package or creating a user
address space, PythCore additionally verifies that every node belongs to the
bounded Phase 2 runtime profile. Unsupported-but-well-formed v1 packages are
rejected with a deterministic marker and receive no ring-3 authority.

## Boot and Packaging Boundary

Normal production builds retain the existing object shell as the only default
normal-boot path. They do not read, clear, or interpret sector 96 as PythTIG
control, and their default `INIT.PAK` does not depend on PythTIG runtime or graph
artifacts.

The Phase 2 acceptance harness opts in twice:

1. it builds PythCore with a dedicated test-only PythTIG launch feature; and
2. it invokes image packaging with an explicit PythTIG bundle flag.

The opt-in packager requires every runtime and graph artifact explicitly. The
default packager ignores stale PythTIG artifacts and preserves its existing
shell-only caller contract.

## Admission and Execution Boundary

The shared verifier remains the canonical architecture-independent format and
semantic verifier. A separate Phase 2 execution-profile check runs in PythCore
after shared verification and before package mapping, address-space creation,
bootstrap construction, or ring-3 entry.

The Phase 2 profile admits only the opcodes whose semantics are implemented by
the reference runtime in this milestone. Future host-operation opcodes retain
their frozen numeric identities but are rejected by the Phase 2 profile. This
is a compatibility boundary, not a claim that those future operations work.

The ring-3 interpreter remains defensive: an impossible unsupported opcode
still returns its typed runtime error if the kernel boundary is violated.

## Failure Handling

- Invalid shared-verifier packages emit one deterministic package-rejection
  marker before ring-3 entry.
- Valid v1 packages outside the Phase 2 execution profile emit one deterministic
  unsupported-opcode rejection marker before ring-3 entry.
- Budget exhaustion produces the existing typed budget exit.
- Runtime invalid-instruction faults terminate only the graph process and leave
  the permitted peer alive.
- The default object shell remains reachable when no test-only launch mode is
  compiled and selected.

## Verification

The completed slice requires fresh evidence for:

- Phase 1 canonical format and mutation acceptance;
- shared ABI/verifier tests and the ring-3 interpreter tests;
- clean default image and ISO packaging without PythTIG artifacts;
- explicit opt-in image and ISO packaging with required artifacts;
- interface compatibility and boot-marker freezes from current `main`;
- default normal fast boot and object-shell boot;
- Phase 2 success, invalid-package rejection, unsupported-profile rejection,
  budget exhaustion, and contained runtime fault QEMU boots; and
- the complete existing milestone QEMU acceptance path.

COM1 serial output remains the boot oracle. Compile success or a screenshot is
not completion evidence.

## Completion Boundary

After the Phase 2 verification matrix passes, halt and report. Do not implement
or begin Phase 3.
