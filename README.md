# PythOS

PythOS is an x86-64 anti-desktop operating environment. It boots through UEFI
into PythCore and presents a world a person can author. Typed objects, tasks,
revisions, and explicit relationships describe the work. The vault holds
their durable state and evidence. PythCore enforces the capabilities that
authorize effects.

Kai is the work session PythOS is building toward. The opt-in Kai boot profile
resumes a bounded task selected by an NVMe vault root after PythCore takes
control. Start with the [agent instructions](AGENTS.md), the [current Kai
handover](docs/HANDOVER.md#current-kai-checkpoint), and the [roadmap](docs/ROADMAP.md).

## Native design

Reject POSIX and Unix as sources of the runtime, interaction, authority, and
relationships. Derive identity, naming, traversal, execution, and service
lifetime from PythOS's typed objects, tasks, and capabilities. If a slice starts
depending on an inherited model, stop and reassess the operation. [ADR 0121
on Kai's interaction model](docs/decisions/0121-kai-native-interaction.md)

A protected PythOS task evaluates the admitted vault program and proposes a
transition. PythCore admits the storage, validates the result, and publishes
the checkpoint. The native renderer projects validated records. The capability
check governs an action even when the task and its relationships are visible.
[System architecture](docs/PythOS-SAS-001.md) ·
[Boot handoff](docs/PythOS-TDD-001.md) ·
[PythTIG package ABI](docs/decisions/0065-pyth-graph-package-abi.md)

## Verified in local QEMU

- **Vault continuation.** PythCore recovered the last committed checkpoint
  from a Virtio image. The native evaluator finished the interrupted task,
  and replay returned its original receipt.
  [Vault continuation proof](docs/decisions/0110-vault-continuation-qemu-proof.md)
- **Kai from NVMe.** The same ISO ran two different tasks selected by their
  NVMe roots. After a cut during publication, the restarted task resumed
  from the last committed continuation and finished with one receipt.
  Replaying the event left the image unchanged.
  [Kai vault boot proof](docs/decisions/0120-kai-vault-boot-root.md)
- **Native world and PythTIG presentation.** Edited objects and their
  bindings survived a restart. A PythTIG-authored scene animated during drag
  and save, and the native renderer restored its committed placement at boot.
  [Editable world proof](docs/decisions/0114-native-editable-world.md) ·
  [PythTIG presentation proof](docs/decisions/0118-native-graph-window.md)

Physical Kai boot requires root provisioning and hardware acceptance. The
resident Kai session and a firmware-stage graph are design work. The next
interaction candidate is one capability-checked action on a saved task
reached through a typed relationship in the authored world.

## Run locally

Run from the repository root with Python 3, Rust via rustup, QEMU
`qemu-system-x86_64`, and OVMF firmware. Set `PYTHOS_OVMF_CODE` to the OVMF
code image if the runner cannot locate it. On Windows, `py -3` can substitute
for `python`.

```text
python -B scripts/test-vault-pulse.py
python -B scripts/test-kai-vault-boot.py
python -B scripts/test-world-session.py
python -B scripts/test-graph-window.py
```

The harnesses build the opt-in profiles and use disposable QEMU disk images.
The Kai boot check prints `PYTHOS_KAI_VAULT_BOOT_OK` after verifying the serial
sequence and committed image.

GitHub Actions runs the milestone and handoff suites on Ubuntu 24.04.
The handoff job includes the native Kai and spatial world proofs.
[QEMU Acceptance workflow](.github/workflows/qemu-acceptance.yml)

To try the native PythTIG presentation interactively, drag the title bar and
press **0** inside PythOS to exit. Reuse the output directory to restore the
placement from its emulated NVMe disk.

```text
python -B scripts/build-graph-window.py target/kai-world/native/graph-window-play --play
```

## Architecture and earlier evidence

- [Technical overview](docs/TECHNICAL-OVERVIEW.md)
- [PythTIG acceptance](docs/pyth-tig/ACCEPTANCE.md) and
  [threat model](docs/THREAT-MODEL.md)
- Phase 14 bounded secure-transport proof. [ADR 0104](docs/decisions/0104-phase-14-secure-transport-proof.md)
- Phase 15 bounded PCI observations in QEMU and on the Lenovo target.
  [Later-phase roadmap](docs/ROADMAP-LATER-PHASES.md#phase-15-hardware-driver-expansion) ·
  [Lenovo PCI capability metadata evidence](docs/evidence/2026-09-29-phase-15-pci-capability-metadata-snapshot.md)
- [Physical persistent object storage milestone](https://github.com/craigCODA/pythos/releases/tag/milestone-1-physical-storage)
