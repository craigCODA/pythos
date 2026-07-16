# ADR 0016: Phase 4 system.* API Surface

Date: 2026-07-16

## Status

Accepted

## Context

Phase 4 requires a deliberately narrow `system.*` API. Every function on this
surface must validate a capability before doing privileged work. The
`interpreter-boot` slice already parses `system.log("PythOS [HISS] We Are Woken")` into
an internal operation, but it deliberately did not execute the host call.

This ADR records the complete `system.*` surface for the current slice before
implementation expands beyond a stub.

## Decision

The initial Phase 4 `system.*` surface contains exactly one function:

```text
system.log(message)
```

For this slice:

* `message` is the already parsed string literal from the custom-minimal
  interpreter plan.
* The host rejects empty messages.
* The host rejects messages longer than 128 bytes.
* The host checks a `LOG` capability for the caller's service identity before
  emitting the log marker.
* A task that knows the log operation but lacks the capability is denied.
* Successful execution emits `PYTHOS:CORE:SYSTEM:LOG`.
* The slice completes with `PYTHOS:CORE:SYSTEM_API_READY`.

No other `system.*` functions exist yet. `self.ready()` is not part of the
`system.*` API; it belongs to later service lifecycle slices.

## Deferred

The following remain outside this slice:

* general Python value conversion
* native/Python ownership transfer rules
* exceptions raised through `system.*`
* service readiness transitions
* service manager lifecycle policy
* async event delivery
* user-mode syscall or IPC transport for `system.*`

## Consequences

The first Python-shaped host call now has the correct authority shape: service
identity plus explicit capability, rather than ambient access. The value surface
is still intentionally tiny. Later Phase 4 slices must extend this ADR or add a
new ADR before adding new `system.*` functions or generalizing argument/value
handling.

ADR 0017 defines the follow-up value-validation contract for the current
`system.log` argument without adding new `system.*` functions.
