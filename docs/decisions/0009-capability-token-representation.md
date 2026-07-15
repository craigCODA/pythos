# ADR 0009: Capability Token Representation

Date: 2026-07-15

## Status

Accepted

## Context

Phase 3 introduces local, in-kernel capability handles. The representation must
make forgery and stale-handle reuse detectable inside PythCore, while honestly
reflecting that all code still runs in kernel mode until Phase 8.

## Decision

Capabilities are represented by kernel-owned table entries. A handle is an
opaque reference to that table entry:

```text
CapabilityHandle {
    slot: u32,
    generation: u32,
}
```

The authority itself is stored only in the kernel table entry:

```text
holder ServiceId
resource ResourceId
rights RightsMask
generation u32
state Active | Revoked
```

Rights are not encoded in the handle value. Every privileged operation resolves
the handle through the kernel table and verifies:

```text
slot is in range
entry generation equals handle generation
entry state is Active
entry holder matches the calling service
entry rights include the requested operation
entry resource matches the target resource
```

Phase 3 handles are local to one boot and one kernel instance. Persistence,
cross-boot capabilities, cryptographic tokens, and network-scoped authority are
not part of this phase.

## Consequences

Unforgeability in Phase 3 is logical and kernel-mediated: code without a valid
table entry cannot pass validation merely by knowing a resource name or
operation. This does not yet defend against hostile kernel-mode code; Phase 8
must revalidate the same contract at the syscall boundary.
