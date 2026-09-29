# Task 5 Report: PCI Capability Snapshot CI and Status Synchronization

## Status

Completed locally. ADR 0109 remains **Proposed for owner review**. This task
adds only the required workflow gates, CI workflow contract entries, current
status links, and a QEMU-only evidence record. It does not alter Rust code,
QEMU runner behavior, ISO contents, or physical-hardware evidence.

## Changed files

- `.github/workflows/qemu-acceptance.yml`
  - Adds the capability-profile cargo test, strict clippy, Python compile, and
    unittest commands beside the existing 0105-0108 profile gates.
  - Adds the capability oracle self-test and live oracle at the end of the
    existing QEMU milestone network-hardware block.
- `tests/test_ci_workflow.py`
  - Freezes all six exact capability commands in
    `NETWORK_HARDWARE_MILESTONE_ONLY_COMMANDS`.
  - Requires each exactly once outside handoff and verifies that the capability
    oracle follows the 0105-0108 QEMU oracle sequence.
- `README.md`, `docs/HANDOVER.md`, `docs/ROADMAP.md`, and
  `docs/PythOS-TDD-001.md`
  - Record the implemented QEMU-only snapshot, retain the ADR proposed status,
    link the evidence record, and preserve the stated architecture and
    non-goal boundaries.
- `docs/evidence/2026-09-29-phase-15-pci-capability-metadata-snapshot.md`
  - New QEMU-only evidence record. It links ADR 0109, records the Task 4 QEMU
    outcomes, and explicitly leaves Lenovo `81VS` evidence pending.

The pre-existing untracked
`docs/superpowers/plans/2026-09-29-phase-15-pci-capability-metadata-snapshot.md`
was left untouched.

## Verification

### Task 5 focused gates

```text
cargo test -p pythos-core --no-default-features --features network-hardware-capability-probe
cargo clippy -p pythos-core --target x86_64-unknown-none --no-default-features --features network-hardware-capability-probe -- -D warnings
py -3 -m py_compile scripts/test-network-hardware-capability-probe.py tests/test_network_hardware_capability_probe.py
py -3 -m unittest tests.test_network_hardware_capability_probe
py -3 scripts/test-network-hardware-capability-probe.py --self-test
py -3 scripts/test-network-hardware-capability-probe.py
```

Result: exit 0. The Rust profile ran 915 tests successfully; strict clippy
passed. The focused Python suite passed 15 tests, the oracle self-test passed
12 tests, and the live oracle passed both isolated QEMU models with terminal
`NETWORK_HARDWARE_CAPABILITY_PROBE_TEST_OK`.

The exact workflow spellings using `python` were attempted first:

```text
python -m py_compile scripts/test-network-hardware-capability-probe.py tests/test_network_hardware_capability_probe.py
python -m unittest tests.test_network_hardware_capability_probe
python scripts/test-network-hardware-capability-probe.py --self-test
python scripts/test-network-hardware-capability-probe.py
```

Result: this Windows environment's `python` app-execution alias reported that
Python was not found. The installed `py -3` launcher ran the identical scripts
and arguments successfully as shown above.

### CI contract and whitespace

```text
py -3 -m unittest tests.test_ci_workflow
py -3 -m unittest tests.test_network_hardware_capability_probe
git diff --check
```

Result: exit 0; CI workflow contract passed 17 tests, capability contract
passed 15 tests, and no whitespace errors were reported.

The CI contract was intentionally run once after adding the expected strings
and before editing the workflow. It failed only because the new cargo test
command was missing, proving the added assertion was active; it passed after
the workflow update.

### Existing 0105-0108 local equivalents

The existing focused cargo-test and strict-clippy commands for
`network-hardware-probe`, `network-hardware-bar-probe`,
`network-hardware-register-probe`, and
`network-hardware-register-enable-probe` all exited 0. The combined first
sweep reached the local 120-second wrapper limit without a failing test, so the
remaining gates were rerun in smaller conclusive groups.

```text
py -3 -m py_compile scripts/test-network-hardware-probe.py tests/test_network_hardware_probe.py
py -3 -m py_compile scripts/test-network-hardware-bar-probe.py tests/test_network_hardware_bar_probe.py
py -3 -m py_compile scripts/test-network-hardware-register-probe.py tests/test_network_hardware_register_probe.py
py -3 -m py_compile scripts/test-network-hardware-register-enable-probe.py tests/test_network_hardware_register_enable_probe.py
```

