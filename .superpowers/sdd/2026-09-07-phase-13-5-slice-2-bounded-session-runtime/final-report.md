# Phase 13.5 Slice 2 Final Evidence Report

Date: 2026-09-07

## Boundary and repository state

- Branch: `agent/phase13-5-session-runtime`
- Merge base: `c189b15438a3727f730d50bf9509999b44e05e44`
- Accepted implementation/evidence-source HEAD:
  `6109425047de86cf60e4367313811fc284c43bad`
- Evidence-source status before Task 9: clean
- Task 9 scope: CI contract/workflow and evidence documentation only
- Canonical report directory:
  `.superpowers/sdd/2026-09-07-phase-13-5-slice-2-bounded-session-runtime/`

The later `docs: accept bounded session runtime lifecycle` commit contains
this report, so the report cannot contain that commit's own SHA. The exact
self-containing branch tip and post-commit clean status are recorded in the
external `D:\PythOS-Workspace\CURRENT-STATE.md` checkpoint after the commit.

ADR 0089 remains the semantic authority. ADR 0091 records evidence and the
bounded lifecycle boundary; it does not define new Viewing behavior or alter
the PythTIG ABI.

## CI contract TDD

Before editing `.github/workflows/qemu-acceptance.yml`:

```text
py -3 -m unittest tests.test_ci_workflow
Ran 5 tests
FAILED (failures=1)
AssertionError: 0 != 1 : milestone must run exactly once: cargo test -p pythos-user-session-runtime
```

After the workflow edit:

```text
py -3 -m unittest tests.test_ci_workflow
Ran 5 tests
OK
```

The exact-line contract now requires, once in `milestone_acceptance`:

```text
cargo test -p pythos-user-session-runtime
cargo test -p pythos-core session_runtime
cargo test -p pythos-core pyth_service_supervisor
python scripts/build-session-runtime.py --target-dir target/session-runtime-probe
python scripts/verify-user-elf.py --elf target/session-runtime-probe/x86_64-unknown-none/debug/pythos-user-session-runtime
cargo clippy -p pythos-user-session-runtime --target x86_64-unknown-none -- -D warnings
cargo clippy -p pythos-core --target x86_64-unknown-none --features session-runtime-probe -- -D warnings
python -m py_compile scripts/qemu_probe_support.py scripts/build-session-runtime.py scripts/test-session-runtime-probe.py
python -m unittest tests.test_iso_image tests.test_boot_marker_contract tests.test_qemu_exit tests.test_qemu_boot_media tests.test_ci_workflow tests.test_build_orchestration tests.test_verify_user_elf tests.test_interface_compatibility_freeze tests.test_session_input_bridge_boundary tests.test_session_runtime_boundary
python scripts/test-session-input-bridge-probe.py --self-test
python scripts/test-session-runtime-probe.py --self-test
python scripts/test-session-input-bridge-probe.py
python scripts/test-session-runtime-probe.py
```

Both oracle self-tests precede both live oracles. The Slice 2 live oracle is
absent from `handoff_acceptance`. The pinned QEMU 11.1.1 download/hash/cache,
pinned OVMF, independent milestone/handoff jobs, and single aggregate
`qemu_acceptance` job remain unchanged.

## Fresh local gates

The following commands all exited zero:

```text
cargo fmt --check
cargo test --workspace
cargo clippy -p pythos-user-session-runtime --target x86_64-unknown-none -- -D warnings
cargo clippy -p pythos-core --target x86_64-unknown-none --features session-runtime-probe -- -D warnings
py -3 -m py_compile scripts/qemu_probe_support.py scripts/build-session-runtime.py scripts/test-session-runtime-probe.py
py -3 -m unittest tests.test_build_orchestration tests.test_ci_workflow tests.test_session_input_bridge_boundary tests.test_session_runtime_boundary
  Ran 54 tests
  OK
py -3 scripts/test-session-input-bridge-probe.py --self-test
  SESSION_INPUT_BRIDGE_ORACLE_SELF_TEST_OK
py -3 scripts/test-session-runtime-probe.py --self-test
  Ran 26 tests
  OK
  SESSION_RUNTIME_ORACLE_SELF_TEST_OK
```

The workspace run included 768 PythCore tests, 128 shared-library tests, and
the bounded session-runtime tests. Both strict Clippy profiles completed
without warnings.

The full Python discovery gate immediately before Task 9 passed 152/152 at the
accepted implementation checkpoint. Task 9 then added one CI contract test;
fresh full discovery passed 153/153, and the focused 54-test gate above also
includes and passes that new test.

## Fresh local two-boot Slice 2 evidence

Host QEMU:

```text
QEMU emulator version 11.0.50 (v11.0.0-12631-g54e84cdc7a)
C:\Program Files\qemu\qemu-system-x86_64.exe
```

Command and terminal result:

```text
py -3 scripts/test-session-runtime-probe.py
SESSION_RUNTIME_PROBE_OK
```

Boot 1:

```text
QEMU_OUTCOME success
SESSION_RUNTIME_BOOT_1_PROCESS_TREE_REAPED
```

