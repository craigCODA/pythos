# PythOS

PythOS is a from-scratch, verification-driven x86-64 operating system. It
boots through UEFI, runs a capability-controlled ring-3 object shell, persists
typed and versioned objects across QEMU reboots, includes polling AHCI and
SDHCI/eMMC block backends verified in QEMU, and carries the accepted PythTIG
version 1 graph-package direction through Phase 7 cutover/cross-target
evidence.

Current `main` is stopped at the Phase 13 -> Phase 13.5 boundary. Phase 12
`path-vs-graph-decision` is recorded by ADR 0069: PythOS uses a
capability-scoped object locator namespace, not POSIX paths. ADR 0070 records
the internal `object-locator 0.1` resolver ABI, ADR 0071 records the finite
loader read-bound extension needed for the debug acceptance image, and ADR 0072
records the path adversarial suite through `PYTHOS:CORE:PHASE_12_COMPLETE`.
ADR 0073 records the local package lifecycle and schema-extensibility proof
through `PYTHOS:CORE:PHASE_13_COMPLETE`. The accompanying semantic-checkpoint
contract records how later parallel evidence lanes must compare artifact
digests, normalized markers, object graph state, capability transcripts,
denials, and storage/locator state before merge.

The SDHCI/eMMC backend and evidence terminal have target-specific physical
evidence on the disposable O2 Micro `1217:8620` laptop. The 2026-08-08 physical
capture shows five terminal pages with `count 00000139` (hexadecimal, 313
decimal markers), `drop 00000000`, and CRC `176F4C6E`; two separate physical
boots reproduced the same count, zero-drop state, and CRC. This is scoped
physical evidence, not a generic hardware-support claim.

ADR 0074 adds an opt-in physical wake diagnostic. It is QEMU-accepted through
`scripts/test-physical-wake-diagnostic.py` and has one operator-reported
physical acceptance on the current USB boot machine: after the wake screen, the
diagnostic accepted `wake` plus Enter from the physical keyboard. This proves
only that diagnostic polling path on that machine, not USB HID, trackpad input,
IRQ-driven input, shell keyboard control, or generic PC input support.

Start with [docs/TECHNICAL-OVERVIEW.md](docs/TECHNICAL-OVERVIEW.md) for the
current external-facing account of what the repository proves, how those claims
are verified, and what is not yet claimed.

Current-state references:

- [Technical overview](docs/TECHNICAL-OVERVIEW.md)
- [Handover](docs/HANDOVER.md)
- [Phase 12 roadmap](docs/ROADMAP-LATER-PHASES.md)
- [ADR 0069: object locator namespace and semantic checkpoints](docs/decisions/0069-phase-12-object-locator-and-semantic-checkpoints.md)
- [ADR 0070: object locator resolution ABI](docs/decisions/0070-phase-12-object-locator-resolution-abi.md)
- [ADR 0072: path adversarial suite](docs/decisions/0072-phase-12-path-adversarial-suite.md)
- [ADR 0073: package lifecycle and schema extensibility](docs/decisions/0073-phase-13-package-lifecycle-and-schema-extensibility.md)
- [ADR 0074: physical wake diagnostic gate](docs/decisions/0074-physical-wake-diagnostic.md)
- [Semantic checkpoint contract](docs/semantic-checkpoint-contract.md)
- [PythTIG acceptance](docs/pyth-tig/ACCEPTANCE.md)

Physical evidence records:

- [Phase 10 physical SDHCI/eMMC backend](docs/milestones/2026-08-01-physical-emmc-phase10.md)
- [Milestone 1 physical evidence terminal validation](docs/evidence/2026-08-08-physical-evidence-terminal.md)
- [ADR 0074 physical wake diagnostic gate](docs/decisions/0074-physical-wake-diagnostic.md)

Public milestone site:
[https://craigcoda.github.io/pythos/](https://craigcoda.github.io/pythos/)

Milestone release:
[PythOS Milestone 1: Physical Persistent Object Storage](https://github.com/craigCODA/pythos/releases/tag/milestone-1-physical-storage).
