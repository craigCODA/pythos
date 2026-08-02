# PythOS

[![QEMU Acceptance](https://github.com/craigCODA/pythos/actions/workflows/qemu-acceptance.yml/badge.svg?branch=main)](https://github.com/craigCODA/pythos/actions/workflows/qemu-acceptance.yml?query=branch%3Amain)

PythOS is a from-scratch, verification-driven x86-64 operating system. It
boots through UEFI, runs a capability-controlled ring-3 object shell, persists
typed and versioned objects across QEMU reboots, and includes polling AHCI and
SDHCI/eMMC block backends verified in QEMU. The SDHCI/eMMC backend has also
reached its Phase 10 physical acceptance panel across two cold-boot runs on the
disposable O2 Micro `1217:8620` laptop. That is target-specific evidence, not a
generic SDHCI/eMMC support claim.

Start with [docs/TECHNICAL-OVERVIEW.md](docs/TECHNICAL-OVERVIEW.md) for the
current external-facing account of what the repository proves, how those claims
are verified, and what is not yet claimed.

Current physical-evidence record:
[docs/milestones/2026-08-01-physical-emmc-phase10.md](docs/milestones/2026-08-01-physical-emmc-phase10.md).

Public milestone site:
[https://craigcoda.github.io/pythos/](https://craigcoda.github.io/pythos/)

Milestone release:
[PythOS Milestone 1: Physical Persistent Object Storage](https://github.com/craigCODA/pythos/releases/tag/milestone-1-physical-storage).
