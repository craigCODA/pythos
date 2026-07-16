# ADR 0017: Runtime Value Validation

Date: 2026-07-16

## Status

Accepted

## Context

ADR 0016 introduced the first Python-shaped host call,
`system.log(message)`, and intentionally left general native/runtime value
handling for the next Phase 4 slice. The custom-minimal interpreter still runs
inside PythCore during this phase, so capability checks are logical kernel
checks rather than a hardware hostile-code boundary.

The value boundary still needs an explicit contract before service lifecycle
work starts. If the interpreter keeps passing already trusted Rust references,
future slices can accidentally skip type, bounds, encoding, or ownership
checks.

## Decision

Runtime operations now carry untrusted boundary values, not already trusted
native strings. The first accepted value shape is:

```text
StringBytes(&[u8])
```

The native boundary validator converts `StringBytes` into a trusted
`RuntimeString` only after proving:

* the value kind is a string
* the byte length is nonzero
* the byte length is at most 128 bytes
* the bytes are valid UTF-8

The boundary explicitly rejects unsupported non-string values, raw pointer
address shaped values, and unchecked native-struct byte shapes. These rejected
shapes exist only as internal negative-test inputs; they are not accepted ABI
objects and are never exposed to Python as valid values.

Host calls return an explicit host-call result:

```text
Returned
Rejected(ValueValidationError)
```

Capability failures remain separate authorization failures. Value rejection is
not silently converted into host-call success.

The slice completes only after the runtime's current `system.log` value is
validated, negative value proofs pass, and the boot oracle observes:

```text
PYTHOS:CORE:VALUE_VALIDATION_READY
```

## Deferred

The following remain outside this slice:

* general Python object model
* arbitrary string allocation or ownership transfer
* Python exceptions raised from validation failures
* `self.ready()` lifecycle transition
* service manager policy
* user-mode syscall copy-in/copy-out rules
* Phase 8 hardware-enforced pointer containment

## Consequences

Phase 4 now has a small value discipline for the existing trusted-kernel-mode
runtime path. The current proof is intentionally narrow, but the interpreter no
longer hands host calls pre-trusted native strings. Later slices can extend the
value enum and validation rules without treating raw pointers or unchecked
native structures as acceptable runtime values.
