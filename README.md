# PythOS

PythOS is a from-scratch, verification-driven x86-64 operating system. It
boots through UEFI, runs a capability-controlled ring-3 object shell, persists
typed and versioned objects across QEMU reboots, and includes polling AHCI and
SDHCI/eMMC block backends verified in QEMU. The SDHCI/eMMC backend has
target-specific physical panel evidence on the disposable O2 Micro `1217:8620`
laptop. `main` also contains the evidence-terminal implementation and QEMU
acceptance harness. The 2026-08-08 physical capture shows five terminal pages
with `count 00000139` (hexadecimal, 313 decimal markers), `drop 00000000`, and
CRC `176F4C6E`; two separate physical boots reproduced the same count, zero-drop
state, and CRC.

Start with [docs/TECHNICAL-OVERVIEW.md](docs/TECHNICAL-OVERVIEW.md) for the
current external-facing account of what the repository proves, how those claims
are verified, and what is not yet claimed.

Physical evidence records:

- [Phase 10 physical SDHCI/eMMC backend](docs/milestones/2026-08-01-physical-emmc-phase10.md)
- [Milestone 1 physical evidence terminal validation](docs/evidence/2026-08-08-physical-evidence-terminal.md)

Public milestone site:
[https://craigcoda.github.io/pythos/](https://craigcoda.github.io/pythos/)

Milestone release:
[PythOS Milestone 1: Physical Persistent Object Storage](https://github.com/craigCODA/pythos/releases/tag/milestone-1-physical-storage).
