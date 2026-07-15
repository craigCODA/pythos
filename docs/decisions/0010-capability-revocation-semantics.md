# ADR 0010: Capability Revocation Semantics

Date: 2026-07-15

## Status

Accepted

## Context

Phase 3 must support revoking a specific capability handle without revoking a
service's unrelated handles and without requiring cooperation from the holder.
The rule has to be defined before `service-identity` and the later capability
slices begin depending on it.

## Decision

Revocation is slot-specific and generation-based.

When PythCore revokes a capability:

```text
entry.state = Revoked
entry.generation += 1
```

After revocation, validation of any stale handle to the old generation fails.
The same table slot may later be reused for a new grant, but only with the new
generation. Revocation does not mutate or invalidate the holder's other handles.

Revocation checks are mandatory at every capability use site. A service that
knows the target resource and operation name but lacks an active matching handle
must be denied.

Phase 3 revocation does not implement distributed revocation, delegation trees,
lease expiry, persistence, or cross-machine invalidation.

## Consequences

The negative-authorization proof can distinguish "no handle," "stale revoked
handle," and "wrong rights" as denial cases. This remains local kernel-mode
enforcement until Phase 8 moves services behind a hardware-enforced syscall
boundary.