Boot 2:

```text
QEMU_OUTCOME success
SESSION_RUNTIME_BOOT_2_PROCESS_TREE_REAPED
```

Storage checkpoints:

```text
SESSION_RUNTIME_IMAGE_INITIAL size=16777216 sha256=080acf35a507ac9849cfcba47dc2ad83e01b75663a516279c8b9d243b719643e
SESSION_RUNTIME_IMAGE_AFTER_BOOT_1 size=16777216 sha256=080acf35a507ac9849cfcba47dc2ad83e01b75663a516279c8b9d243b719643e
SESSION_RUNTIME_IMAGE_AFTER_BOOT_2 size=16777216 sha256=080acf35a507ac9849cfcba47dc2ad83e01b75663a516279c8b9d243b719643e
```

COM1 artifacts:

```text
target/session-runtime-probe/session-runtime-boot-1-com1.log
  size 1573
  SHA-256 4863AFE1A482286A54CACA7C6072DBFE5668E7D11758B5A0E11FD9DFE464F4F1
target/session-runtime-probe/session-runtime-boot-2-com1.log
  size 1573
  SHA-256 4863AFE1A482286A54CACA7C6072DBFE5668E7D11758B5A0E11FD9DFE464F4F1
```

Each COM1 log contains the exact bounded lifecycle acceptance markers through:

```text
PYTHOS:CORE:SESSION_RUNTIME:COM2_READY
PYTHOS:CORE:SESSION_RUNTIME:AUTHORITY_CREATED
PYTHOS:CORE:SESSION_RUNTIME:IDENTITIES_VALID
PYTHOS:CORE:SESSION_RUNTIME:STREAM_BOUND
PYTHOS:CORE:SESSION_RUNTIME:PS2_READY
PYTHOS:CORE:SESSION_RUNTIME:RING3_ENTER
PYTHOS:CORE:PS2:KEYBOARD_IRQ_FIRED
PYTHOS:CORE:PS2:MOUSE_IRQ_FIRED
PYTHOS:CORE:USER_MODE:RETURN
PYTHOS:CORE:SESSION_RUNTIME:RING3_RETURN
PYTHOS:CORE:SESSION_RUNTIME:INVOCATION_1_VALID
PYTHOS:CORE:SESSION_RUNTIME:REINVOKE_VALID
PYTHOS:CORE:SESSION_RUNTIME:INVOCATION_2_VALID
PYTHOS:CORE:SESSION_RUNTIME:STATE_RETENTION_VALID
PYTHOS:CORE:SESSION_RUNTIME:NO_DISK_WRITES
PYTHOS:CORE:SESSION_RUNTIME:READY
```

COM2 boot 1 artifact and captured transcript:

```text
target/session-runtime-probe/session-runtime-boot-1-com2.log
size 1102
SHA-256 0D185C2E936796449852BE48CB5B0EC5C104F0C6FD88CF31201EC5ACE507C66A

PYTHOS:SESSION_RUNTIME:BOOT_STATE_0
PYTHOS:SESSION_RUNTIME:READY_FOR_EVENT_1
PYTHOS:SESSION_RUNTIME:EVENT_1_KEY_A_SEQUENCE_0
PYTHOS:SESSION_RUNTIME:COMMAND_1_SLICE2_ONE
PYTHOS:SESSION_RUNTIME:RESULT_1_SLICE2_ONE
PYTHOS:SESSION_RUNTIME:INVOCATION_1_EXIT_OK
PYTHOS:SESSION_RUNTIME:STATE_INPUTS_1_INVOCATIONS_1
PYTHOS:SESSION_RUNTIME:INVOCATION_LOCAL_RESET
PYTHOS:SESSION_RUNTIME:READY_FOR_EVENT_2
PYTHOS:SESSION_RUNTIME:EVENT_2_RELATIVE_MOTION_DX_7_DY_NEG_7_SEQUENCE_1
PYTHOS:SESSION_RUNTIME:COMMAND_2_SLICE2_TWO
PYTHOS:SESSION_RUNTIME:RESULT_2_SLICE2_TWO
PYTHOS:SESSION_RUNTIME:INVOCATION_2_EXIT_OK
PYTHOS:SESSION_RUNTIME:STATE_INPUTS_2_INVOCATIONS_2
PYTHOS:SESSION_RUNTIME:INPUT_CONTIGUOUS
PYTHOS:SESSION_RUNTIME:SESSION_ID_STABLE
PYTHOS:SESSION_RUNTIME:READY
```

COM2 boot 2 artifact and captured transcript:

