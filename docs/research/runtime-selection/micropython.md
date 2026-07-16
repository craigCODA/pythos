# MicroPython Runtime Evidence

## Smoke Test

Selection-neutral conclusion: MicroPython has a plausible porting model for
small bare-metal systems, but the required local smoke test did not produce an
object file on this workstation because no MicroPython source tree and no C
compiler/build tool were available on `PATH`.

Scratch path:

```text
target/runtime-selection/micropython/embed_probe.c
```

Probe source:

```c
#include "py/runtime.h"

int main(void) {
    mp_init();
    mp_deinit();
    return 0;
}
```

Discovery and probe commands:

```powershell
git status --short --branch
```

Result: exit code `0`; current branch reported
`## milestone/phase4-runtime-selection`.

```powershell
rg -n "MicroPython|micropython|mp_init|mp_deinit" .
```

Result: exit code `0`; only planning, roadmap, and `AGENTS.md` text mentions
were found before the scratch probe was created. No MicroPython runtime source
tree was found.

```powershell
rg --files -g "mpconfig*.h" -g "runtime.h" -g "compile.h" -g "mpstate.h" -g "mpconfigport.h"
```

Result: exit code `1`; no MicroPython headers were found in the workspace.

```powershell
Test-Path micropython
Test-Path target\runtime-selection\micropython\micropython
```

Result: both commands returned `False`.

```powershell
Get-ChildItem -Path . -Directory -Recurse -Filter micropython
```

Result: exit code `0`; only the allowed scratch directory
`target/runtime-selection/micropython` was present.

```powershell
Get-Command cl,clang,gcc,cc,cmake,nmake,make,ninja -ErrorAction SilentlyContinue | Select-Object Name,Source,Version
```

Result: exit code `1`; no matching compiler or build command was returned.

```powershell
where.exe cl clang gcc cc cmake nmake make ninja
```

Result: exit code `1`;
`INFO: Could not find files for the given pattern(s).`

```powershell
clang -c target\runtime-selection\micropython\embed_probe.c -Itarget\runtime-selection\micropython\micropython -o target\runtime-selection\micropython\embed_probe.obj
```

Result: exit code `1`; blocking error:

```text
The term 'clang' is not recognized as a name of a cmdlet, function, script file, or executable program.
```

```powershell
cl /nologo /c target\runtime-selection\micropython\embed_probe.c /Itarget\runtime-selection\micropython\micropython /Fotarget\runtime-selection\micropython\embed_probe.obj
```

Result: exit code `1`; blocking error:

```text
The term 'cl' is not recognized as a name of a cmdlet, function, script file, or executable program.
```

```powershell
Get-ChildItem -Force target\runtime-selection\micropython
```

Result: exit code `0`; produced files:

```text
embed_probe.c
```

Object files produced: none.

C compiler used: none. `clang` and MSVC `cl` were both unavailable on `PATH`.

Build-system assumptions identified from upstream documentation: a real port
build is not a single-header embed. It needs the MicroPython source tree, a C
toolchain, Makefile integration, generated build files, `mpconfigport.h`,
`mphalport.h`, platform I/O hooks, selected core source files, and qstr scanning.
The MicroPython porting guide describes the minimal firmware path as a port
under `ports/`, with `main.c`, a Makefile, configuration headers, HAL code, a
GC heap, `gc_init`, `mp_init`, and `mp_deinit`. Source:
https://docs.micropython.org/en/latest/develop/porting.html

## Host-Controlled Embedding Surface

MicroPython exposes a C API through headers under `py/`; upstream documentation
calls out `runtime.h` and `obj.h` as important runtime API surfaces. Source:
https://docs.micropython.org/en/latest/develop/publiccapi.html

For PythOS, the host-controlled surface would likely be a narrow C shim linked
as a static library and called from Rust through `extern "C"`. The shim should
own `mp_init`, `mp_deinit`, script execution, exception-to-status conversion,
and registration of only the Phase 4 `system.*` module. Direct exposure of broad
MicroPython internals to Rust would increase the ABI surface and make Phase 8
migration harder.

MicroPython can support denial of ambient authority if the port omits filesystem
imports, hardware modules, networking modules, and unrestricted native modules.
The porting guide's minimal example returns no filesystem entries from
`mp_import_stat` and raises an error for file lexer creation, which matches the
Phase 4 requirement to expose only capability-gated host calls.

## no_std Or Bare-Metal Feasibility

