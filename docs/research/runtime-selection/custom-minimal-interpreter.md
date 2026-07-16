# Custom Minimal Interpreter Runtime Evidence

## Smoke Test

Temporary host-only artifacts:

```text
target/runtime-selection/custom-minimal/hello_service.py
target/runtime-selection/custom-minimal/prototype.py
```

The prototype recognizes only this exact Phase 4 proof shape:

```python
class HelloService(Service):
    async def start(self):
        system.log("hello from Python")
        self.ready()
```

Command:

```powershell
python target\runtime-selection\custom-minimal\prototype.py target\runtime-selection\custom-minimal\hello_service.py
```

Result:

```text
CUSTOM_MINIMAL_SMOKE start
system.log: hello from Python
service.ready: HelloService
CUSTOM_MINIMAL_SMOKE success
```

Exit code: `0`.

What this proves: a host-owned interpreter sketch can reject all source except
the exact target shape, synthesize a tiny instruction list, call a host-provided
`system.log` surface, and mark the service ready without exposing ambient
authority.

What this does not prove: Python compatibility, a real parser, a bytecode
format, coroutine semantics, exception behavior, imports, module loading,
standard-library behavior, or any boot-time PythCore integration.

## Host-Controlled Embedding Surface

The embedding surface is fully host controlled in the prototype. The source does
not execute through the host Python runtime as Python code; the prototype checks
the exact text shape and emits only two internal operations:

```text
system.log("hello from Python")
self.ready()
```

The only authority exposed to interpreted code is whatever the host chooses to
bind for those operations. That matches the desired Phase 4 direction better
than a general Python runtime with imports, files, process APIs, reflection, and
native extension hooks enabled by default.

The cost is that the host must define nearly every Python-facing concept itself:
class declaration recognition, service inheritance rules, method lookup,
descriptor behavior, bound `self`, `system.*` capability checks, diagnostics,
and rejection paths for unsupported syntax.

## no_std Or Bare-Metal Feasibility

A custom interpreter has the cleanest theoretical `no_std` path because it can
be designed around PythCore's existing constraints: no OS calls, no filesystem,
no threads, no dynamic loading, and no hidden runtime initialization. A real
implementation could use `core` plus fixed-capacity buffers first, then add an
explicit allocator only when the runtime-selection slice defines one.

The smoke prototype is host Python and is not itself bare-metal evidence. It is
evidence about the minimum recognized language surface only. A credible
bare-metal follow-up would need a Rust `#![no_std]` parser/executor sketch that
operates on bytes from an `INIT.PAK` payload and reports deterministic errors
over the existing serial oracle.

## Memory And Allocator Assumptions

The minimal language shape can be represented without a general heap:

```text
class name: fixed string or interned symbol
method body: fixed instruction array
string literal: bounded byte slice
service instance: fixed record with readiness state
host calls: fixed dispatch table
```

That is favorable for early PythCore because the first prototype could use
static or fixed-capacity storage and avoid garbage collection. The compatibility
debt is substantial: real Python object identity, dictionaries, attribute
mutation, exceptions, closures, comprehensions, generators, arbitrary strings,
lists, modules, and async task state all introduce allocation pressure. If the
project later wants ordinary Python source rather than a Python-shaped service
DSL, a custom runtime would need a deliberate heap strategy and probably a
collector or strict ownership restrictions.

## Threading And OS Assumptions

The custom path can assume no host OS threads. The prototype runs one service
start body synchronously and treats `async def start(self)` as a recognized
entry label rather than as a real coroutine. That keeps early execution
compatible with the current single-CPU, kernel-owned scheduler model.

The debt is that `async` is not credible until the interpreter owns an event
model. Phase 4 would need to define whether service `start()` returns a tiny
future object, a bytecode continuation, or a one-shot entrypoint that merely uses
Python-like syntax. Until then, `async def` is syntax compatibility only, not
Python coroutine compatibility.

## Native Boundary Shape

The clean boundary shape is a fixed host-call table:

```text
RuntimeHost {
    log(message_slice, caller_service_id) -> Result
    ready(caller_service_id) -> Result
}
```

The interpreter should not receive raw authority. It should receive a service
identity, a bounded bytecode/source slice, fixed memory limits, and a dispatch
table whose entries validate capabilities before performing privileged work.
That maps well to Phase 8: host calls can become syscalls or IPC messages with
the same conceptual shape.

The risky boundary is object fidelity. If the custom interpreter tries to mimic
CPython's object model too closely, native helpers will start depending on
Python implementation details that become expensive to preserve across the
Phase 8 isolation boundary.

## Phase 8 Migration Cost

Kernel-mode Phase 4 through Phase 7 prototypes could keep this interpreter
small and deterministic, then migrate by moving the interpreter into a ring-3
runtime service and replacing direct host calls with syscall or IPC stubs. That
is a low boundary-migration cost if the language surface remains deliberately
small and capability calls are table-driven from the start.

The migration cost becomes high if early services depend on custom-only syntax,
nonstandard async semantics, or missing Python behavior. Every service written
before Phase 8 would either need to stay within the reduced language subset or
be ported later to the chosen full Python runtime. The more "Python-like" the
custom runtime claims to be, the larger the future compatibility trap.

## Acceptance Or Rejection Reason

Disposition for this evidence only: not rejected by the exact-shape smoke test,
but not accepted as the final runtime by this document.

Reason to keep it in comparison: it offers the narrowest host-controlled
embedding surface and the most plausible early `no_std` story because every
operation can be designed around PythCore's capability, allocator, and scheduler
constraints.

Reason to reject it later: it is not a Python runtime yet. The compatibility
debt is the largest of the three candidate families because PythOS would own
the parser, object model, async model, diagnostics, service ABI, test suite,
documentation, and long-term user expectations for Python behavior.
