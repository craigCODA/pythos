# ADR 0015: Custom Minimal Interpreter Bootstrap Boundary

Date: 2026-07-16

## Status

Accepted

## Context

ADR 0013 selects a custom minimal interpreter as the first Phase 4 runtime, and
ADR 0014 defines the `INIT.PAK` payload format that carries its initial source.
The `interpreter-boot` slice must start that runtime path without pulling in
later Phase 4 scope such as `system.*` execution, value conversion rules,
service-manager lifecycle policy, or async event delivery.

## Decision

The first PythCore interpreter bootstrap recognizes only the exact Phase 4
proof source:

```python
class HelloService(Service):
    async def start(self):
        system.log("hello from Python")
        self.ready()
```

Booting the interpreter means:

* decoding the already validated runtime source from `INIT.PAK`
* rejecting any source outside the exact accepted shape
* synthesizing a fixed internal operation plan:
  * `system.log("hello from Python")`
  * `self.ready()`
* assigning the runtime to a fixed native task id
* assigning a kernel service identity to that task
* requiring an explicit boot capability before the runtime instance is created
* emitting `PYTHOS:CORE:INTERPRETER_BOOTED`

This slice does not execute the internal operations. In particular,
`system.log(...)` and `self.ready()` are parsed as a bounded instruction plan
only. Their host-call semantics, argument validation, capability checks,
service readiness state, and exception behavior are later Phase 4 slices.

## Consequences

PythOS now has a real PythCore-side interpreter bootstrap path, but not a
general Python runtime. The accepted language surface remains deliberately
smaller than Python and must reject unsupported syntax deterministically.

This keeps the authority boundary narrow: the runtime starts with a service
identity and explicit boot capability, not ambient kernel authority. The cost is
that later Phase 4 slices must define and test each missing language/runtime
concept explicitly instead of relying on CPython-compatible behavior.
