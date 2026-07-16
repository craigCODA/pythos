# Phase 4 Runtime Selection Design

## Goal

Choose the first embedded Python runtime for PythOS by evidence, not by
embedding convenience. The selected runtime must fit Phase 4's narrow
capability-gated `system.*` boundary and minimize the later Phase 8 migration to
ring-3, separate address spaces, and syscall-mediated service calls.

## Scope

This design covers only the `runtime-selection` slice of Phase 4. It may add
research notes, ADRs, benchmark or smoke-test scripts, marker tests, and
documentation. It must not add a Python interpreter to `core/`, execute
`INIT.PAK` payloads, expose `system.*`, implement a service manager, add GUI,
storage, networking, AI, or begin any Phase 5+ work.

## Required Decisions

Two decisions must remain separate:

1. `docs/decisions/0012-phase-4-kernel-mode-runtime-sequencing.md` records the
   project-wide sequencing choice: trusted kernel-mode runtime prototypes
   through Phase 7, migration to hardware isolation in Phase 8.
2. A new runtime-selection ADR records the chosen runtime, benchmark evidence,
   smoke-test results, and specific rejection reasons for the other candidates.

The runtime-selection ADR must not duplicate or replace ADR 0012.

## Candidates

Evaluate exactly these first-round candidates:

```text
RustPython
MicroPython
custom minimal interpreter
```

Additional candidates require a roadmap amendment before entering this slice.

## Evaluation Criteria

Each candidate receives one evidence file under
`docs/research/runtime-selection/` and must be evaluated against the same
criteria:

```text
host-controlled embedding surface
no_std or bare-metal feasibility
cross-compile or link smoke-test result
memory ownership assumptions
allocator assumptions
threading assumptions
native call boundary shape
ability to deny ambient authority
startup path under PythCore
expected idle memory footprint
expected startup latency
maintenance activity
Phase 8 migration cost
specific rejection or acceptance reason
```

The selected runtime is the one with the lowest boundary and migration risk, not
the one with the fastest path to printing `hello`.

## Empirical Smoke Tests

Each candidate must include one empirical test that can disqualify it early:

### RustPython

Attempt to build the smallest Rust crate that depends on the chosen RustPython
crates with `default-features = false` for the host and then for
`x86_64-unknown-none` when feasible. The evidence must record whether the crate
pulls in `std`, OS threading, filesystem, dynamic loading, or allocator
assumptions before any PythCore integration is attempted.

### MicroPython

Attempt to compile the smallest MicroPython embedding or minimal port object
with the available Windows toolchain. The evidence must record the required C
toolchain, build-system assumptions, object files produced, and the expected
Rust-to-C boundary cost.

### Custom Minimal Interpreter

Write a tiny host-only prototype that parses and executes the exact Phase 4
target proof shape:

```python
class HelloService(Service):
    async def start(self):
        system.log("hello from Python")
        self.ready()
```

The prototype may be intentionally incomplete, but the evidence must record the
minimum parser, object model, async/event model, and compatibility debt required
to make it credible.

Smoke-test artifacts may live under `target/runtime-selection/` or `C:\tmp`.
Generated artifacts must not be committed.

## Runtime-Selection Marker

The `runtime-selection` slice should add one serial marker after the
runtime-selection ADR is accepted and before any runtime implementation begins:

```text
PYTHOS:CORE:RUNTIME_SELECTED
```

This marker proves that the boot oracle now enforces the Phase 4 decision gate.
It does not mean an interpreter has booted.

## Testing

The first commit in the slice should update marker tests to expect
`PYTHOS:CORE:RUNTIME_SELECTED` after `PYTHOS:CORE:PHASE_3_COMPLETE` and before
`PYTHOS:CORE:FRAMEBUFFER_READY`, then fail with the missing marker.

After the runtime-selection ADR lands, the implementation commit may emit the
marker from PythCore without adding interpreter code. Required acceptance:

```powershell
cargo fmt --check
cargo test -p pythos-core
cargo test -p pythos-shared
cargo clippy -p pythos-core --target x86_64-unknown-none -- -D warnings
cargo clippy -p pythos-boot --target x86_64-unknown-uefi -- -D warnings
python scripts\test-boot.py --slice runtime-selection
python scripts\test-boot.py --slice milestone-1
python scripts\test-boot.py --slice milestone-1 --media iso
```

## Output

The slice is complete only when:

```text
runtime-selection ADR accepted
all three candidate evidence files committed
rejected candidates have specific rejection reasons
selected runtime has a specific acceptance reason
PYTHOS:CORE:RUNTIME_SELECTED appears in the QEMU serial oracle
ESP and ISO boot paths still pass
```
