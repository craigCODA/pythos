# RustPython Runtime Evidence

## Smoke Test

Scratch project:

```text
target/runtime-selection/rustpython/Cargo.toml
target/runtime-selection/rustpython/src/lib.rs
```

The scratch crate imports `rustpython-vm` with default features disabled:

```toml
[dependencies]
rustpython-vm = { version = "0.5.0", default-features = false }
```

It references the VM type directly:

```rust
#![cfg_attr(target_os = "none", no_std)]

pub fn rustpython_vm_type_is_visible() -> &'static str {
    core::any::type_name::<rustpython_vm::VirtualMachine>()
}
```

Commands and results:

```text
cargo search rustpython-vm --limit 3
exit code: 0
result: crates.io reported rustpython-vm = "0.5.0".
```

```text
cargo info rustpython-vm
exit code: 0
result: rustpython-vm 0.5.0, rust-version 1.93.0. Default features are
compiler, wasmbind, gc, host_env, and stdio. Optional threading feature exists.
```

```text
cargo check --manifest-path target\runtime-selection\rustpython\Cargo.toml
exit code: 1
blocking error: Cargo treated the scratch package as part of the root workspace:
"current package believes it's in a workspace when it's not". The scratch
manifest was then isolated with an empty [workspace] table.
```

```text
cargo check --manifest-path target\runtime-selection\rustpython\Cargo.toml
exit code: 0
result: host build passed. Cargo locked 229 packages and compiled
rustpython-vm v0.5.0 with default features disabled.
```

```text
cargo check --manifest-path target\runtime-selection\rustpython\Cargo.toml --target x86_64-unknown-none
exit code: 1
blocking error:
error[E0463]: can't find crate for `std`
  --> ...\either-1.16.0\src\lib.rs:19:1
   |
19 | extern crate std;
   | ^^^^^^^^^^^^^^^^^ can't find crate
   |
   = note: the `x86_64-unknown-none` target may not support the standard library
```

Dependency probes:

```text
cargo tree --manifest-path target\runtime-selection\rustpython\Cargo.toml -e normal --target x86_64-unknown-none -i either
exit code: 0
result: either -> itertools -> malachite/rustpython compiler and common crates
-> rustpython-vm -> scratch crate. This is enough to fail before PythCore
integration because `either` requires std for x86_64-unknown-none.
```

```text
cargo tree --manifest-path target\runtime-selection\rustpython\Cargo.toml -e normal -i libloading
exit code: 0
result: libloading is a direct rustpython-vm dependency on the Windows host.
```

```text
cargo tree --manifest-path target\runtime-selection\rustpython\Cargo.toml -e normal -i libffi
exit code: 0
result: libffi is a direct rustpython-vm dependency on the Windows host.
```

```text
cargo tree --manifest-path target\runtime-selection\rustpython\Cargo.toml -e normal -i num_cpus
exit code: 0
result: num_cpus is a direct rustpython-vm dependency for non-wasm targets.
```

```text
cargo tree --manifest-path target\runtime-selection\rustpython\Cargo.toml -e normal -i rustyline
exit code: 0
result: rustyline is a direct rustpython-vm dependency for non-wasm targets.
```

The normalized `rustpython-vm 0.5.0` manifest also shows unconditional
dependencies with `std` enabled, including `chrono` with `std`, `getrandom`
with `std`, `indexmap` with `std`, `parking_lot`, `libc`, and target-specific
Windows, Unix, dynamic-loading, and C-FFI dependencies.

## Host-Controlled Embedding Surface

RustPython exposes a Rust VM embedding surface (`VirtualMachine`,
`Interpreter`, `InterpreterBuilder`, and `Settings` are visible in the crate
source), so a host can plausibly create and configure an interpreter from native
Rust. This is attractive for a capability-gated `system.*` boundary because
PythCore could theoretically construct only the objects and modules it wants to
publish.

