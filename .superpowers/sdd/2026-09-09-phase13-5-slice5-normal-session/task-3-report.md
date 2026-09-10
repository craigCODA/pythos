# Task 3 implementation report: normal ring-3 runtime and status graph

Date: 2026-09-10

Implementation commit: `465a90a325ca71b0c8c0905e9ad4651d0fd6826c`

## Implemented surfaces

- Added the feature-gated `pythos-normal-session` bare-metal binary at `user/session-runtime/src/normal_main.rs`; the existing `src/main.rs` remains byte-identical (SHA-256 `C0BD64F4EA98D391AE65E980F5883B9FD8D2F8D7FF9630057A1967E5C8CE0757`).
- Added `NormalGraphRunner`, `NormalGraphInvocation`, and the testable `NormalGraphExitSink` boundary in `normal_graph.rs`. Each call constructs a fresh `SessionCommandHost` and `Interpreter`, validates the real command result and graph lifecycle record, and uses runtime-owned static invocation tables.
- Every interpreter-produced `GraphExitRecord`, including budget/runtime/non-result exits, is sent to the sink before adapter validation. Pre-execution host validation emits no record. The binary's sink performs the only raw `write_volatile`, to the already validated `bootstrap.graph.result_ptr` at return-page offset 64. The typed `NormalSessionReturnV1` still writes separately through `bootstrap.return_ptr` at offset 0.
- Added direct typed syscall adapters for nonblocking input, COM2 read/write, session wait, and scalar presentation. Result classifiers distinguish legal byte/readiness words from syscall error words.
- Added the exact approved `programs/normal-session-manager/main.pyth`. Host tests compile the actual source through `typecheck_source -> lower_program -> encode_verified_graph -> verify_package`. The old graph remains status-silent and kind-3 capable; the normal graph is status-only.
- Added `normal-session` and `normal-session-fault-test` features and explicit old/new binary selection with isolated target directories in `build-session-runtime.py`.
- Added an acceptance-only, compile-time-gated UD2 helper after the third completed status write, at least eight accepted events, matching presentation revision, and active state. The ordinary normal image contains no runtime fault flag and continues indefinitely.
- The large bootstrap/import/value/host-result/input storage is one static `UnsafeCell` region with documented single-entry/single-core ownership. No interpreter table is constructed or copied by value on the user stack.

## TDD evidence

### Actual status graph

RED:

`cargo test -p pythos-user-session-runtime admitted_session_manager_emits_the_live_system_status_result -- --nocapture`

The admitted old source reached `GRAPH_EXIT_OK` but returned `None`; the assertion failed by unwrapping the missing result. This reproduced the compile/interpreter mismatch rather than using a canned graph.

GREEN:

`cargo test -p pythos-user-session-runtime normal_session_manager -- --nocapture`

Result: 1 passed. The compiled normal source emitted the exact 61-byte SYSTEM_STATUS result and encoded to 696 bytes.

`cargo test -p pythos-user-session-runtime session_graph -- --nocapture`

Result: 1 passed. It also proved the old source still emits kind 3 while the normal source rejects it, and the old source remains status-silent.

### Live adapter and graph-result exchange

Initial adapter RED was a compile failure for the missing `NormalGraphRunner`; implementation then passed three real-package adapter tests. The clarified graph-result write began with:

`cargo test -p pythos-user-session-runtime normal_graph::tests -- --nocapture`

RED: `error[E0405]: cannot find trait NormalGraphExitSink in this scope`.

GREEN: 3 passed. The tests compare the exact recorded exits for two successful calls, a successful exit with missing command result, budget exhaustion, and runtime error. Invalid UTF-8 fails before interpretation and records no exit.

### Syscall, fault predicate, and builder boundaries

- Syscall classifier RED: missing result classifiers; GREEN: 2 passed, covering all wait masks 0..3, invalid mask 4, NO_BYTE versus all byte values, input event/empty, and effect errors.
- Fault predicate RED: missing `fault_acceptance_ready`; GREEN: 1 passed with each acceptance precondition independently false and the exact accepted state true.
- Builder/boundary RED: four missing normal/fault/explicit-bin cases; GREEN commands:
  - `uv run --no-project --with pytest python -m pytest tests/test_build_orchestration.py -q` -> 29 passed, 6 subtests passed.
  - `uv run --no-project --with pytest python -m pytest tests/test_session_runtime_boundary.py -q` -> 16 passed, 112 subtests passed.

