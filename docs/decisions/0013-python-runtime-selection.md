# ADR 0013: Python Runtime Selection

Date: 2026-07-16

## Status

Accepted

## Context

Phase 4 requires a Python runtime that can be embedded behind a narrow,
capability-gated `system.*` API and later migrated behind Phase 8 hardware
isolation. The project criterion is least painful Phase 8 migration, not the
fastest first print.

ADR 0012 records the separate sequencing decision: Phase 4 through Phase 7 may
prototype trusted runtime services in kernel mode, but all native/Python
interactions must already look like host-provided, capability-checked boundary
calls.

The first runtime-selection pass evaluated:

```text
RustPython
MicroPython
custom minimal interpreter
```

Each candidate has an evidence file under `docs/research/runtime-selection/`.

## Decision

PythOS will use a custom minimal interpreter as its first embedded Python
runtime.

This is a deliberately narrow Phase 4 service-language runtime. It is not a
claim of CPython compatibility, standard-library availability, general Python
module loading, native extension support, or unrestricted Python execution.

The first accepted language target is the Phase 4 proof shape:

```python
class HelloService(Service):
    async def start(self):
        system.log("PythOS [HISS] We Are Woken")
        self.ready()
```

The interpreter must reject unsupported syntax explicitly. It must expose only
host-provided `system.*` calls whose arguments are validated before any
capability-checked native operation executes.

## Evidence

RustPython evidence:
`docs/research/runtime-selection/rustpython.md` created a scratch Cargo project
depending on `rustpython-vm = { version = "0.5.0", default-features = false }`.
The host check passed:

```text
cargo check --manifest-path target\runtime-selection\rustpython\Cargo.toml
exit code: 0
```

The bare-metal target check failed:

```text
cargo check --manifest-path target\runtime-selection\rustpython\Cargo.toml --target x86_64-unknown-none
exit code: 1
error[E0463]: can't find crate for `std` in either-1.16.0
```

The dependency probes also found host-shaped dependencies such as `libloading`,
`libffi`, `num_cpus`, and `rustyline` in the VM dependency graph.

MicroPython evidence:
`docs/research/runtime-selection/micropython.md` attempted a minimal C embedding
probe using `mp_init()` and `mp_deinit()`. The local smoke test was blocked
before compile because no MicroPython source tree and no C compiler or build
tool were present on `PATH`:

```text
rg --files -g "mpconfig*.h" -g "runtime.h" -g "compile.h" -g "mpstate.h" -g "mpconfigport.h"
exit code: 1

where.exe cl clang gcc cc cmake nmake make ninja
exit code: 1
```

No object files were produced. The evidence still records that a real
MicroPython port would need a C toolchain, source tree, port configuration
headers, generated qstr/build artifacts, a managed heap, GC root handling, and a
Rust-to-C shim boundary.

Custom minimal interpreter evidence:
`docs/research/runtime-selection/custom-minimal-interpreter.md` built a
host-only prototype that recognizes only the Phase 4 target proof shape and
emits a bounded internal operation list. The smoke test passed:

```text
python target\runtime-selection\custom-minimal\prototype.py target\runtime-selection\custom-minimal\hello_service.py
exit code: 0
CUSTOM_MINIMAL_SMOKE success
```

The prototype demonstrated a host-controlled boundary for:

```text
system.log("PythOS [HISS] We Are Woken")
self.ready()
```

It did not prove full Python compatibility, coroutine semantics, imports,
exceptions, object dictionaries, standard-library support, or PythCore
integration.

## Rejected Options

RustPython is rejected for direct Phase 4 PythCore embedding. The host build
works, but the `x86_64-unknown-none` check fails on a transitive `std`
requirement, and the dependency graph brings broad OS-shaped surfaces into the
trusted kernel-mode prototype. Using it now would require fork-level feature
work before PythCore integration and would increase the later Phase 8 migration
cost.

MicroPython is rejected for this slice. It remains architecturally plausible,
but this pass did not produce a compiled object because the source tree and C
toolchain were absent. Selecting it now would make the next step a C porting and
build-system project before the `system.*` boundary is proven, and would add
FFI, GC-root scanning, fatal-error containment, and C toolchain maintenance to
the kernel-mode prototype.

## Consequences

The accepted path gives PythOS the smallest authority surface for Phase 4. The
runtime can be designed around fixed buffers, explicit host calls,
service-identity-aware capability checks, deterministic rejection of unsupported
syntax, and a future Phase 8 move to syscalls or IPC without preserving broad
ambient runtime access.

The cost is compatibility debt. PythOS now owns the first parser, service object
model, async-entry semantics, diagnostics, value validation rules, and runtime
test suite for this Python-shaped subset. Every Phase 4 document and marker
must be clear that the first runtime is a minimal service interpreter, not a
general Python implementation.

The next Phase 4 slice, `init-pak-loading`, must load only the selected minimal
runtime payload format from validated `INIT.PAK`. It must not silently expand
into imports, packages, filesystem access, native extensions, standard-library
modules, or general Python source execution.