Result: exit 0.

```text
py -3 -m unittest tests.test_network_hardware_probe
py -3 -m unittest tests.test_network_hardware_bar_probe
py -3 -m unittest tests.test_network_hardware_register_probe
py -3 -m unittest tests.test_network_hardware_register_enable_probe
```

Result: exit 0; 5, 11, 4, and 4 tests passed respectively.

```text
py -3 scripts/test-network-hardware-probe.py --self-test
py -3 scripts/test-network-hardware-bar-probe.py --self-test
py -3 scripts/test-network-hardware-register-probe.py --self-test
py -3 scripts/test-network-hardware-register-enable-probe.py --self-test
```

Result: exit 0; 7, 12, 6, and 5 self-tests passed respectively.

```text
py -3 scripts/test-network-hardware-probe.py
py -3 scripts/test-network-hardware-bar-probe.py
py -3 scripts/test-network-hardware-register-probe.py
py -3 scripts/test-network-hardware-register-enable-probe.py
```

Result: exit 0. Each oracle passed isolated QEMU `e1000` and `e1000e` cases
and ended with its corresponding `*_TEST_OK` marker.

## QEMU evidence

The new [QEMU-only evidence record](../../../docs/evidence/2026-09-29-phase-15-pci-capability-metadata-snapshot.md)
records Task 4's exact capability outcomes:

- `e1000`: successful absent-list observation.
- `e1000e`: successful four-entry PM/MSI/PCIe/MSI-X traversal.

Neither result interprets capability control fields or claims device readiness.
No Lenovo `81VS` evidence is recorded for ADR 0109; any future physical
observation remains separately gated under `F:\iso` and preserves existing
Ventoy/ISO contents.

## Self-review

- All six required literal commands appear once in the milestone workflow and
  once in the frozen CI contract tuple; no command was added to handoff.
- The capability slice follows the existing 0105-0108 network-hardware probes
  in every affected milestone step. No job, runner, backend, or gate was
  removed or relaxed.
- Current-status documents consistently retain `VirtioTransport`, transport
  adapter, and `NetworkPort`, retain ADR 0109's proposed status, and state the
  full read-only non-goal boundary without introducing `Driver` as a PythOS
  architectural noun.
- The evidence record is explicitly QEMU-only and makes no Lenovo or Realtek
  capability-meaning claim.

## Concerns

- The local Windows `python` command is an unavailable app-execution alias;
  `py -3` supplied the working equivalent used for all Python verification.
- `git status` emits pre-existing permission warnings for unrelated temporary
  directories. The only unrelated visible worktree item is the untracked plan
  noted above; it was not staged or modified.

## Fix Round 1

### Process-local `python` shim

The prior verification limitation was the Windows app-execution alias named
`python`, not the capability profile. For this verification only, the
PowerShell process defined the following temporary function before running any
requested command:

```powershell
function python { & py -3 @args }
```

The function exists only for that PowerShell process and forwards its argument
list to the installed `py -3` launcher. No PATH, app-execution-alias, workflow,
or project-code change was made. GitHub's Ubuntu runner resolves the workflow's
native `python` command; this local shim makes the Windows invocation use the
same command strings and arguments.

### Exact command results

```text
python -m py_compile scripts/test-network-hardware-capability-probe.py tests/test_network_hardware_capability_probe.py
python -m unittest tests.test_network_hardware_capability_probe
python scripts/test-network-hardware-capability-probe.py --self-test
python scripts/test-network-hardware-capability-probe.py
```

Result: all four commands exited 0 through the process-local shim. `py_compile`
produced no output. The capability contract suite reported `Ran 15 tests` and
`OK`; the oracle self-test reported `Ran 12 tests` and `OK`. The live oracle
rebuilt its isolated QEMU profile and passed both models:

```text
e1000:  PCI_CAPABILITY_LIST_ABSENT; QEMU_OUTCOME success
e1000e: PCI_CAPABILITY_LIST_PRESENT; PM/MSI/PCIe/MSI-X; QEMU_OUTCOME success
NETWORK_HARDWARE_CAPABILITY_PROBE_TEST_OK
```

The requested supporting checks also passed:

```text
python -m unittest tests.test_ci_workflow
git diff --check
```

Result: CI workflow contract reported `Ran 17 tests` and `OK`; `git diff
--check` exited 0 with no whitespace errors. This fix round is
verification/report-only: no workflow command, source file, evidence record,
or project behavior changed.
