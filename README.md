# PythOS

PythOS is a from-scratch, verification-driven x86-64 operating system. It
boots through UEFI, runs a capability-controlled ring-3 object shell, persists
typed and versioned objects across QEMU reboots, includes polling AHCI and
SDHCI/eMMC block backends verified in QEMU, and carries the accepted PythTIG
version 1 graph-package direction through Phase 7 cutover/cross-target
evidence.

Current `main` is stopped at the Phase 12 slice 1 -> slice 2 boundary. Phase 12
`path-vs-graph-decision` is recorded by ADR 0069: PythOS uses a
capability-scoped object locator namespace, not POSIX paths. The accompanying
semantic-checkpoint contract records how later parallel evidence lanes must
compare artifact digests, normalized markers, object graph state, capability
transcripts, denials, and storage/locator state before merge.

The SDHCI/eMMC backend and evidence terminal have target-specific physical
evidence on the disposable O2 Micro `1217:8620` laptop. The 2026-08-08 physical
capture shows five terminal pages with `count 00000139` (hexadecimal, 313
decimal markers), `drop 00000000`, and CRC `176F4C6E`; two separate physical
boots reproduced the same count, zero-drop state, and CRC. This is scoped
physical evidence, not a generic hardware-support claim.

Start with [docs/TECHNICAL-OVERVIEW.md](docs/TECHNICAL-OVERVIEW.md) for the
current external-facing account of what the repository proves, how those claims
are verified, and what is not yet claimed.

Current-state references:

- [Technical overview](docs/TECHNICAL-OVERVIEW.md)
- [Handover](docs/HANDOVER.md)
- [Phase 12 roadmap](docs/ROADMAP-LATER-PHASES.md)
- [ADR 0069: object locator namespace and semantic checkpoints](docs/decisions/0069-phase-12-object-locator-and-semantic-checkpoints.md)
- [Semantic checkpoint contract](docs/semantic-checkpoint-contract.md)
- [PythTIG acceptance](docs/pyth-tig/ACCEPTANCE.md)

Physical evidence records:

- [Phase 10 physical SDHCI/eMMC backend](docs/milestones/2026-08-01-physical-emmc-phase10.md)
- [Milestone 1 physical evidence terminal validation](docs/evidence/2026-08-08-physical-evidence-terminal.md)

Public milestone site:
[https://craigcoda.github.io/pythos/](https://craigcoda.github.io/pythos/)

Milestone release:
[PythOS Milestone 1: Physical Persistent Object Storage](https://github.com/craigCODA/pythos/releases/tag/milestone-1-physical-storage).