The empirical build shows that this surface is not currently narrow enough for
PythCore as-is. Even with `default-features = false`, the VM crate still brings
large builtin and stdlib implementation surfaces plus direct dependencies tied
to dynamic loading, libffi, terminal support, CPU discovery, OS error handling,
and platform APIs. Host control exists at the API layer, but the dependency and
compiled-code surface is much wider than Phase 4's intended trusted prototype.

## no_std Or Bare-Metal Feasibility

The `x86_64-unknown-none` smoke test failed before linking the scratch crate.
The first hard blocker was a transitive dependency requiring `std`:

```text
either v1.16.0 -> itertools v0.14.0 -> malachite/rustpython compiler/common
crates -> rustpython-vm v0.5.0
```

This means RustPython is not presently feasible as a direct `no_std`
PythCore dependency by changing only Cargo feature flags in the embedding
crate. A bare-metal port would need upstream feature work or a project fork to
remove or replace `std`-bound dependencies before any kernel integration could
start.

## Memory And Allocator Assumptions

RustPython assumes a rich allocator-backed object model. The host check pulled
in heap-heavy structures such as hash maps, big integers, parser/compiler data
structures, Unicode tables, locks, and Python object storage. The default GC
feature can be disabled, but the VM still requires allocation throughout normal
operation.

For PythCore this would require a kernel allocator mature enough for Python
object churn, clear failure behavior for allocation exhaustion, and strict
resource accounting per service identity. The current Phase 4 boundary has not
yet proven those allocator policies.

## Threading And OS Assumptions

The optional `threading` feature is disabled by the scratch crate, but the VM
still depends on synchronization and host/OS-shaped crates. The host dependency
tree includes `parking_lot`, `crossbeam-utils`, `num_cpus`, Windows API crates,
`libc`, `errno`, and terminal/console support. The source also contains stdlib
modules for filesystem, process, time, OS, Windows, POSIX, `_thread`, `_ctypes`,
and similar host features.

Those modules can potentially be withheld from exposed Python code, but their
presence in the crate makes denial of ambient authority a compile-time and
maintenance problem rather than a simple runtime policy decision.

## Native Boundary Shape

RustPython's native boundary is Rust-native, which is better than a C ABI for
PythCore ownership and unsafe policy. A future PythCore integration could expose
a deliberately small native module for `system.log`, service readiness, and
capability-mediated IPC.

The boundary problem is negative space: PythCore would also need to prevent
ordinary Python imports and builtin modules from reaching filesystem, process,
dynamic loading, C FFI, terminal, clock, random, or threading capabilities. With
the current crate shape, this likely means building a custom RustPython
configuration, patching feature gates, and auditing stdlib module registration
instead of just registering one safe module.

## Phase 8 Migration Cost

Phase 8 moves runtime work behind hardware isolation and syscall-mediated
service calls. RustPython's Rust-native embedding could migrate conceptually by
moving the VM into a user-mode runtime service and replacing direct PythCore
calls with syscalls or IPC stubs.

The migration cost is still high if Phase 4 embeds current RustPython directly
in kernel mode. Any kernel-only shims for allocation, dynamic loading denial,
filesystem denial, stdlib pruning, clocks, randomness, and host environment
emulation would have to be undone or carried into the user-mode runtime service.
The crate also currently fails the bare-metal target before integration, so a
Phase 4 kernel embedding path would require fork-level work that may not survive
the Phase 8 transition cleanly.

## Acceptance Or Rejection Reason

RustPython should be rejected for direct Phase 4 PythCore embedding unless the
project is willing to fork or upstream a substantially narrower `no_std` VM
profile first. The host build succeeds with `default-features = false`, proving
the crate can be imported on Windows, but the `x86_64-unknown-none` check fails
on a transitive `std` requirement and the host dependency graph still carries
OS, filesystem, terminal, dynamic-loading, C-FFI, synchronization, and allocator
assumptions.

This is only RustPython evidence. It does not select the final Phase 4 runtime.