```text
target/session-runtime-probe/session-runtime-boot-2-com2.log
size 1102
SHA-256 0D185C2E936796449852BE48CB5B0EC5C104F0C6FD88CF31201EC5ACE507C66A

PYTHOS:SESSION_RUNTIME:BOOT_STATE_0
PYTHOS:SESSION_RUNTIME:READY_FOR_EVENT_1
PYTHOS:SESSION_RUNTIME:EVENT_1_KEY_A_SEQUENCE_0
PYTHOS:SESSION_RUNTIME:COMMAND_1_SLICE2_ONE
PYTHOS:SESSION_RUNTIME:RESULT_1_SLICE2_ONE
PYTHOS:SESSION_RUNTIME:INVOCATION_1_EXIT_OK
PYTHOS:SESSION_RUNTIME:STATE_INPUTS_1_INVOCATIONS_1
PYTHOS:SESSION_RUNTIME:INVOCATION_LOCAL_RESET
PYTHOS:SESSION_RUNTIME:READY_FOR_EVENT_2
PYTHOS:SESSION_RUNTIME:EVENT_2_RELATIVE_MOTION_DX_7_DY_NEG_7_SEQUENCE_1
PYTHOS:SESSION_RUNTIME:COMMAND_2_SLICE2_TWO
PYTHOS:SESSION_RUNTIME:RESULT_2_SLICE2_TWO
PYTHOS:SESSION_RUNTIME:INVOCATION_2_EXIT_OK
PYTHOS:SESSION_RUNTIME:STATE_INPUTS_2_INVOCATIONS_2
PYTHOS:SESSION_RUNTIME:INPUT_CONTIGUOUS
PYTHOS:SESSION_RUNTIME:SESSION_ID_STABLE
PYTHOS:SESSION_RUNTIME:READY
```

Additional disposable artifacts:

```text
target/session-runtime-probe/session-runtime-store.img
  size 16777216
  SHA-256 080ACF35A507AC9849CFCBA47DC2AD83E01B75663A516279C8B9D243B719643E
target/session-runtime-probe/session-runtime-boot-1-com1-esp.img
  size 16777216
  SHA-256 D783E97441A3F7DF48AC3DC17435DA74EC8422C970ABCC0EA16F3800FF87D8E3
target/session-runtime-probe/session-runtime-boot-2-com1-esp.img
  size 16777216
  SHA-256 D783E97441A3F7DF48AC3DC17435DA74EC8422C970ABCC0EA16F3800FF87D8E3
```

## Independent Windows/QEMU evidence

JacesPC independently checked out implementation checkpoint `6109425` and ran
the exact two-boot oracle with QEMU 11.1.0. Both boots reached
`SESSION_RUNTIME_PROBE_OK` with exactly one `QEMU_OUTCOME success`; both
process trees were reaped; both COM2 transcripts began
`PYTHOS:SESSION_RUNTIME:BOOT_STATE_0`; and the 16,777,216-byte storage image
retained SHA-256
`080ACF35A507AC9849CFCBA47DC2AD83E01B75663A516279C8B9D243B719643E`
before, between, and after the boots.

GitHub Actions is pinned to QEMU 11.1.1. Hosted acceptance of the later Task 9
commit is not claimed by this report before that workflow actually passes.

## Regression gates

These stages ran in the planned order without rebuilding or restarting an
already-green independent stage:

```text
py -3 scripts/test-session-input-bridge-probe.py
  SESSION_INPUT_BRIDGE_PROBE_OK
  QEMU_OUTCOME success
py -3 scripts/test-normal-fast-boot.py
  NORMAL_FAST_BOOT_TEST_OK
py -3 scripts/test-persistent-storage.py
  PERSISTENT_STORAGE_TEST_OK
  QEMU_OUTCOME success
```

## Accepted claims

- One separately named ring-3 `session-runtime.elf` is entered once per boot
  under one stable session `ServiceId`.
- The ADR 0090 input capability is granted and bound once; two exact normalized
  events are consumed contiguously.
- Two immutable, independent typed `CREATE_NOTE` commands are read through the
  exact session-command resource `0x5059_5345_5343_4D44` with `READ | APPEND`
  rights; neither comes from physical input.
- Two fresh in-process interpreter invocations of unchanged
  `session-manager.tig` return successfully.
- Invocation-local interpreter value, host-result, command/result, and exit
  state reset before invocation 2 while neutral session state advances from
  zero to one to two.
- Session state begins at zero again on boot 2; no reboot durability is claimed.
- Shared lifecycle policy reinvokes after a clean exit and requests recovery
  after failure or an unknown exit.
- PythCore restores its root, clears the active caller, and validates the
  terminal result before acceptance.
- The bounded result page and one expected ring-3 `int3` return are
  acceptance-only mechanisms.
- The bounded QEMU profile performed no storage writes.

## Explicit non-claims and stop boundary

- No default normal-boot cutover.
- No production persistent Session Manager or production wait/wakeup contract.
- No Viewing behavior, `SessionControlInterpreter` binding, traversal routing,
  cursor activation/deactivation, FocusMark rendering, or presentation change.
- No PythTIG ABI expansion.
- No USB/xHCI production input integration and no removable-media write.
- No new physical Lenovo validation or physical-hardware acceptance.
- No click, double-click, drag, scroll, or zoom semantics.

Stop at Phase 13.5 Slice 2. Slice 3 is separately invoked and would bind ADR
0089 `SessionControlInterpreter` and session-lifetime `ViewingState` to this
retained owner.
