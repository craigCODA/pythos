# ADR 0120 — Kai vault boot root on NVMe

Status: Bounded QEMU boot and recovery proof. The native interaction direction
is recorded in [ADR 0121](0121-kai-native-interaction.md).

The owner wants Kai to be the work session that starts when PythOS boots.
The vault supplies which work to resume. PythCore owns storage checks,
checkpoint publication, memory protection, and entry into the native
pulse evaluator.

## First slice

The opt-in `kai-vault-boot` profile enters the vault coordinator immediately
after the core substrate comes up. It opens the same identity-bound NVMe
partition as the hardware world build. The vault boot root supplies the
task ID, exact program digest, and first event. The existing bounded
vault evaluator runs in a protected PythOS address space and returns one
candidate at a time. It needs no host OS or language environment.

The native test record is the ADR 0110 typed link program. PythTIG v1 and
the UEFI handoff retain their current formats. A future boot-stage PythTIG
graph can select a root and pass a read-only candidate across the handoff.
PythCore still checks the admitted disk and authoritative generation.

## Partition records

Each record occupies one aligned 4 KiB page, with its typed data in the
first sector. The boot root begins at partition record 8. The earlier
world records keep their positions.

| Record | Contents |
| --- | --- |
| 8 | `PYKAIRT1`, little-endian u64 task ID, SHA-256 of the 64-byte program, initial ADR 0110 task event, and the interruption selector at byte 72 |
| 9 | ADR 0110 event mailbox |
| 10 | Immutable program |
| 11–14 | Existing alternating checkpoint body and commit records |

Bytes 73–511 of the root are zero. The interruption selector must be zero
on physical hardware. The disposable emulated identity allows the
ADR 0110 interruption modes. The driver verifies the exact target,
protective MBR, both GPTs, and world identity header before consulting the
root. The pulse store validates the program digest and rejects any valid
committed checkpoint with a different task, program, or initial event.
The storage path grants the
evaluator no raw disk access. Publication writes only records 11–14 and
flushes each update before readback.

This slice accepts a provisioned root. The acceptance harness creates it
on disposable images. A physical installation needs an explicit
provisioning path for the reserved records. On its hardware-target build,
the session view stays on screen when the bounded boot work settles.

## Acceptance

Boot the same ISO in QEMU with two different rooted programs. With an
empty event mailbox, each boot runs the task selected by its disk. Kill
one at the next uncommitted candidate and restart from the same image.
Check that exactly one receipt survives at completion. Check duplicate
delivery, a missing root, a changed program, and a checkpoint from a
different task. Compare bytes outside the checkpoint records.

Run the proof with `python -B scripts/test-kai-vault-boot.py`. The
booted records selected two different tasks using the same ISO. After
an interrupted generation 4 publication, the first task recovered
generation 3 at value 4 and finished at generation 7 with value 11.
The final record contained one receipt. Duplicate delivery and the
invalid roots left the disk unchanged.
