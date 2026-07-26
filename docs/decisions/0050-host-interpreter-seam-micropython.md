# ADR 0050: Host–Interpreter Seam, With MicroPython Behind It

Status: Accepted

## Context

The vertical loop (ADR 0049) needs a general-purpose Python interpreter running
arbitrary code in ring 3, replacing the toy that recognizes one hardcoded
program. The load-bearing decision is **not** "which interpreter" — it is the
**seam** between the interpreter and PythOS's object system. If the interpreter's
object model leaks past that seam it becomes permanent; if the seam stays narrow,
swapping interpreters later is a bounded cost. This ADR records the seam and the
build/runtime constraints, and adopts **MicroPython** behind it.

Why MicroPython (not RustPython, not grow-custom): the freestanding-x86_64 hard
parts are already solved — `nlrx64.c` (per-arch setjmp/longjmp, the piece that
kills most bare-metal ports), `gc_init` against a static buffer (no allocator
integration), `mp_lexer_new_from_str_len` → compile → exec (run arbitrary Python
from a string with no VFS/import/filesystem), and a ~200-line libc shim (no
newlib). RustPython 0.5 is `std`-shaped (threading/posix) with no `no_std` story —
adopting it means maintaining a fork forever for a language subset we don't need.
Grow-custom is years of writing a language runtime that is a *dependency* of the
project, not the project.

Codebase reality that de-risks this (verified 2026-07-26): PythOS already has a
proven ring-3 + syscall substrate — `syscall_entry_abi`/`sysretq`
(`core/src/syscall.rs`, Phase 8/9), copy-in/out (`user_copy.rs`), and dynamic
user-ELF loading (`user_elf`, Phase 9). So MicroPython is **one** unknown dropped
into a marker-verified environment, not two stacked unknowns.

## Decision

### The seam (host interface)

A narrow host interface is the durable contract. The interpreter reaches the host
only through it; the host object model never leaks into interpreter internals and
vice versa. Realistic width is ~9–10 operations:

```
eval_str(src) -> result | error
get_attr(handle, name) -> value
set_attr(handle, name, value)
call(handle, args) -> value | raise
enumerate(handle) -> iterator
type_of(handle) -> type            // Python calls type(x)
identity(handle) -> id             // Python uses `is`
retain(handle)                     // host-owned lifetime
release(handle)
raise_across(error)                // host<->interpreter error crossing
```

**Host object references are opaque small integer handles into a host-side
table — never raw pointers.** MicroPython's GC scans stack and heap
*conservatively*; a raw kernel pointer stored in an MP object is a value the
collector will try to reason about, and one that aliases into the MP heap range
becomes kernel memory treated as an MP object. Integer handles are invisible to
the scanner and give the host explicit lifetime control (`retain`/`release`),
eliminating the boundary use-after-free class. The seam is written at full width
now; a seam amended in month two has lost its authority.

### Build constraints (hard, not notes)

- **AVX off, everywhere.** FXSAVE/FXRSTOR saves x87+SSE (512 bytes) but **not**
  YMM upper halves; a single AVX instruction anywhere in userspace silently
  corrupts context-switched state. Pin it off in both toolchains: Rust
  `-C target-feature=+sse2,-avx,-avx2,-avx512f` for user code; C
  `-msse2 -mno-avx -mno-avx2 -mno-avx512f`. FXSAVE is then complete forever; do
  **not** take on XSAVE (CPUID 0xD / CR4.OSXSAVE / XCR0 / variable save areas)
  for no benefit.
- **Kernel stays soft-float.** Confirm the kernel binary emits no SSE
  (`objdump -d target/.../pythcore | grep -i xmm` is empty) so syscall entry need
  not preserve XMM; only user-task preemption does. Design against the verified
  fact, not the target default.
- **MicroPython linked `-static -no-pie` (ET_EXEC)** — `user_elf::validate`
  accepts `ET_EXEC` only, and it matches the kernel's `relocation-model=static` /
  `--no-pie`.
- **`MICROPY_FLOAT_IMPL_NONE` initially** — reduces surface, but does **not**
  remove the SSE requirement (C emits SSE for struct copies / inlined memcpy
  regardless of float support).
- **Vendor MicroPython at a pinned commit** inside the repo (no floating
  submodule) so the `verify` build is reproducible; pin the C compiler in CI.
- **Binary size 200–400 KB** — check against the UEFI load path and memory-map
  assumptions before first load.

### Runtime constraints

- **FPU: eager, not lazy.** `fninit` + CR0 (clear EM, set MP) + CR4.OSFXSR/
  OSXMMEXCPT once at boot; **eager FXSAVE/FXRSTOR on user-task context switch**.
  No CR0.TS/#NM lazy FPU (Lazy FP State Restore, CVE-2018-3665).
- **GC stack bounds:** set `MP_STATE_THREAD(stack_top)` explicitly for the ring-3
  stack MicroPython runs on; a wrong bound collects live objects at random.
- **TSS.RSP0 on the interrupt path:** RSP0 must name the running user task's
  kernel stack on every entry into ring 3, so a timer tick landing mid-C (mid-GC)
  switches stacks safely — the syscall path proves only its own stack.
- **Instrument boundaries with markers before the port:** syscall entry/exit,
  interpreter entry/exit, GC entry/exit. Once a large opaque C blob is in the
  loop the serial-marker oracle stops localizing failures unless the boundaries
  are already instrumented.

## Consequences

- Reversibility is the only genuinely hard-to-undo axis; a ~9–10-op seam keeps
  interpreter choice bounded. This ADR is really "adopt a seam," and
  "MicroPython" is the current implementation behind it.
- Sequencing (updates ADR 0049's plan): (1) **SSE/FPU userspace enablement**
  (small, unblocks all C); (2) **the host-object protocol in Rust** — define +
  prove the seam against the existing typed-object store, with its **own ordered
  marker sequence driven by a Rust harness** (no "looks reasonable" completion);
  (3) **port MicroPython** behind the seam, `eval_str` first. Shell / object
  browser / round-trip follow.
- The FPU/context-switch work is nondeterministic-corruption territory and is
  started cold, not at the tail of a long session.

## Alternatives Considered

- **RustPython** — rejected: `std`-shaped, no `no_std` story, fork-forever cost.
- **Grow the custom interpreter** — rejected: years to build a dependency.
- **A 6-op seam** — rejected: omits handle lifetime (`retain`/`release`),
  `type_of`, and identity, which then leak through ad-hoc paths.
- **Lazy FPU / XSAVE / AVX** — rejected: CVE-2018-3665 and unnecessary complexity.
