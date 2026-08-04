# Physical Evidence Terminal Design

Date: 2026-08-02

Status: Implemented and QEMU-accepted; physical O2 Micro `1217:8620`
evidence-terminal validation pending operator boot evidence.

## Goal

Add an opt-in physical evidence terminal that renders the full boot marker
transcript on the framebuffer after the Phase 10 storage proof completes. The
terminal exists for serial-less physical hardware: it makes the same milestone
marker stream visible in a photo or video while preserving COM1 as the primary
QEMU oracle.

The first target remains the already confirmed disposable O2 Micro `1217:8620`
SDHCI/eMMC laptop. This design does not add USB storage, FAT writes, partitions,
filesystems, DMA/ADMA, interrupts, hotplug, or generic SDHCI/eMMC support.

## User-Facing Behavior

When built with the evidence-terminal feature, the final physical screen becomes
a terminal-style transcript instead of the current five-line acceptance panel.
It renders pages like:

```text
PythOS Evidence Terminal
page 01/04 count 000000F2 drop 00000000 crc 8A31C04E
> PYTHOS:LOADER:ENTER
> PYTHOS:LOADER:GOP_READY
> PYTHOS:LOADER:KERNEL_LOADED
...
```

The renderer advances through all pages at a fixed bounded dwell so a phone
video can capture the full transcript. The dwell should use the existing PIT
tick clock because this screen renders after Phase 10, long after
`PYTHOS:CORE:TIMER_READY`. Any CPU spin-loop fallback is calibrated and verified
only for the O2 Micro `1217:8620` target evidence image; it is not portable to a
second physical target without new measurement. In QEMU, the same image still
exits through the existing success path after the terminal pass, so automated
acceptance remains deterministic. On physical hardware, the QEMU debug-exit
write is ignored and execution remains in an infinite halt/spin loop with the
final terminal page visible.

The terminal must include the loader markers and PythCore markers from the same
boot:

```text
PYTHOS:LOADER:ENTER
PYTHOS:LOADER:GOP_READY
PYTHOS:LOADER:KERNEL_LOADED
PYTHOS:LOADER:MEMORY_MAP_READY
PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK
...
PYTHOS:CORE:PHASE_10_COMPLETE
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

The terminal is a full milestone marker transcript. It is not a byte-for-byte
serial capture of arbitrary diagnostic text.

## Architecture

The loader owns the first part of the transcript because it emits
`PYTHOS:LOADER:*` before PythCore exists. PythCore owns the rest after handoff.

```text
UEFI firmware
-> BOOTX64.EFI
   -> allocate fixed evidence-log buffer
   -> mirror loader marker lines to COM1 and evidence buffer
   -> pass evidence buffer through PythBootInfo
-> PythCore
   -> validate evidence buffer metadata
   -> mirror core marker lines to COM1 and evidence buffer
   -> run normal verify milestone path
   -> render evidence terminal after Phase 10
   -> emit framebuffer/milestone completion markers
