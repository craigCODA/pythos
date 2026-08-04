# ADR 0063: Physical Evidence Terminal

Date: 2026-08-02
Status: Accepted

## Implementation Status on Main

This ADR is accepted as design, and `main` carries the five 2026-08-02
evidence-terminal gallery frames. As of 2026-08-04, `main` does not contain the
`evidence_log.rs` or `evidence_terminal.rs` implementation files, an
`evidence-terminal` Cargo feature, or `scripts/test-evidence-terminal.py`.

The implementation remains on unmerged branch
`agent/physical-evidence-terminal`. That branch reports QEMU acceptance at
implementation commit `5e73e73` and treats physical validation as the next
step. The committed gallery frames are therefore retained as physical artifact
evidence, not as a reproducible acceptance path from `main`.

## Context

ADR 0062 proved the polling SDHCI/eMMC backend through QEMU acceptance and
through target-specific physical evidence on the disposable O2 Micro
`1217:8620` laptop. That physical target does not currently provide a captured
serial log. The existing framebuffer acceptance panel proves that the Phase 10
storage path reached its final physical screen, but it does not show the full
marker transcript in a form a reviewer can inspect from a photo or video.

PythOS still treats COM1 as the automated QEMU oracle. A screenshot remains
insufficient evidence by itself. The physical screen may mirror the marker
stream for serial-less hardware, but it must not replace serial ordering checks
or broaden the hardware claim.

## Decision

Add an opt-in `evidence-terminal` feature for verification images. The feature
renders a terminal-style marker transcript on the framebuffer after the Phase
10 storage proof reaches `PYTHOS:CORE:PHASE_10_COMPLETE`.

The boot ABI minor version moves from `0.2` to `0.3`. `PythBootInfo` consumes
part of the existing reserved area with explicit evidence-log metadata:

```rust
pub evidence_log_phys: u64,
pub evidence_log_len: u32,
pub evidence_log_flags: u32,
pub reserved: [u64; 6],
```

`evidence_log_flags` uses bit `0x0000_0001` as
`PYTH_EVIDENCE_LOG_FLAG_PRESENT`; all other bits are invalid. When the flag is
clear, `evidence_log_phys` and `evidence_log_len` must be zero. When the flag
is set, the evidence buffer must be page-aligned, loader-allocated RAM with a
total length of 64 KiB.

The shared log format is named `PYLOG001`, version `1`. Its payload stores
ASCII marker lines separated by `\n`. The header records payload capacity,
bytes used, accepted line count, dropped line count, and CRC-32/ISO-HDLC over
accepted payload bytes, including the trailing newline for each accepted line.
CRC-32/ISO-HDLC is defined as reflected polynomial `0xEDB88320`, initial state
`0xFFFF_FFFF`, reflected byte updates, and stored/displayed value
`state ^ 0xFFFF_FFFF`.

The loader allocates and initializes the evidence buffer before emitting
`PYTHOS:LOADER:ENTER`, then mirrors loader markers to both COM1 and the
evidence buffer. PythCore validates the buffer, maps it into the
PythCore-owned page tables before the broad loader identity map is removed,
backfills the earliest core markers that were emitted before attachment, then
mirrors subsequent core marker writes through the serial path.

The evidence terminal emits no replacement milestone markers. Existing marker
names and order remain intact. To let the QEMU screendump capture the terminal
after the final page is visible, the opt-in acceptance path emits the additional
post-render marker `PYTHOS:CORE:EVIDENCE_TERMINAL_READY` only when the evidence
log reports zero dropped lines. If `dropped` is nonzero, the terminal renders
with the nonzero drop count, emits `PYTHOS:CORE:EVIDENCE_TERMINAL_DROPPED`,
and enters the panic exit path. The evidence-terminal QEMU harness uses
`PYTHOS:CORE:EVIDENCE_TERMINAL_READY` as its success trigger while still
requiring `PYTHOS:CORE:MILESTONE_1_COMPLETE` to appear first in the transcript.

On physical hardware the QEMU debug-exit port write is ignored and
`qemu_exit::success()` remains in its non-returning loop, leaving the final
terminal page visible. Page dwell uses the existing PIT tick clock. Any CPU
spin fallback is calibrated and verified only for the O2 Micro `1217:8620`
target evidence image.

## Consequences

Default builds without `evidence-terminal` are unchanged. The feature creates
a visual mirror of the milestone marker stream for physical evidence, not a
trusted log store, filesystem, USB writer, object-store record, or replacement
serial oracle.

Evidence-terminal builds fail loudly if required evidence metadata is absent,
malformed, unmappable, or internally corrupt. If the transcript exceeds the
64 KiB buffer, the terminal renders with a nonzero `dropped` count and does not
emit the ready marker, so the artifact cannot be misread as a complete
transcript.

This ADR does not add USB mass storage, FAT, partitions, filesystems, DMA/ADMA,
interrupt-driven SDHCI, hotplug, or generic physical SDHCI/eMMC support. The
physical claim remains limited to the disposable O2 Micro `1217:8620` target
until another ADR records a new target and verification boundary.