MicroPython is C, not Rust `no_std`, so Rust `no_std` feasibility is an FFI and
linking question rather than a crate feature question. Bare-metal feasibility is
credible because upstream provides a porting guide and a minimal port intended
as a reference implementation. The minimal-port README says the port can run on
the host and on STM32F4-class MCU targets, with `make` for host and a cross build
mode for Cortex-M. Source:
https://github.com/micropython/micropython/blob/master/ports/minimal/README.md

PythOS still needs a new x86_64 PythCore-specific port. The existing evidence
does not prove it will link into `x86_64-unknown-none`, does not prove it avoids
all libc assumptions after configuration, and does not prove it can coexist with
PythCore's interrupt, stack, panic, and allocator rules. The local smoke test
could not reach those questions because source and toolchain discovery failed.

## Memory And Allocator Assumptions

MicroPython expects the port to provide a managed heap and garbage-collection
root discovery. Upstream's minimal port example allocates a static heap, calls
`gc_init(heap, heap + sizeof(heap))`, then calls `mp_init()`. The GC header
surface includes heap initialization, collection start/root/end, lock/unlock,
allocation, free, and realloc operations. Sources:
https://docs.micropython.org/en/latest/develop/porting.html and
https://github.com/micropython/micropython/blob/master/py/gc.h

PythOS impact:

- The heap must be a fixed kernel-owned region in Phase 4; no ambient allocator
  access should be granted to MicroPython.
- GC root collection needs an x86_64 PythCore implementation that respects the
  active task stack layout and does not scan unmapped guard pages.
- Any use of `alloca`, libc allocation, qstr generation products, or generated
  build artifacts must be audited during the real port.
- Expected idle memory footprint cannot be measured locally in this pass because
  no object or firmware was built.

## Threading And OS Assumptions

The minimal-port path does not require OS threads for a basic interpreter loop,
but it does require platform hooks for C stack initialization, input/output,
fatal error behavior, imports, time/interrupt behavior where enabled, and GC
collection. Thread support must remain disabled for Phase 4 unless a later
milestone explicitly introduces runtime-level concurrency.

For PythOS, MicroPython must run as a single trusted kernel-mode runtime task
behind PythCore scheduling and IPC. It must not create host threads, depend on
Windows/POSIX file descriptors, block the timer path, or assume it owns interrupt
delivery.

## Native Boundary Shape

Expected boundary:

```text
Rust PythCore
-> small unsafe FFI wrapper with documented invariants
-> C MicroPython port shim
-> MicroPython VM
-> registered system.* functions
-> capability-checked PythCore IPC/log/status calls
```

Rust-to-C boundary cost is moderate. It requires a C toolchain in the build,
static-library or object integration, symbol naming control, C panic/fatal-error
containment, strict ownership rules for buffers crossing FFI, and a generated
binding layer for the narrow host API. It is cleaner than exposing Python code
directly to core internals, but less native to this repository than a Rust-only
runtime.

No `unsafe` Rust was added in this evidence pass. Any later FFI integration must
document the unsafe invariants required by `AGENTS.md`.

## Phase 8 Migration Cost

MicroPython's C VM can migrate behind Phase 8 hardware isolation if Phase 4
keeps the API narrow. The expected migration path is to keep Python-visible
`system.*` calls stable while replacing direct C-to-core calls with syscalls or
IPC mediated by service identities and capabilities.

Migration risks:

- The VM heap, GC stack scanning, qstr pool, and native module state must move
  from kernel-owned memory to a runtime address space.
- Any direct pointer borrowed from PythCore into MicroPython must be eliminated
  before hostile-code isolation.
- Fatal-error and exception paths must become recoverable service failures, not
  kernel panics.
- A C build and port layer must remain maintained beside the Rust core.

Migration advantage: if the Phase 4 shim is kept small, the same Python-facing
module boundary can survive the move from trusted kernel-mode prototype to
syscall-mediated runtime service.

## Acceptance Or Rejection Reason

MicroPython should not be selected solely from this local pass, because the
empirical smoke test is blocked: there is no local MicroPython source tree and
no C compiler/build tool on `PATH`, so no object file was produced.

MicroPython remains architecturally plausible for Phase 4 because upstream
documents a minimal porting path, C API, explicit heap setup, no-filesystem
minimal behavior, and configurable features. The concrete acceptance condition
for a later runtime-selection ADR should be a rerun with a checked-out
MicroPython source tree and a pinned C toolchain that successfully builds a
minimal PythCore-facing object or static library and records produced objects,
heap size, disabled modules, and host-call shims.

Selection-neutral evidence status: blocked local smoke test; do not treat this
file as the final runtime choice.