## Final verification

- `cargo test -p pythos-user-session-runtime --all-features` -> library 45 passed; normal binary 1 passed; 0 failed.
- `cargo fmt --all -- --check` -> exit 0.
- `git diff --check` and staged `git diff --cached --check` -> exit 0.
- Strict target Clippy, each with `-- -D warnings`, passed for:
  - normal binary with `--features normal-session`;
  - normal binary with `--features normal-session-fault-test`;
  - retained old binary with `--features session-viewing`.
- Actual builders passed:
  - `uv run --no-project python scripts/build-session-runtime.py --features normal-session`
  - `uv run --no-project python scripts/build-session-runtime.py --features normal-session-fault-test`
  - `uv run --no-project python scripts/build-session-runtime.py --features session-viewing`
- `uv run --no-project python scripts/verify-user-elf.py --elf <artifact>` -> `USER_ELF_VERIFY_OK` for all three artifacts below.
- `cargo run -p pythc -- build programs/normal-session-manager/main.pyth -o target/normal-session/pyth-tig/session-manager.tig` -> `PYTHC_BUILD_OK`.
- `cargo run -p pyth-tig-tool -- verify target/normal-session/pyth-tig/session-manager.tig` -> `PYTH_TIG_VERIFY_OK`.

## Artifacts and layout evidence

- Normal ELF: `target/normal-session/x86_64-unknown-none/debug/pythos-normal-session`, 2,003,632 bytes. PT_LOAD memory sizes: RX 105,151 (26 pages), RO 14,344 (4 pages), RW 140,736 (35 pages): 65 load pages total.
- Fault ELF: `target/normal-session-fault-test/x86_64-unknown-none/debug/pythos-normal-session`, 2,005,776 bytes. PT_LOAD memory sizes: RX 105,535 (26 pages), RO 14,336 (4 pages), RW 140,736 (35 pages): 65 load pages total.
- Viewing ELF: `target/session-viewing-probe/x86_64-unknown-none/debug/pythos-user-session-runtime`, 2,090,320 bytes.
- Normal graph: `target/normal-session/pyth-tig/session-manager.tig`, 696 bytes, SHA-256 `14A5EF6E8C4A0FBCAA4A12DD9AEF9E2B499932C05A396AF809A3561DA53C4FDB`.
- Fault helper symbol: `pythos_normal_session_fault_acceptance_ud2`, range `0x601510..0x601515`, size 5. Disassembly is `pushq %rax; ud2; ud2`, so the first genuine acceptance UD2 RIP is `0x601511` in this exact ELF.

## Changed files

- `Cargo.lock`
- `programs/normal-session-manager/main.pyth`
- `scripts/build-session-runtime.py`
- `tests/test_build_orchestration.py`
- `tests/test_session_runtime_boundary.py`
- `user/session-runtime/Cargo.toml`
- `user/session-runtime/src/lib.rs`
- `user/session-runtime/src/normal_graph.rs`
- `user/session-runtime/src/normal_main.rs`
- `user/session-runtime/src/normal_syscalls.rs`

## Self-review and remaining concerns

- The final code keeps policy in the existing `NormalSession` controller, graph mechanics in one adapter, raw syscalls in one leaf module, and mapped-page unsafe access in the executable boundary. No old probe source, old graph source, shared ABI, controller, kernel, boot default, packaging, or QEMU behavior was changed.
- The 65 ELF load pages fit below `MAX_RETAINED_USER_FRAMES = 128`, but Task 4 must calculate the complete launch demand using the selected user stack plus bootstrap/package/result mappings and any retained mapping/page-table overhead. This report does not claim that the ELF-only count proves boot admission.
- Task 3 deliberately makes no QEMU or physical-hardware acceptance claim. Task 5 should derive the expected fault RIP from the compiled fault ELF rather than reuse a synthetic fixed address.
- The concurrently modified plan file under `docs/superpowers/plans/` was preserved and excluded from the implementation commit.
