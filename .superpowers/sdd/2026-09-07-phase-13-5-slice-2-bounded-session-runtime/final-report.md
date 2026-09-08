# Phase 13.5 Slice 2 Final Evidence Report

Date: 2026-09-07

## Boundary and repository state

- Branch: `agent/phase13-5-session-runtime`
- Merge base: `c189b15438a3727f730d50bf9509999b44e05e44`
- Original bounded lifecycle evidence source:
  `6109425047de86cf60e4367313811fc284c43bad`
- Reviewed returnable-fault containment:
  `6dc1b4cfa47391472a7540375bd80fe7d7f8dd0c`
- Reviewed fault harness and milestone-only CI gate:
  `2441c6d442378609fb43a52c4e537c4cac1df740`
- Evidence-source status before this documentation follow-up: clean
- Documentation follow-up scope: ADR 0091, handover, and this report only
- Canonical report directory:
  `.superpowers/sdd/2026-09-07-phase-13-5-slice-2-bounded-session-runtime/`

The documentation commit containing this follow-up cannot contain its own
SHA. At evidence collection, the reviewed code tip was `2441c6d` and the
worktree was clean before these documentation-only edits.

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
python scripts/test-session-runtime-probe.py --fault-test
python scripts/test-session-runtime-probe.py
```

Both oracle self-tests precede all live oracles, and the fault proof precedes
the unchanged standard two-boot Slice 2 proof. The Slice 2 live oracles are
absent from `handoff_acceptance`, as are every other command in the protected
milestone-only list above. An adversarial test injects each protected command
into the handoff job independently and requires the validator to reject every
mutation. The pinned QEMU 11.1.1 download/hash/cache, pinned OVMF, independent
milestone/handoff jobs, and single aggregate `qemu_acceptance` job remain
unchanged.

The review-fix adversarial test first produced the intended RED against the
two-command-only handoff validator:

```text
py -3 -m unittest tests.test_ci_workflow
Ran 6 tests
FAILED (failures=11)
AssertionError: AssertionError not raised
```

The eleven failures corresponded to the eleven newly protected command
mutations; the two previously protected Slice 2 oracle commands already
failed closed. After the validator was generalized across the exact protected
command tuple:

```text
py -3 -m unittest tests.test_ci_workflow
Ran 6 tests
OK
```

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
  Ran 33 tests
  OK
  SESSION_RUNTIME_ORACLE_SELF_TEST_OK
```

The current workspace run included 773 PythCore tests, 128 shared-library
tests, and the bounded session-runtime tests. Both strict Clippy profiles
completed without warnings.

The full Python discovery gate immediately before Task 9 passed 152/152 at the
accepted implementation checkpoint. Initial Task 9 added one CI contract test;
full discovery passed 153/153, and the focused 54-test gate above included it.
The review fix added one adversarial CI mutation test; the exact CI suite then
passed 6/6 and full discovery passed 154/154. The contained-fault review added
one boundary test; fresh full discovery at `2441c6d` passed 155/155. The final
focused gates also passed 33/33 harness self-tests, 14/14 Slice 2 boundary
tests, and 6/6 CI workflow tests.

## Fresh local returnable-fault evidence

The reviewed fault-containment implementation is commit `6dc1b4c`; the
reviewed harness and CI gate are commit `2441c6d`. The harness builds a
synthetic `session-runtime.elf` through the shared image-builder helper with
entry `0x0000000000400000` and bytes `UD2; HLT`, packages it under the existing
runtime identity, and performs one no-input boot.

Command and terminal result on local QEMU 11.0.50:

```text
py -3 scripts/test-session-runtime-probe.py --fault-test
SESSION_RUNTIME_FAULT_PROBE_OK
```

Exact contained-fault evidence:

```text
PYTHOS:CORE:SESSION_RUNTIME:FAULT_CONTAINED principal:50595352544D0001 vector:6 rip:0000000000400000 rsp:FFFFFFFF80084FF0 cr2:0000000000000000
PYTHOS:CORE:SESSION_RUNTIME:RECOVERY_REQUESTED
QEMU_OUTCOME success
SESSION_RUNTIME_FAULT_BOOT_PROCESS_TREE_REAPED
```

The RSP is the value recorded in this live run; it is not specified as a
fixed address. The transcript contains no panic, normal `USER_MODE:RETURN`,
`RING3_RETURN`, `PYTHOS:CORE:SESSION_RUNTIME:READY`, or COM2 session-runtime
output. Storage was unchanged across the single boot:

```text
SESSION_RUNTIME_FAULT_IMAGE_INITIAL size=16777216 sha256=080acf35a507ac9849cfcba47dc2ad83e01b75663a516279c8b9d243b719643e
SESSION_RUNTIME_FAULT_IMAGE_AFTER_BOOT size=16777216 sha256=080acf35a507ac9849cfcba47dc2ad83e01b75663a516279c8b9d243b719643e
```

Canonical disposable artifacts:

```text
target/session-runtime-probe/session-runtime-fault.elf
target/session-runtime-probe/session-runtime-fault-com1.log
target/session-runtime-probe/session-runtime-fault-com2.log
target/session-runtime-probe/session-runtime-fault-store.img
```

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
  SHA-256 BD1566B684490F12DEAB50960EA90C6364FB323053F526F0D902333DA77BDFEC
target/session-runtime-probe/session-runtime-boot-2-com1.log
  size 1573
  SHA-256 BD1566B684490F12DEAB50960EA90C6364FB323053F526F0D902333DA77BDFEC
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
  SHA-256 3C19CBC6236350D638031434361F6861AC80879DCCE17B9A552B72FD00C2B083
target/session-runtime-probe/session-runtime-boot-2-com1-esp.img
  size 16777216
  SHA-256 3C19CBC6236350D638031434361F6861AC80879DCCE17B9A552B72FD00C2B083
```

## Independent Windows/QEMU evidence

JacesPC verified the exact `2441c6d` Git bundle with SHA-256
`6A8604E5EE66CF66165E11F26F9CB7CC899CFF97C9500612C6404AF72B022FFC`.
At that exact checkpoint, formatting, all 33 harness self-tests, and the paired
boundary/CI suites at 20/20 passed.

QEMU 11.1.0 independently reproduced the native-fault boot with the exact
principal `50595352544D0001`, vector 6, RIP `0000000000400000`, CR2 zero, and
recovery outcome. It produced exactly one `QEMU_OUTCOME success`, reaped the
complete process tree, and retained the 16,777,216-byte storage image at
SHA-256
`080ACF35A507AC9849CFCBA47DC2AD83E01B75663A516279C8B9D243B719643E`.
JacesPC then ran the unchanged two-boot oracle. Each boot reached the exact
per-channel terminal markers with one success outcome; both process trees
were reaped; both COM2 transcripts began
`PYTHOS:SESSION_RUNTIME:BOOT_STATE_0`; the same storage hash remained unchanged
before, between, and after the boots; and the harness emitted
`SESSION_RUNTIME_PROBE_OK` once.

GitHub Actions is pinned to QEMU 11.1.1. Hosted acceptance of the reviewed
fault commits remains unproven until that workflow actually passes them.

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
- A CPL3 fault in the active returnable path returns a typed fault context
  after recording principal/vector/RIP/RSP/CR2, disarming the normal expected
  breakpoint, clearing the active caller, and restoring the kernel root.
- The session fault path accepts only the exact session principal after caller
  clearing and root restoration, then requests recovery without normal
  readiness; all transient returnable state is cleared on exit.
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
