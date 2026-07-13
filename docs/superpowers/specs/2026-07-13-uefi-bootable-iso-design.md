# UEFI Bootable ISO Design

## Goal

Produce `target/pythos.iso`, a UEFI El Torito bootable ISO that boots the existing milestone-1 PythOS image in QEMU/OVMF and reaches `PYTHOS:CORE:MILESTONE_1_COMPLETE` over COM1 serial.

## Scope

This is packaging and test infrastructure only. It may touch `scripts/`, `tests/`, and `docs/`. It does not change the boot ABI, PythCore, loader behavior, runtime features, audio/video, Python services, AI, or the milestone-1 trusted core.

## Design

Add `scripts/build-iso.py`, a pure-Python ISO builder. Because `xorriso`, `oscdimg`, and `mcopy` are not available on this host, the script will build two binary structures directly:

1. a FAT16 EFI System Partition image containing `/EFI/BOOT/BOOTX64.EFI`, `/PYTHOS/PYTHCORE.ELF`, `/PYTHOS/INIT.PAK`, `/PYTHOS/BOOT.CFG`, and `/PYTHOS/FONT.PSF`;
2. an ISO9660 image with El Torito boot records pointing at that FAT16 ESP image as a no-emulation UEFI boot image.

Extend `scripts/run-qemu.py` with `--iso target/pythos.iso`, booted as a CD-ROM with OVMF while preserving the existing serial-log oracle.

Extend `scripts/test-boot.py` with `--media esp|iso`. The existing default remains `esp`; ISO testing builds `target/pythos.iso`, boots that ISO, and checks the same serial marker order.

## Acceptance

`python scripts/test-boot.py --slice milestone-1 --media iso` must boot QEMU/OVMF from `target/pythos.iso` and pass the existing serial marker assertion through `PYTHOS:CORE:MILESTONE_1_COMPLETE`.