```

The implementation adds a small shared `evidence_log` format under the
shared crate so the boot loader and PythCore use the same header, checksum, and
append rules.

## Boot ABI

This design intentionally changes the boot ABI. It requires an ADR before code
changes.

The ABI should bump from minor `0.2` to `0.3`. The existing `PythBootInfo`
reserved area should be partially consumed rather than silently overloaded:

```rust
pub evidence_log_phys: u64,
pub evidence_log_len: u32,
pub evidence_log_flags: u32,
pub reserved: [u64; 6],
```

This keeps the structure compact while making the ABI explicit. PythCore must
reject malformed nonzero evidence metadata when the feature is active, and the
existing reserved-field validation must continue rejecting unknown future data.

The evidence buffer must be:

- page-aligned;
- RAM allocated by the loader before `ExitBootServices()`;
- bounded to a fixed size, initially 64 KiB;
- mapped by PythCore-owned page tables before the loader identity map is
  removed;
- treated as boot evidence only, not as trusted policy or security state.

## Log Format

The evidence buffer begins with a fixed header:

```text
magic      "PYLOG001"
version    1
capacity   payload bytes
used       bytes written
lines      accepted line count
dropped    line count lost to capacity/truncation
crc32      CRC-32/ISO-HDLC of accepted bytes
```

Payload bytes are ASCII marker lines separated by `\n`. Appending a line:

- accepts only bounded ASCII lines;
- appends `\n` after each line;
- increments the line count only when the line fits;
- increments `dropped` if the buffer is full;
- updates the CRC for every accepted byte, including `\n`;
- never allocates.

The checksum algorithm is CRC-32/ISO-HDLC: reflected polynomial `0xEDB88320`,
initial state `0xFFFF_FFFF`, reflected byte updates, and displayed/stored value
`state ^ 0xFFFF_FFFF`. Implementations may use a bitwise no-table update to
keep the shared format no-alloc and small.

The serial writer remains the source of visible boot progress. Evidence logging
is a mirror of the marker stream and must not block COM1 output.

## Terminal Renderer

The framebuffer terminal should be separate from the existing storage backend
panel builder. It should render at 8x8 boot-font scale 1 with stable margins,
fixed row height, and fixed columns calculated from the framebuffer dimensions.

Rendering rules:

- clear the screen to a dark terminal background;
- draw a title and status line at the top;
- show `page NN/MM`, line count, drop count, and CRC-32;
- draw transcript lines prefixed with `>`;
- wrap lines at the calculated terminal width;
- advance pages with a fixed PIT-tick dwell; if a CPU spin fallback is used, it
  must be documented as O2 Micro `1217:8620`-only evidence timing;
- render the final page last and leave it visible before calling
  `qemu_exit::success()`;
- emit `PYTHOS:CORE:EVIDENCE_TERMINAL_READY` only after the final page is
  rendered, then wait one bounded capture dwell before `qemu_exit::success()`
  so QEMU has a deterministic screendump window;
- after the success-port write, rely on `qemu_exit::success()`'s non-returning
  infinite loop so real hardware keeps showing the final framebuffer instead of
  falling through into undefined code.

The font table must include every character used by marker lines and terminal
chrome: uppercase letters, digits, colon, underscore, dash, slash, greater-than,
and spaces.

## Failure Behavior

Default builds without the evidence-terminal feature are unchanged.

For evidence-terminal builds:

- if the loader cannot allocate the evidence buffer, it still emits COM1
  markers but PythCore must fail the evidence-terminal acceptance path;
- if PythCore cannot validate or map the buffer, it emits a clear panic marker
  rather than rendering an incomplete success terminal;
- if the transcript overflows, the terminal still renders but reports nonzero
  `dropped` so the artifact cannot be mistaken for a complete transcript;
- storage proof failures continue to panic before the terminal renders.

## Verification

Required tests before physical deployment:

- shared-format unit tests for header initialization, append, checksum, capacity
  handling, and overflow/drop behavior;
- boot-protocol tests for ABI minor bump and evidence metadata validation;
- loader tests or host-testable helpers proving loader markers are appended in
  the same order as COM1 writes;
- core serial tests proving PythCore marker writes append to the existing loader
  transcript;
- terminal renderer tests for paging, glyph coverage, line wrapping/truncation,
  CRC/status formatting, dwell selection, and final-page selection;
- QEMU verify acceptance with `verify,sdhci-emmc-backend,evidence-terminal`
  proving the image still reaches `PYTHOS:CORE:MILESTONE_1_COMPLETE`, renders
  the terminal, emits `PYTHOS:CORE:EVIDENCE_TERMINAL_READY`, and exits with
  `QEMU_OUTCOME success`;
- QEMU serial assertions that the terminal path does not rename or reorder any
  existing milestone markers.

Physical evidence remains target-specific. A successful video of the terminal on
the O2 Micro `1217:8620` laptop proves that one serial-less physical target
rendered the complete marker transcript after the Phase 10 storage path. It
does not prove USB logging, generic physical storage support, or any filesystem.

## QEMU Acceptance Record

Recorded on branch `agent/physical-evidence-terminal` at implementation commit
`5e73e73` before documentation sync:

```powershell
python scripts/test-evidence-terminal.py
```

Successful output included:

```text
PYTHOS:CORE:EVIDENCE_TERMINAL_READY
QEMU_OUTCOME success
EVIDENCE_TERMINAL_TEST_OK
```

The acceptance run required the PPM framebuffer dump at
`target/evidence-terminal.ppm` to match the evidence-terminal frame palette and
expected title/status/body row glyph structure; manual inspection showed
terminal page `05/05` with `count 00000138`, `drop 00000000`, CRC `734FF002`,
and final markers through
`PYTHOS:CORE:MILESTONE_1_COMPLETE`.

## Out Of Scope

- Writing a log file to the boot USB.
- Implementing xHCI, USB mass storage, SCSI, FAT, or partition parsing.
- Persisting the log as an object-store record.
- Replacing COM1 as the QEMU acceptance oracle.
- Broadening physical support beyond the named disposable SDHCI/eMMC target.
